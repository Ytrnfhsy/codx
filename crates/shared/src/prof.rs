//! A light profiler for the client's frames and the server's ticks: how long each part
//! takes (`scope`) and how big some things are (`value`), frame by frame, summed up as
//! p50 / p99 / max at the end (a hitch matters more than the average).
//!
//! Off unless `enable`d: a scope then costs one check of a flag. Each thread keeps its own
//! numbers, so a server hosted inside the client does not mix with the client's frames.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static ON: AtomicBool = AtomicBool::new(false);

/// What one thread measured: per name, one number per frame (or tick).
#[derive(Default)]
struct Book {
    /// Names in the order they first came up (the report keeps it).
    order: Vec<&'static str>,
    /// Per name: the frame's sum so far, and every finished frame's.
    series: Vec<(f64, Vec<f32>)>,
    /// Whether a name is a time (ms) or a value.
    timed: Vec<bool>,
    frames: usize,
}

impl Book {
    fn slot(&mut self, name: &'static str, timed: bool) -> usize {
        match self.order.iter().position(|n| *n == name) {
            Some(i) => i,
            None => {
                self.order.push(name);
                // (frames before this name first came up count as nothing)
                self.series.push((0.0, vec![0.0; self.frames]));
                self.timed.push(timed);
                self.order.len() - 1
            }
        }
    }
}

thread_local! {
    static BOOK: RefCell<Book> = RefCell::new(Book::default());
}

/// Starts measuring (on every thread).
pub fn enable() {
    ON.store(true, Ordering::Relaxed);
}

/// Stops measuring (what was measured stays for the report).
pub fn disable() {
    ON.store(false, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Measures from now until the returned guard is dropped; several scopes of one name in
/// one frame add up.
#[must_use]
pub fn scope(name: &'static str) -> Scope {
    Scope(enabled().then(|| (name, Instant::now())))
}

pub struct Scope(Option<(&'static str, Instant)>);

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some((name, t)) = self.0.take() {
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            BOOK.with(|b| {
                let mut b = b.borrow_mut();
                let i = b.slot(name, true);
                b.series[i].0 += ms;
            });
        }
    }
}

/// Adds `v` to this frame's `name` (bytes sent, things counted…).
pub fn value(name: &'static str, v: f64) {
    if !enabled() {
        return;
    }
    BOOK.with(|b| {
        let mut b = b.borrow_mut();
        let i = b.slot(name, false);
        b.series[i].0 += v;
    });
}

/// Adds a time measured elsewhere (ms) to this frame's `name`.
pub fn time(name: &'static str, ms: f64) {
    if !enabled() {
        return;
    }
    BOOK.with(|b| {
        let mut b = b.borrow_mut();
        let i = b.slot(name, true);
        b.series[i].0 += ms;
    });
}

/// Closes this thread's frame (or tick): every name gets its sum, zero if it did not
/// come up.
pub fn frame_end() {
    if !enabled() {
        return;
    }
    BOOK.with(|b| {
        let mut b = b.borrow_mut();
        b.frames += 1;
        for s in &mut b.series {
            s.1.push(s.0 as f32);
            s.0 = 0.0;
        }
    });
}

/// Forgets what this thread measured so far (after a warm-up).
pub fn reset() {
    BOOK.with(|b| *b.borrow_mut() = Book::default());
}

/// One line of a report.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub name: &'static str,
    pub timed: bool,
    pub mean: f64,
    pub p50: f64,
    pub p99: f64,
    pub max: f64,
}

/// The sorted value at fraction `q` (0..1) of `v`.
pub fn percentile(sorted: &[f32], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let k = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[k.min(sorted.len() - 1)] as f64
}

/// What this thread measured, per name, over its finished frames.
pub fn lines() -> (usize, Vec<Line>) {
    BOOK.with(|b| {
        let b = b.borrow();
        let lines = b
            .order
            .iter()
            .zip(&b.series)
            .zip(&b.timed)
            .map(|((name, (_, v)), timed)| {
                let mut s = v.clone();
                s.sort_by(f32::total_cmp);
                Line {
                    name,
                    timed: *timed,
                    mean: s.iter().map(|x| *x as f64).sum::<f64>() / s.len().max(1) as f64,
                    p50: percentile(&s, 0.5),
                    p99: percentile(&s, 0.99),
                    max: s.last().copied().unwrap_or(0.0) as f64,
                }
            })
            .collect();
        (b.frames, lines)
    })
}

/// The frames' values of one name (to work more out of them, like the 1% low).
pub fn series(name: &str) -> Vec<f32> {
    BOOK.with(|b| {
        let b = b.borrow();
        b.order
            .iter()
            .position(|n| *n == name)
            .map(|i| b.series[i].1.clone())
            .unwrap_or_default()
    })
}

/// A Markdown table of `lines` (times in ms, values as they are).
pub fn table(lines: &[Line]) -> String {
    let mut out = String::from("| | mean | p50 | p99 | max |\n|---|---:|---:|---:|---:|\n");
    for l in lines {
        let f = |x: f64| {
            if l.timed {
                format!("{x:.3} ms")
            } else {
                format!("{x:.0}")
            }
        };
        out += &format!(
            "| {} | {} | {} | {} | {} |\n",
            l.name,
            f(l.mean),
            f(l.p50),
            f(l.p99),
            f(l.max)
        );
    }
    out
}

/// This process's resident memory (MB), where the system tells it (Linux).
pub fn resident_mb() -> Option<f64> {
    let s = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: f64 = s.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096.0 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_sum_their_scopes_and_report_percentiles() {
        enable();
        reset();
        for f in 0..100 {
            value("bytes", 10.0);
            if f == 99 {
                // one big frame: the maximum and the 99th percentile see it, the median not
                value("bytes", 990.0);
            }
            {
                let _s = scope("work");
            }
            frame_end();
        }
        let (frames, lines) = lines();
        assert_eq!(frames, 100);
        let b = lines.iter().find(|l| l.name == "bytes").unwrap();
        assert!(!b.timed);
        assert_eq!((b.p50, b.max), (10.0, 1000.0));
        assert!((b.mean - 19.9).abs() < 1e-6);
        assert!(lines.iter().any(|l| l.name == "work" && l.timed));
        assert_eq!(series("bytes").len(), 100);
        assert!(table(&lines).contains("| bytes | 20 | 10 |"));
        reset();
    }

    #[test]
    fn a_name_that_comes_up_late_counts_zero_before() {
        enable();
        reset();
        frame_end();
        value("late", 5.0);
        frame_end();
        assert_eq!(series("late"), vec![0.0, 5.0]);
        reset();
    }
}
