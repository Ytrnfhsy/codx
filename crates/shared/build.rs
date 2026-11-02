//! Builds made by hand take their game version from the nearest `v*` git tag, so a
//! server built from the source of `v0.2.1` says 0.2.1 like the released game does
//! (CI sets `TW_VERSION` itself; without git the crate version is used).

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=TW_VERSION");
    // look again when tags or the checked out commit change
    let git = Path::new("../../.git");
    for p in ["HEAD", "refs/tags", "packed-refs"] {
        let p = git.join(p);
        if p.exists() {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
    if let Ok(head) = std::fs::read_to_string(git.join("HEAD")) {
        if let Some(r) = head.strip_prefix("ref: ") {
            let p = git.join(r.trim());
            if p.exists() {
                println!("cargo:rerun-if-changed={}", p.display());
            }
        }
    }
    // the highest `v*` tag in this commit's history (two tags can sit on one commit)
    let out = Command::new("git")
        .args(["tag", "--merged", "HEAD", "--list", "v[0-9]*"])
        .output();
    if let Ok(o) = out {
        if o.status.success() {
            let best = String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|t| t.trim().strip_prefix('v').map(str::to_string))
                .max_by_key(|v| key(v));
            if let Some(v) = best {
                println!("cargo:rustc-env=TW_GIT_VERSION={v}");
            }
        }
    }
}

/// "0.10.2" → [0, 10, 2] for comparing (anything after a `-` is ignored).
fn key(v: &str) -> Vec<u64> {
    v.split('-')
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}
