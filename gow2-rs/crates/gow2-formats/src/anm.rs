//! `ANM_` clip curves (port of `tools/anm_decode.py`, documented in `docs/animation.md`).
//!
//! Codec as read from the transform sampler `FUN_00112380` and its helpers. A clip has blocks of 16 bytes
//! at `clip + 0x60`; a block points at a table of 12-byte segments. A segment is either a flat key list
//! or a list of absolute / int8-delta runs. Accumulators use f64 here, like the Python oracle.

use std::collections::BTreeMap;

fn u16at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]))
}

fn u32at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]))
}

fn f32at(b: &[u8], o: usize) -> Option<f32> {
    Some(f32::from_bits(u32at(b, o)?))
}

/// Block kind decides the element formats and scales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// quaternion: s8 deltas, s16 keys, 16384 = 1.0
    Rot,
    /// s16 deltas x 1/256, f32 keys
    Trans,
    /// u16 keys x 1/2048
    Scale,
}

#[derive(Clone, Copy)]
enum Fmt {
    I8,
    I16,
    U16,
    F32,
}

impl Fmt {
    fn size(self) -> usize {
        match self {
            Fmt::I8 => 1,
            Fmt::I16 | Fmt::U16 => 2,
            Fmt::F32 => 4,
        }
    }
    fn read(self, b: &[u8], o: usize) -> Option<f64> {
        Some(match self {
            Fmt::I8 => *b.get(o)? as i8 as f64,
            Fmt::I16 => u16at(b, o)? as i16 as f64,
            Fmt::U16 => u16at(b, o)? as f64,
            Fmt::F32 => f32at(b, o)? as f64,
        })
    }
}

impl Kind {
    /// (delta format, absolute format, delta scale, absolute scale)
    fn spec(self) -> (Fmt, Fmt, f64, f64) {
        match self {
            Kind::Rot => (Fmt::I8, Fmt::I16, 1.0, 1.0),
            Kind::Trans => (Fmt::I16, Fmt::F32, 1.0 / 256.0, 1.0),
            Kind::Scale => (Fmt::I16, Fmt::U16, 1.0 / 2048.0, 1.0 / 2048.0),
        }
    }
}

const DEFAULT_TABLE: [u8; 8] = [1, 1, 0, 0, 1, 0, 0, 0];

/// Curves of one segment: output slot -> (frame -> value).
pub type Curves = BTreeMap<u32, BTreeMap<i64, f64>>;

#[derive(Debug, Clone)]
pub struct Segment {
    pub slot: u16,
    pub flags: u16,
    pub curves: Curves,
}

/// Decodes one segment with a fresh accumulator. Returns `None` when the segment runs past the record.
pub fn decode_segment(b: &[u8], seg: usize, kind: Kind) -> Option<Segment> {
    decode_segment_acc(b, seg, kind, &mut BTreeMap::new())
}

/// Decodes one segment. `acc` (slot -> value) is shared by all segments of a clip block: the game keeps one
/// accumulator per slot, so a delta run continues from a key set by another segment.
pub fn decode_segment_acc(b: &[u8], seg: usize, kind: Kind, acc: &mut BTreeMap<u32, f64>) -> Option<Segment> {
    let (dfmt, afmt, dsc, asc) = kind.spec();
    let slot = u16at(b, seg)?;
    let flags = u16at(b, seg + 2)?;
    let nkeys = u16at(b, seg + 4)? as usize;
    let start = u16at(b, seg + 6)? as i64;
    let ex = u16at(b, seg + 8)?;
    let lo = u16at(b, seg + 10)?;
    let data = seg + ((((ex & 0xC000) as usize) << 2) | lo as usize) + (flags >> 8) as usize * 0x10000;
    // keyed segments keep the table at the data offset; run lists put it after the runs
    let (tb, t): (&[u8], usize) = if flags & 2 != 0 {
        (b, if nkeys != 0 { data } else { data + (((*b.get(data + 1)? as usize) << 3) | 2) })
    } else {
        (&DEFAULT_TABLE, 0)
    };
    let rows = *tb.get(t)? as usize;
    let stride = *tb.get(t + 1)? as usize;
    let base = u16at(tb, t + 2)? as usize;
    let masks: Vec<u16> = (0..rows).map(|r| u16at(tb, t + 4 + 2 * r)).collect::<Option<_>>()?;
    let shifts: Vec<i32> = if stride == 1 {
        vec![((flags & 0xFF) as u8 as i8 >> 4) as i32]
    } else {
        (0..stride).map(|i| tb.get(t + 4 + 2 * rows + i).map(|&v| v as i8 as i32)).collect::<Option<_>>()?
    };
    let mut comps: Vec<(u32, usize)> = Vec::new();
    let mut i = 0;
    for (r, m) in masks.iter().enumerate() {
        for k in 0..16 {
            if m & (1 << k) != 0 {
                comps.push((slot as u32 + 16 * r as u32 + k, i));
                i += 1;
            }
        }
    }
    let mut curves: Curves = comps.iter().map(|&(s, _)| (s, BTreeMap::new())).collect();
    for &(s, _) in &comps {
        acc.entry(s).or_insert(0.0);
    }
    let (dsz, asz) = (dfmt.size(), afmt.size());

    let step = |off: usize, f: usize, absolute: bool, acc: &mut BTreeMap<u32, f64>| -> Option<()> {
        for &(s, ci) in &comps {
            if absolute {
                let v = afmt.read(b, off + asz * (f * stride + ci))?;
                acc.insert(s, v * asc);
            } else {
                let sh = *shifts.get(ci).or(shifts.first())?; // empty shift table: the Python decoder raises IndexError here
                let d = dfmt.read(b, off + dsz * (f * stride + ci))?;
                *acc.get_mut(&s).unwrap() += d * dsc * 2f64.powi(-sh);
            }
        }
        Some(())
    };

    if nkeys != 0 {
        // flat key list: flags & 1 -> deltas, else absolute keys; delta key f gives frame start + 1 + f
        let lag = (flags & 1) as i64;
        for f in 0..nkeys {
            step(data + base, f, flags & 1 == 0, acc)?;
            for &(s, _) in &comps {
                curves.get_mut(&s).unwrap().insert(start + f as i64 + lag, acc[&s]);
            }
        }
        return Some(Segment { slot, flags, curves });
    }
    let ndelta = *b.get(data)? as usize;
    let nruns = *b.get(data + 1)? as usize;
    let mut events: Vec<(i64, bool, usize, usize)> = Vec::new();
    for r in 0..nruns {
        let o = data + 2 + 8 * r;
        let (cnt, st, ex2, lo2) = (u16at(b, o)?, u16at(b, o + 2)?, u16at(b, o + 4)?, u16at(b, o + 6)?);
        let off = seg + base + ((((ex2 & 0xC000) as usize) << 2) | lo2 as usize) + (flags >> 8) as usize * 0x10000;
        events.push((st as i64, r >= ndelta, cnt as usize, off));
    }
    // absolute runs seed the accumulators, delta runs integrate in frame order (stable sort like Python)
    events.sort_by_key(|e| (e.0, !e.1));
    for (st, absolute, cnt, off) in events {
        for f in 0..cnt {
            step(off, f, absolute, acc)?;
            for &(s, _) in &comps {
                curves.get_mut(&s).unwrap().insert(st + f as i64 + if absolute { 0 } else { 1 }, acc[&s]);
            }
        }
    }
    Some(Segment { slot, flags, curves })
}

/// Clip start offsets of an `ANM_` record.
pub fn clips(b: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let ng = u16at(b, 0x12).unwrap_or(0) as usize;
    if ng == 0 && b.len() > 0x40 && u16at(b, 0) == Some(3) {
        // group-only record (level WADs): one group at +0x0c, its clips use the character's track layout
        let go = 0xCusize;
        for c in 0..u32at(b, go + 0xC).unwrap_or(0) as usize {
            if let Some(o) = u32at(b, go + 0x34 + 4 * c) {
                out.push(go + o as usize);
            }
        }
        return out;
    }
    for g in 0..ng {
        let Some(go) = u32at(b, 0x18 + 4 * g) else { continue };
        let go = go as usize;
        for c in 0..u32at(b, go + 0xC).unwrap_or(0) as usize {
            if let Some(o) = u32at(b, go + 0x34 + 4 * c) {
                out.push(go + o as usize);
            }
        }
    }
    out
}

/// The kind of every block of an `ANM_` record, from its track list. A transform track (type 0) owns
/// rot/trans/scale blocks; emitter (10) and material (3, 8) tracks use f32 keys.
pub fn block_kinds(b: &[u8]) -> Option<Vec<Kind>> {
    let ntr = u16at(b, 0x10)? as usize;
    let ng = u16at(b, 0x12)? as usize;
    let mut kinds: Vec<Kind> = Vec::new();
    const CYCLE: [Kind; 3] = [Kind::Rot, Kind::Trans, Kind::Scale];
    for i in 0..ntr {
        let o = 0x18 + 4 * ng + 8 * i;
        let ttype = u16at(b, o)?;
        let nsub = *b.get(o + 3)? as usize;
        for j in 0..nsub {
            let k = if ttype == 0 {
                CYCLE[j % 3]
            } else if matches!(ttype, 3 | 8 | 10) {
                Kind::Trans
            } else {
                CYCLE[kinds.len() % 3]
            };
            kinds.push(k);
        }
    }
    Some(kinds)
}

/// One decoded block of a clip: its sample interval and segments.
#[derive(Debug, Clone)]
pub struct Block {
    pub dt: f32,
    pub segments: Vec<Option<Segment>>,
}

/// Decodes all blocks of a clip with a fresh accumulator per segment (the oracle's behaviour).
pub fn decode_clip(b: &[u8], clip: usize, kinds: &[Kind]) -> Option<Vec<Block>> {
    let mut out = Vec::new();
    for (k, &kind) in kinds.iter().enumerate() {
        let blk = clip + 0x60 + 16 * k;
        let nseg = u16at(b, blk + 2)? as usize;
        let tab = u32at(b, blk + 8)? as usize;
        let dt = f32at(b, blk + 12)?;
        let segments = (0..nseg).map(|s| decode_segment(b, clip + tab + 12 * s, kind)).collect();
        out.push(Block { dt, segments });
    }
    Some(out)
}

/// Clip duration in seconds (`clip + 0x14`).
pub fn clip_duration(b: &[u8], clip: usize) -> Option<f32> {
    f32at(b, clip + 0x14)
}

/// Sort key for decoding the segments of one clip block in time order with a shared accumulator: (first frame,
/// 0 if that frame is an absolute key else 1). Run lists use their earliest run.
pub fn segment_order(b: &[u8], seg: usize) -> Option<(i64, u8)> {
    let flags = u16at(b, seg + 2)?;
    let nkeys = u16at(b, seg + 4)?;
    let start = u16at(b, seg + 6)? as i64;
    let ex = u16at(b, seg + 8)?;
    let lo = u16at(b, seg + 10)?;
    if nkeys != 0 {
        return Some((start, (flags & 1) as u8));
    }
    let data = seg + ((((ex & 0xC000) as usize) << 2) | lo as usize) + (flags >> 8) as usize * 0x10000;
    let ndelta = *b.get(data)? as usize;
    let nruns = *b.get(data + 1)? as usize;
    let mut best: Option<(i64, u8)> = None;
    for r in 0..nruns {
        let key = (u16at(b, data + 2 + 8 * r + 2)? as i64, if r < ndelta { 1 } else { 0 });
        best = Some(best.map_or(key, |m| m.min(key)));
    }
    Some(best.unwrap_or((0, 0)))
}
