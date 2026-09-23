// ---
// tags: bbg, cli, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Cyber-dialect tade frames for the bbg store log.
//!
//! The store log is a concatenation of tade frames — the stack's one wire
//! format (see [[tade]]). bbg operates on the state-application signal
//! (`bbg::Signal`), so these frames carry that shape, distinct from sync's
//! envelope frame (which also carries prev/step/network for the chain).
//!
//! Frame catalog:
//!
//! | event    | sigil   | payload                                        |
//! |----------|---------|------------------------------------------------|
//! | signal   | ZAP `!` | neuron ‖ height ‖ links ‖ box_moves            |
//! | intent   | KET `^` | neuron ‖ h0 ‖ scope_hash ‖ signature           |
//! | finalize | DOT `.` | (empty) — a block boundary marker              |

use bbg::{BoxMove, Cyberlink, IntentRecord, Signal};
use tade::{sigil, Chunk, ReadResult, Reader};

const RENDER_BIN: u8 = b'b';
const SIG_FINALIZE: u8 = b'.'; // tade DOT — a block-boundary marker

/// One replayable event in the log, in wire order.
pub enum Event {
    Signal(Signal),
    Intent(IntentRecord),
    Finalize,
}

// ── encode ─────────────────────────────────────────────────────────────────

pub fn encode_signal(s: &Signal) -> Vec<u8> {
    Chunk::new(sigil::ZAP, RENDER_BIN, serialize_signal(s).into()).encode()
}

pub fn encode_intent(i: &IntentRecord) -> Vec<u8> {
    Chunk::new(sigil::KET, RENDER_BIN, serialize_intent(i).into()).encode()
}

pub fn encode_finalize() -> Vec<u8> {
    Chunk::new(SIG_FINALIZE, RENDER_BIN, Vec::new().into()).encode()
}

// ── decode ───────────────────────────────────────────────────────────────────

/// Walk a concatenated tade stream and return every event in order.
/// Frames with unknown sigils are skipped (forward-compatible).
pub fn decode_events(bytes: &[u8]) -> Vec<Event> {
    let mut reader = Reader::new();
    reader.feed(bytes);
    let mut out = Vec::new();
    loop {
        match reader.next_chunk() {
            ReadResult::Chunk(c) if c.sigil == sigil::ZAP => {
                if let Some(s) = deserialize_signal(&c.payload) {
                    out.push(Event::Signal(s));
                }
            }
            ReadResult::Chunk(c) if c.sigil == sigil::KET => {
                if let Some(i) = deserialize_intent(&c.payload) {
                    out.push(Event::Intent(i));
                }
            }
            ReadResult::Chunk(c) if c.sigil == SIG_FINALIZE => out.push(Event::Finalize),
            ReadResult::Chunk(_) => {} // unknown sigil — skip
            ReadResult::Pending | ReadResult::Eof => break,
        }
    }
    out
}

// ── serialization (hand-rolled LE; small, stable payloads) ───────────────────

fn serialize_signal(s: &Signal) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&s.neuron);
    b.extend_from_slice(&s.height.to_le_bytes());
    b.extend_from_slice(&(s.links.len() as u32).to_le_bytes());
    for l in &s.links {
        b.extend_from_slice(&l.from);
        b.extend_from_slice(&l.to);
        b.extend_from_slice(&l.token);
        b.extend_from_slice(&l.amount.to_le_bytes());
        b.push(l.valence as u8);
    }
    b.extend_from_slice(&(s.box_moves.len() as u32).to_le_bytes());
    for m in &s.box_moves {
        b.extend_from_slice(&m.nullifier);
        match &m.commitment {
            Some((point, value)) => {
                b.push(1);
                b.extend_from_slice(point);
                b.extend_from_slice(&value.to_le_bytes());
            }
            None => b.push(0),
        }
    }
    b
}

/// Wire size of one `Cyberlink` record: from ‖ to ‖ token ‖ amount ‖ valence.
const LINK_BYTES: usize = 32 + 32 + 32 + 8 + 1;
/// Wire size of the smallest possible `BoxMove` record: nullifier ‖ tag(0).
const MOVE_MIN_BYTES: usize = 32 + 1;

fn deserialize_signal(buf: &[u8]) -> Option<Signal> {
    let mut p = 0;
    let neuron = take32(buf, &mut p)?;
    let height = take_u64(buf, &mut p)?;
    let n_links = take_u32(buf, &mut p)? as usize;
    // A payload this short cannot possibly hold n_links records — reject before
    // Vec::with_capacity tries to allocate an attacker-chosen amount of memory.
    if n_links > buf.len().saturating_sub(p) / LINK_BYTES {
        return None;
    }
    let mut links = Vec::with_capacity(n_links);
    for _ in 0..n_links {
        links.push(Cyberlink {
            from: take32(buf, &mut p)?,
            to: take32(buf, &mut p)?,
            token: take32(buf, &mut p)?,
            amount: take_u64(buf, &mut p)?,
            valence: take_u8(buf, &mut p)? as i8,
        });
    }
    let n_moves = take_u32(buf, &mut p)? as usize;
    if n_moves > buf.len().saturating_sub(p) / MOVE_MIN_BYTES {
        return None;
    }
    let mut box_moves = Vec::with_capacity(n_moves);
    for _ in 0..n_moves {
        let nullifier = take32(buf, &mut p)?;
        let commitment = match take_u8(buf, &mut p)? {
            1 => Some((take32(buf, &mut p)?, take_u64(buf, &mut p)?)),
            _ => None,
        };
        box_moves.push(BoxMove { nullifier, commitment });
    }
    Some(Signal { neuron, links, box_moves, height })
}

fn serialize_intent(i: &IntentRecord) -> Vec<u8> {
    let mut b = Vec::with_capacity(32 + 8 + 32 + 64);
    b.extend_from_slice(&i.neuron);
    b.extend_from_slice(&i.h0.to_le_bytes());
    b.extend_from_slice(&i.scope_hash);
    b.extend_from_slice(&i.signature);
    b
}

fn deserialize_intent(buf: &[u8]) -> Option<IntentRecord> {
    let mut p = 0;
    let neuron = take32(buf, &mut p)?;
    let h0 = take_u64(buf, &mut p)?;
    let scope_hash = take32(buf, &mut p)?;
    let signature = take64(buf, &mut p)?;
    Some(IntentRecord { neuron, h0, scope_hash, signature })
}

// ── bounded cursor helpers ───────────────────────────────────────────────────

fn take32(buf: &[u8], p: &mut usize) -> Option<[u8; 32]> {
    let s = buf.get(*p..*p + 32)?;
    *p += 32;
    s.try_into().ok()
}
fn take64(buf: &[u8], p: &mut usize) -> Option<[u8; 64]> {
    let s = buf.get(*p..*p + 64)?;
    *p += 64;
    s.try_into().ok()
}
fn take_u64(buf: &[u8], p: &mut usize) -> Option<u64> {
    let s = buf.get(*p..*p + 8)?;
    *p += 8;
    Some(u64::from_le_bytes(s.try_into().ok()?))
}
fn take_u32(buf: &[u8], p: &mut usize) -> Option<u32> {
    let s = buf.get(*p..*p + 4)?;
    *p += 4;
    Some(u32::from_le_bytes(s.try_into().ok()?))
}
fn take_u8(buf: &[u8], p: &mut usize) -> Option<u8> {
    let b = *buf.get(*p)?;
    *p += 1;
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig() -> Signal {
        Signal {
            neuron: [1u8; 32],
            links: vec![Cyberlink { from: [2u8; 32], to: [3u8; 32], token: [0u8; 32], amount: 4, valence: 1 }],
            box_moves: vec![],
            height: 7,
        }
    }

    #[test]
    fn signal_frame_roundtrips() {
        let frame = encode_signal(&sig());
        assert_eq!(frame[0], 0x1F, "tade marker");
        assert_eq!(frame[1], sigil::ZAP);
        let events = decode_events(&frame);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Signal(s) => {
                assert_eq!(s.neuron, [1u8; 32]);
                assert_eq!(s.height, 7);
                assert_eq!(s.links.len(), 1);
                assert_eq!(s.links[0].amount, 4);
                assert_eq!(s.links[0].to, [3u8; 32]);
            }
            _ => panic!("expected signal"),
        }
    }

    #[test]
    fn intent_frame_roundtrips() {
        let i = IntentRecord { neuron: [9u8; 32], h0: 3, scope_hash: [5u8; 32], signature: [0u8; 64] };
        let events = decode_events(&encode_intent(&i));
        assert!(matches!(&events[0], Event::Intent(r) if r.h0 == 3 && r.neuron == [9u8;32]));
    }

    #[test]
    fn mixed_log_decodes_in_order() {
        let mut log = Vec::new();
        log.extend(encode_signal(&sig()));
        log.extend(encode_finalize());
        log.extend(encode_signal(&sig()));
        let events = decode_events(&log);
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], Event::Signal(_)));
        assert!(matches!(events[1], Event::Finalize));
        assert!(matches!(events[2], Event::Signal(_)));
    }

    #[test]
    fn rejects_n_links_that_cannot_fit_in_the_payload() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 32]); // neuron
        buf.extend_from_slice(&7u64.to_le_bytes()); // height
        buf.extend_from_slice(&u32::MAX.to_le_bytes()); // n_links: malicious
        assert!(deserialize_signal(&buf).is_none());
    }

    #[test]
    fn rejects_n_links_larger_than_the_records_remaining() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 32]); // neuron
        buf.extend_from_slice(&7u64.to_le_bytes()); // height
        buf.extend_from_slice(&2u32.to_le_bytes()); // claims 2 links
        buf.extend_from_slice(&[0u8; LINK_BYTES]); // room for only 1
        assert!(deserialize_signal(&buf).is_none());
    }

    #[test]
    fn rejects_n_moves_that_cannot_fit_in_the_payload() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 32]); // neuron
        buf.extend_from_slice(&7u64.to_le_bytes()); // height
        buf.extend_from_slice(&0u32.to_le_bytes()); // n_links: 0
        buf.extend_from_slice(&u32::MAX.to_le_bytes()); // n_moves: malicious
        assert!(deserialize_signal(&buf).is_none());
    }

    #[test]
    fn accepts_a_well_formed_signal_with_links_and_moves() {
        let s = Signal {
            neuron: [1u8; 32],
            links: vec![Cyberlink { from: [2u8; 32], to: [3u8; 32], token: [0u8; 32], amount: 4, valence: -1 }],
            box_moves: vec![BoxMove { nullifier: [5u8; 32], commitment: Some(([6u8; 32], 9)) }],
            height: 7,
        };
        let frame = encode_signal(&s);
        let events = decode_events(&frame);
        match &events[0] {
            Event::Signal(decoded) => {
                assert_eq!(decoded.links.len(), 1);
                assert_eq!(decoded.box_moves.len(), 1);
                assert_eq!(decoded.box_moves[0].nullifier, [5u8; 32]);
                assert_eq!(decoded.box_moves[0].commitment, Some(([6u8; 32], 9)));
            }
            _ => panic!("expected signal"),
        }
    }
}
