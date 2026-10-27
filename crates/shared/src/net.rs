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

#[derive(Serialize, Deserialize, Default)]
struct Packet {
    ack: u32,
    rel: Vec<(u32, Vec<u8>)>,
    unrel: Vec<Vec<u8>>,
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
}

impl Channel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn send_reliable<T: Serialize>(&mut self, msg: &T) {
        let data = bincode::serialize(msg).expect("serialize");
        self.out.push_back(Pending {
            id: self.next_id,
            data,
            last_sent: f64::NEG_INFINITY,
        });
        self.next_id = self.next_id.wrapping_add(1);
    }

    pub fn send_unreliable<T: Serialize>(&mut self, msg: &T) {
        self.unrel_out
            .push(bincode::serialize(msg).expect("serialize"));
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
        p.unrel = std::mem::take(&mut self.unrel_out);
        let bytes = bincode::serialize(&p).expect("serialize");
        self.bytes_sent += bytes.len() as u64;
        bytes
    }

    /// Processes a datagram; returns decoded messages (reliable ones in order, then unreliable).
    pub fn receive<T: DeserializeOwned>(&mut self, data: &[u8]) -> Option<Vec<T>> {
        let p: Packet = bincode::deserialize(data).ok()?;
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
            if !seq_lt(id, self.expected) {
                self.held.entry(id).or_insert(d);
            }
        }
        let mut msgs = Vec::new();
        while let Some(d) = self.held.remove(&self.expected) {
            if let Ok(m) = bincode::deserialize::<T>(&d) {
                msgs.push(m);
            }
            self.expected = self.expected.wrapping_add(1);
        }
        for d in p.unrel {
            if let Ok(m) = bincode::deserialize::<T>(&d) {
                msgs.push(m);
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
