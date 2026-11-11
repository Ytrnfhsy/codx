//! Minimal reliable-ordered + unreliable messaging over UDP datagrams.
//!
//! Every datagram carries:
//! * `ack` — the next reliable id the sender expects (everything below was received);
//! * a batch of reliable messages that are resent until acknowledged;
//! * a batch of unreliable messages (snapshots, inputs).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// Keep datagrams comfortably below a typical MTU.
pub const RELIABLE_BUDGET: usize = 900;
const RESEND_AFTER: f64 = 0.12;
/// Unreliable payload per datagram: more than this spills into further datagrams, and a
/// single message bigger than it is cut into fragments (so that the network never has
/// to: a datagram over the MTU is dropped whole by many routers when one piece is lost).
pub const UNRELIABLE_BUDGET: usize = 1100;
/// Messages decoded from the wire may not claim to be bigger than this.
const DECODE_LIMIT: u64 = 1 << 20;

fn codec() -> impl bincode::Options {
    use bincode::Options;
    // variable-length integers: lengths, enum tags and small numbers take a byte
    bincode::options().with_limit(DECODE_LIMIT)
}

/// A message as it goes on the wire.
pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    use bincode::Options;
    codec().serialize(msg).expect("serialize")
}

pub fn decode<T: DeserializeOwned>(data: &[u8]) -> Option<T> {
    use bincode::Options;
    codec().deserialize(data).ok()
}
/// Reliable messages further ahead than this of the next expected one are dropped.
const MAX_AHEAD: u32 = 1024;

#[derive(Serialize, Deserialize, Default)]
struct Packet {
    ack: u32,
    rel: Vec<(u32, Vec<u8>)>,
    unrel: Vec<Vec<u8>>,
    /// A piece of a big unreliable message: (message number, piece, pieces, bytes).
    frag: Option<(u16, u8, u8, Vec<u8>)>,
}

struct Pending {
    id: u32,
    data: Vec<u8>,
    last_sent: f64,
}

#[derive(Default)]
pub struct Channel {
    next_id: u32,
    out: VecDeque<Pending>,
    unrel_out: Vec<Vec<u8>>,
    expected: u32,
    held: BTreeMap<u32, Vec<u8>>,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    /// Datagrams after the first one of the last `build_packet` (send them too).
    pub spill: Vec<Vec<u8>>,
    next_frag: u16,
    /// The big message being put together: its number and the pieces so far.
    frag_in: Option<(u16, Vec<Option<Vec<u8>>>)>,
    frag_done: Option<u16>,
}

impl Channel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn send_reliable<T: Serialize>(&mut self, msg: &T) {
        let data = encode(msg);
        self.out.push_back(Pending {
            id: self.next_id,
            data,
            last_sent: f64::NEG_INFINITY,
        });
        self.next_id = self.next_id.wrapping_add(1);
    }

    pub fn send_unreliable<T: Serialize>(&mut self, msg: &T) {
        self.unrel_out.push(encode(msg));
    }

    /// Reliable messages received out of order, waiting for the ones before them.
    pub fn held_len(&self) -> usize {
        self.held.len()
    }

    /// Number of reliable messages waiting for an ack (backlog indicator).
    pub fn backlog(&self) -> usize {
        self.out.len()
    }

    /// Builds the next datagram. Always returns a packet (acks must flow even when idle).
    pub fn build_packet(&mut self, now: f64) -> Vec<u8> {
        let mut p = Packet {
            ack: self.expected,
            ..Default::default()
        };
        let mut budget = RELIABLE_BUDGET;
        for m in self.out.iter_mut() {
            if now - m.last_sent < RESEND_AFTER {
                continue;
            }
            if m.data.len() + 8 > budget && !p.rel.is_empty() {
                break;
            }
            budget = budget.saturating_sub(m.data.len() + 8);
            m.last_sent = now;
            p.rel.push((m.id, m.data.clone()));
        }
        // unreliable messages: what fits goes along, the rest in datagrams of its own
        // (`spill`), big ones in pieces
        let mut more: Vec<Packet> = Vec::new();
        let mut room = UNRELIABLE_BUDGET.saturating_sub(RELIABLE_BUDGET - budget);
        let ack = self.expected;
        for m in std::mem::take(&mut self.unrel_out) {
            if m.len() > UNRELIABLE_BUDGET {
                let pieces = m.chunks(UNRELIABLE_BUDGET).collect::<Vec<_>>();
                if pieces.len() > 255 {
                    continue;
                }
                let id = self.next_frag;
                self.next_frag = self.next_frag.wrapping_add(1);
                for (k, piece) in pieces.iter().enumerate() {
                    more.push(Packet {
                        ack,
                        frag: Some((id, k as u8, pieces.len() as u8, piece.to_vec())),
                        ..Default::default()
                    });
                }
            } else if m.len() + 2 <= room || p.unrel.is_empty() && p.rel.is_empty() {
                room = room.saturating_sub(m.len() + 2);
                p.unrel.push(m);
            } else {
                match more.last_mut().filter(|q| {
                    q.frag.is_none()
                        && q.unrel.iter().map(|u| u.len() + 2).sum::<usize>() + m.len()
                            < UNRELIABLE_BUDGET
                }) {
                    Some(q) => q.unrel.push(m),
                    None => more.push(Packet {
                        ack,
                        unrel: vec![m],
                        ..Default::default()
                    }),
                }
            }
        }
        self.spill.clear();
        for q in &more {
            let b = encode(q);
            self.bytes_sent += b.len() as u64;
            self.spill.push(b);
        }
        let bytes = encode(&p);
        self.bytes_sent += bytes.len() as u64;
        bytes
    }

    /// Processes a datagram; returns decoded messages (reliable ones in order, then unreliable).
    pub fn receive<T: DeserializeOwned>(&mut self, data: &[u8]) -> Option<Vec<T>> {
        let p: Packet = decode(data)?;
        self.bytes_recv += data.len() as u64;
        // drop acknowledged messages
        while let Some(front) = self.out.front() {
            if seq_lt(front.id, p.ack) {
                self.out.pop_front();
            } else {
                break;
            }
        }
        for (id, d) in p.rel {
            // ids far ahead are bogus: a broken or hostile peer could make us hold on to
            // ever more messages waiting for the gap before them
            let ahead = id.wrapping_sub(self.expected);
            if !seq_lt(id, self.expected) && ahead < MAX_AHEAD {
                self.held.entry(id).or_insert(d);
            }
        }
        let mut msgs = Vec::new();
        while let Some(d) = self.held.remove(&self.expected) {
            if let Some(m) = decode::<T>(&d) {
                msgs.push(m);
            }
            self.expected = self.expected.wrapping_add(1);
        }
        for d in p.unrel {
            if let Some(m) = decode::<T>(&d) {
                msgs.push(m);
            }
        }
        // a piece of a big message: whole once every piece of the newest one is in
        if let Some((id, k, n, bytes)) = p.frag {
            let (k, n) = (k as usize, n as usize);
            if n > 0 && k < n && bytes.len() <= UNRELIABLE_BUDGET {
                let after = |a: u16, b: u16| (a.wrapping_sub(b) as i16) > 0;
                let newer = self.frag_in.as_ref().is_none_or(|(cur, _)| after(id, *cur))
                    && self.frag_done.is_none_or(|done| after(id, done))
                    && self.frag_in.as_ref().is_none_or(|(cur, _)| *cur != id);
                if newer {
                    self.frag_in = Some((id, vec![None; n]));
                }
                if let Some((cur, parts)) = self.frag_in.as_mut() {
                    if *cur == id && parts.len() == n {
                        parts[k] = Some(bytes);
                        if parts.iter().all(|p| p.is_some()) {
                            let whole: Vec<u8> =
                                parts.iter_mut().flat_map(|p| p.take().unwrap()).collect();
                            self.frag_in = None;
                            // (keep the number: late pieces of it are not a new message)
                            self.frag_done = Some(id);
                            if let Some(m) = decode::<T>(&whole) {
                                msgs.push(m);
                            }
                        }
                    }
                }
            }
        }
        Some(msgs)
    }
}

fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reliable_survives_loss_and_reorder() {
        let mut a = Channel::new();
        let mut b = Channel::new();
        for i in 0..50u32 {
            a.send_reliable(&i);
        }
        let mut got: Vec<u32> = Vec::new();
        let mut t = 0.0;
        for round in 0..200 {
            t += 0.05;
            let pkt = a.build_packet(t);
            // drop every third packet
            if round % 5 != 0 {
                got.extend(b.receive::<u32>(&pkt).unwrap());
            }
            let back = b.build_packet(t);
            if round % 4 != 1 {
                a.receive::<u32>(&back).unwrap();
            }
        }
        assert_eq!(got, (0..50).collect::<Vec<_>>());
        assert_eq!(a.backlog(), 0);
    }

    /// Big unreliable messages go in pieces under the MTU and come out whole; a lost
    /// piece loses that message only.
    #[test]
    fn big_messages_are_cut_up_and_put_together() {
        let mut a = Channel::new();
        let mut b = Channel::new();
        let big: Vec<u8> = (0..5000u32).map(|i| (i * 7) as u8).collect();
        a.send_unreliable(&big);
        a.send_unreliable(&vec![1u8, 2, 3]);
        let first = a.build_packet(0.0);
        let rest = a.spill.clone();
        assert!(rest.len() >= 5);
        assert!(first.len() < 1300 && rest.iter().all(|p| p.len() < 1300));
        let mut got: Vec<Vec<u8>> = b.receive(&first).unwrap();
        // (out of order)
        for p in rest.iter().rev() {
            got.extend(b.receive::<Vec<u8>>(p).unwrap());
        }
        assert_eq!(got, vec![vec![1u8, 2, 3], big.clone()]);
        // one piece lost: nothing comes out, and the next message still does
        a.send_unreliable(&big);
        let _ = a.build_packet(0.1);
        let rest = a.spill.clone();
        for p in rest.iter().skip(1) {
            assert!(b.receive::<Vec<u8>>(p).unwrap().is_empty());
        }
        a.send_unreliable(&big);
        let _ = a.build_packet(0.2);
        let mut got = Vec::new();
        for p in a.spill.clone() {
            got.extend(b.receive::<Vec<u8>>(&p).unwrap());
        }
        assert_eq!(got, vec![big]);
    }

    #[test]
    fn unreliable_passes_through() {
        let mut a = Channel::new();
        let mut b = Channel::new();
        a.send_unreliable(&7u8);
        a.send_reliable(&1u8);
        let msgs: Vec<u8> = b.receive(&a.build_packet(0.0)).unwrap();
        assert_eq!(msgs, vec![1, 7]);
    }
}
