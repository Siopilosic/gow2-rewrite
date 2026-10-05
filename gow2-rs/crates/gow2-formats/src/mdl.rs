//! `MDL_` model parts: DMA packet walk and vertex batches (port of `tools/mdl_decode.py`).
//!
//! Hierarchy and the batch layout are described in `docs/models.md`. Each DMA `ref` packet holds a sequence
//! of vertex batches written for VU1 memory: UV (V2-16), normal (V3-8), position (V4-16, w = flags),
//! colour (V4-8u) and a V4-32 header that closes the batch. Strip rule: position w bit 15 = no triangle.
//! The Python decoder is the oracle; `tests/mdl_oracle.rs` compares the two.

use crate::{le_u16, le_u32};

/// One decoded UNPACK block: the attribute role and its element tuples (up to four components).
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub fmt: u8,
    pub values: Vec<[i64; 4]>,
}

/// A vertex batch: blocks keyed by role, in stream order. A V4-32 header closes a batch.
#[derive(Debug, Clone, Default)]
pub struct Batch {
    pub uv: Option<Vec<[i64; 4]>>,
    pub normal: Option<Vec<[i64; 4]>>,
    pub pos: Option<Vec<[i64; 4]>>,
    pub col: Option<Vec<[i64; 4]>>,
    pub hdr: Option<Vec<[i64; 4]>>,
}

/// (components, element size in bytes, signed) for an UNPACK format nibble.
fn unpack_format(fmt: u8, unsigned: bool) -> (usize, usize, bool) {
    let (comps, size, signed) = match fmt {
        0x0 => (1, 4, true),
        0x1 => (1, 2, true),
        0x2 => (1, 1, true),
        0x4 => (2, 4, true),
        0x5 => (2, 2, true),
        0x6 => (2, 1, true),
        0x8 => (3, 4, true),
        0x9 => (3, 2, true),
        0xA => (3, 1, true),
        0xC => (4, 4, true),
        0xD => (4, 2, true),
        0xE => (4, 1, true),
        0xF => (1, 2, false),
        _ => (1, 4, true),
    };
    (comps, size, signed && !unsigned)
}

fn read_elem(buf: &[u8], at: usize, size: usize, signed: bool) -> i64 {
    match (size, signed) {
        (1, true) => buf[at] as i8 as i64,
        (1, false) => buf[at] as i64,
        (2, true) => i16::from_le_bytes([buf[at], buf[at + 1]]) as i64,
        (2, false) => u16::from_le_bytes([buf[at], buf[at + 1]]) as i64,
        (4, true) => i32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]) as i64,
        _ => u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]) as i64,
    }
}

/// Walks a VIF packet stream and returns its batches (stream order).
pub fn batches(buf: &[u8], mut pos: usize, end: usize) -> Vec<Batch> {
    let mut out = Vec::new();
    let mut cur = Batch::default();
    let mut any = false;
    while pos + 4 <= end {
        let code = le_u32(buf, pos);
        pos += 4;
        let cmd = ((code >> 24) & 0x7F) as u8;
        let num = ((code >> 16) & 0xFF) as usize;
        let imm = (code & 0xFFFF) as usize;
        if cmd >= 0x60 {
            let fmt = cmd & 0xF;
            let (comps, size, signed) = unpack_format(fmt, imm & 0x4000 != 0);
            let n = if num == 0 { 256 } else { num };
            let mut vals = Vec::with_capacity(n);
            for i in 0..n {
                let mut v = [0i64; 4];
                for (c, slot) in v.iter_mut().enumerate().take(comps) {
                    *slot = read_elem(buf, pos + (i * comps + c) * size, size, signed);
                }
                vals.push(v);
            }
            pos += (n * comps * size + 3) & !3;
            any = true;
            match fmt {
                0x5 => cur.uv = Some(vals),
                0xA => cur.normal = Some(vals),
                0xD => cur.pos = Some(vals),
                0xE => cur.col = Some(vals),
                0xC => {
                    cur.hdr = Some(vals);
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
                _ => {}
            }
        } else if cmd == 0x20 {
            pos += 4;
        } else if cmd == 0x30 || cmd == 0x31 {
            pos += 16;
        } else if cmd == 0x4A {
            pos += (if num == 0 { 256 } else { num }) * 8;
        } else if cmd == 0x50 || cmd == 0x51 {
            pos += (if imm == 0 { 65536 } else { imm }) * 16;
        }
    }
    if any {
        out.push(cur);
    }
    out
}

/// One model part: indices into the hierarchy, the part kind, and its DMA packets `(group, start, end)`.
#[derive(Debug, Clone)]
pub struct Part {
    pub i: usize,
    pub j: usize,
    pub k: usize,
    pub offset: usize,
    pub kind: i16,
    pub packets: Vec<(usize, usize, usize)>,
}

pub fn parts(blob: &[u8]) -> Vec<Part> {
    let mut out = Vec::new();
    for i in 0..le_u16(blob, 8) as usize {
        let a = le_u32(blob, 0x18 + 4 * i) as usize;
        for j in 0..le_u16(blob, a + 2) as usize {
            let bo = a + le_u32(blob, a + 4 + 4 * j) as usize;
            for k in 0..le_u16(blob, bo + 4) as usize {
                let c = bo + le_u32(blob, bo + 8 + 4 * k) as usize;
                let kind = i16::from_le_bytes([blob[c], blob[c + 1]]);
                let mut packets = Vec::new();
                if kind == 0x18 || kind == 0x0E {
                    let groups = blob[c + 0x18] as usize * le_u32(blob, c + 0xC) as usize;
                    let per = le_u32(blob, c + 4) as usize;
                    let mut e = c + 0x20;
                    for g in 0..groups {
                        for _ in 0..per {
                            let w0 = le_u32(blob, e);
                            let addr = le_u32(blob, e + 4) as usize;
                            if matches!((w0 >> 28) & 7, 0 | 3 | 4) {
                                packets.push((g, c + addr, c + addr + (w0 & 0xFFFF) as usize * 16));
                            }
                            e += 16;
                        }
                    }
                }
                out.push(Part { i, j, k, offset: c, kind, packets });
            }
        }
    }
    out
}

/// Vertices, colours and triangles of DMA group `group`, as the Python decoder builds them.
#[derive(Debug, Default, Clone)]
pub struct Mesh {
    pub verts: Vec<[i64; 3]>,
    pub cols: Vec<[i64; 4]>,
    pub tris: Vec<[u32; 3]>,
}

pub fn mesh(blob: &[u8], group: usize) -> Mesh {
    let mut m = Mesh::default();
    for part in parts(blob) {
        for &(g, s, e) in &part.packets {
            if g != group {
                continue;
            }
            for b in batches(blob, s, e) {
                let Some(p) = b.pos else { continue };
                let base = m.verts.len() as u32;
                m.verts.extend(p.iter().map(|v| [v[0], v[1], v[2]]));
                match b.col {
                    Some(c) => m.cols.extend(c),
                    None => m.cols.extend(std::iter::repeat([128; 4]).take(p.len())),
                }
                for n in 2..p.len() {
                    if p[n][3] & 0x8000 == 0 {
                        let t = [base + n as u32 - 2, base + n as u32 - 1, base + n as u32];
                        m.tris.push(if n % 2 == 0 { t } else { [t[1], t[0], t[2]] });
                    }
                }
            }
        }
    }
    m
}

/// Material slots of a model: the `MAT_` reference records between `MDL_<name>` (88 bytes) and `MDL_<name>_0`
/// (observed layout, docs/models.md). A part's slot (low 16 bits of its header word +0x08) indexes this list.
pub fn model_materials(recs: &[crate::wad::Record], name: &str) -> Vec<String> {
    let head = format!("MDL_{name}");
    let tail = crate::wad::mesh_record_name(name);
    let mut out = Vec::new();
    let mut inside = false;
    for r in recs {
        if !inside {
            inside = r.tag == crate::wad::Tag::Object && r.name == head && r.data.len() == 88;
            continue;
        }
        if r.name == tail {
            break;
        }
        if r.tag == crate::wad::Tag::Object && r.name.starts_with("MAT_") && r.data.is_empty() {
            out.push(r.name.clone());
        }
    }
    out
}

