//! The `FLP_*` movie format of the UI server (`renFlashServer`, server 0x1b) and the `MDL_*` shape model that holds its geometry.
//!
//! A movie is a compiled, Flash-like file: dictionaries of characters (shapes, fonts, static text, text fields, buttons and movie clips), per-clip
//! layer timelines of keyframes, frame labels and action lists (a compact variant of the SWF action bytecode), placement matrices (16.16 plus a
//! 16-bit translation) and colour transforms (8.8 multipliers). The file is stored with its pointers zeroed; the loader (`FUN_00159670`) lays the
//! arrays out one after another, each aligned to four bytes, and fixes the pointers up. [`Flp::parse`] walks the file with the same algorithm.
//! The shapes themselves are not in the movie: a shape's id indexes a table in the movie's model (`MDL_HUDA_0`), whose packets are PS2 VIF
//! unpack lists (positions, texture coordinates). Layout notes and evidence: `docs/hud.md`.

use crate::{le_f32, le_u16, le_u32};

/// One keyframe of a layer: from `frame` on, show character `ch` (index into [`Flp::chars`]; 0 shows nothing).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    pub frame: u16,
    pub ch: u16,
    /// Index into [`Flp::matrices`]; 0 means the parent's matrix.
    pub matrix: u16,
    /// Index into [`Flp::cxforms`]; 0 means the parent's colour transform.
    pub cxform: u16,
    /// Instance name (string-pool offset), `0xffff` when unnamed.
    pub name: u16,
}

/// A frame's label and action lists.
#[derive(Debug, Clone, Default)]
pub struct FrameInfo {
    pub frame: u16,
    pub label: Option<String>,
    pub actions: Vec<Vec<u8>>,
}

/// A movie clip (also the root timeline).
#[derive(Debug, Clone, Default)]
pub struct Clip {
    pub frames: u16,
    /// Bounds in twips: x min, y min, x max, y max.
    pub bounds: [i16; 4],
    pub layers: Vec<Vec<Key>>,
    pub frame_info: Vec<FrameInfo>,
}

impl Clip {
    pub fn label_frame(&self, label: &str) -> Option<u16> {
        self.frame_info.iter().find(|f| f.label.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(label))).map(|f| f.frame)
    }

    /// The key shown on `layer` at `frame`: the last key at or before it.
    pub fn key_at(&self, layer: usize, frame: u16) -> Option<&Key> {
        self.layers[layer].iter().rev().find(|k| k.frame <= frame)
    }
}

/// A draw item of a shape: a flat colour or a texture (index into the movie's texture group).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemRef {
    /// Fill colour `0xAARRGGBB` when the item has no texture (`-1` when it is textured).
    pub color: u32,
    /// Index into the texture group (`TXR_*` in the order of the group), -1 for none.
    pub texture: i32,
}

/// A shape or glyph entry: an id into the model's shape table and its draw items.
#[derive(Debug, Clone, Default)]
pub struct ShapeRef {
    pub shape: u16,
    pub items: Vec<ItemRef>,
}

#[derive(Debug, Clone, Default)]
pub struct Font {
    /// Em scale used with a text size (`size / 1024 * scale` twips).
    pub scale: u16,
    /// Glyph shapes (the low-resolution set when two sets exist).
    pub glyphs: Vec<ShapeRef>,
    /// Advance of each glyph in glyph units.
    pub advance: Vec<i16>,
    /// Character code to glyph index (256 entries when present).
    pub map: Vec<u16>,
}

/// One glyph run of a static text (`D` array), with the font, size, colour and pen position in force.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticRun {
    /// Index into [`Flp::fonts`].
    pub font: u16,
    /// Size factor: the glyph shapes are scaled by `size / 1024`.
    pub size: f32,
    /// Colour multipliers `[r, g, b, a]` in 0..1 (the stream stores BGRA bytes, each plus one, over 256).
    pub color: [f32; 4],
    /// Pen position of the first glyph (twips); the baseline is `y`.
    pub x: f32,
    pub y: f32,
    /// `(glyph index, advance in twips)` pairs.
    pub glyphs: Vec<(u16, i16)>,
}

/// A dynamic text field (`E` array).
#[derive(Debug, Clone)]
pub struct TextField {
    /// Variable the text shows (string-pool offset), `0xffff` when none.
    pub var: u16,
    /// Initial text (string-pool offset), `0xffff` when none.
    pub text: u16,
    /// Character index (into [`Flp::chars`]) of the font.
    pub font: u16,
    /// Size in 1/1024 em.
    pub size: u16,
    /// Colour `0xAARRGGBB`.
    pub color: u32,
    pub raw: [u8; 32],
}

#[derive(Debug, Clone, Default)]
pub struct Flp {
    /// The character dictionary: `(type, id)`; types 1 shape (`shapes`), 3 font (`fonts`), 4 static text, 5 text field, 6 button, 7 clip.
    pub chars: Vec<(u16, u16)>,
    pub shapes: Vec<ShapeRef>,
    pub fonts: Vec<Font>,
    pub texts: Vec<TextField>,
    /// Static texts (`D` array), decoded from the command streams of `FUN_001550f0` (`docs/hud.md`).
    pub statics: Vec<Vec<StaticRun>>,
    pub clips: Vec<Clip>,
    pub root: Clip,
    pub matrices: Vec<[f32; 6]>,
    pub cxforms: Vec<[i16; 4]>,
    pub strings: Vec<u8>,
}

struct Walker<'a> {
    b: &'a [u8],
    cur: usize,
}

impl<'a> Walker<'a> {
    fn align(&mut self) -> usize {
        self.cur = (self.cur + 3) & !3;
        self.cur
    }

    /// Reserves `count` records of `stride` bytes at the aligned cursor.
    fn take(&mut self, count: usize, stride: usize) -> usize {
        let o = self.align();
        self.cur = o + count * stride;
        if count == 0 { 0 } else { o }
    }

    fn u16(&self, o: usize) -> u16 {
        le_u16(self.b, o)
    }

    fn u32(&self, o: usize) -> u32 {
        le_u32(self.b, o)
    }

    /// `FUN_001590e8`: `entry + 6` items of 8 bytes: `{color or -1, texture index}`.
    fn items(&mut self, entry: usize) -> Vec<ItemRef> {
        let n = self.u16(entry + 6) as usize;
        let o = self.take(n, 8);
        (0..n).map(|i| ItemRef { color: self.u32(o + 8 * i), texture: self.u32(o + 8 * i + 4) as i32 }).collect()
    }

    fn shape_ref(&mut self, entry: usize) -> ShapeRef {
        let shape = self.u16(entry + 4);
        ShapeRef { shape, items: self.items(entry) }
    }

    /// `FUN_00159428` on the 0x18-byte clip struct at `o`.
    fn clip(&mut self, o: usize, strings: &dyn Fn(u16) -> Option<String>) -> Clip {
        let frames = self.u16(o + 8);
        let n1 = self.u16(o + 10) as usize;
        let mut bounds = [0i16; 4];
        for (i, b) in bounds.iter_mut().enumerate() {
            *b = self.u16(o + 14 + 2 * i) as i16;
        }
        let a1 = self.take(n1, 8);
        let mut layers = Vec::with_capacity(n1);
        for i in 0..n1 {
            let e = a1 + 8 * i;
            let n = self.u16(e + 6) as usize;
            let p = self.take(n, 10);
            layers.push(
                (0..n)
                    .map(|k| {
                        let q = p + 10 * k;
                        Key { frame: self.u16(q), ch: self.u16(q + 2), matrix: self.u16(q + 4), cxform: self.u16(q + 6), name: self.u16(q + 8) }
                    })
                    .collect(),
            );
        }
        let n2 = self.u16(o + 12) as usize;
        let a2 = self.take(n2, 0xc);
        let mut frame_info = Vec::with_capacity(n2);
        for i in 0..n2 {
            let e = a2 + 0xc * i;
            let frame = self.u16(e + 4);
            let n = self.u16(e + 6) as usize;
            let label = strings(self.u16(e + 8));
            let arr = self.take(n, 8);
            let mut actions = Vec::with_capacity(n);
            for j in 0..n {
                let size = self.u32(arr + 8 * j + 4) as usize;
                let off = self.take(size, 1);
                actions.push(if size == 0 { Vec::new() } else { self.b[off..off + size].to_vec() });
            }
            frame_info.push(FrameInfo { frame, label, actions });
        }
        Clip { frames, bounds, layers, frame_info }
    }
}

impl Flp {
    /// Reads a movie record. `None` when the data is too short or does not end where the loader's layout says.
    pub fn parse(b: &[u8]) -> Option<Flp> {
        if b.len() < 0x5c {
            return None;
        }
        let count = |o: usize| le_u32(b, o) as usize;
        let (na, nb, nc, nd, ne, nf, ng) = (count(0x38), count(0x3c), count(0x40), count(0x44), count(0x48), count(0x4c), count(0x50));
        let (nh, ni, nj) = (le_u16(b, 0x54) as usize, le_u16(b, 0x56) as usize, le_u16(b, 0x58) as usize);
        // string pool position is needed while reading clips (labels), so find it first with a dry walk
        let pool = Self::find_pool(b)?;
        let strings = move |off: u16| -> Option<String> {
            if off == 0xffff {
                return None;
            }
            let s = pool + off as usize;
            let e = b[s..].iter().position(|&c| c == 0)? + s;
            Some(b[s..e].iter().map(|&c| c as char).collect())
        };
        let mut w = Walker { b, cur: 0x5c };
        let oa = w.take(na, 4);
        let chars: Vec<(u16, u16)> = (0..na).map(|i| (le_u16(b, oa + 4 * i), le_u16(b, oa + 4 * i + 2))).collect();
        let ob = w.take(nb, 8);
        let mut shapes = Vec::with_capacity(nb);
        for i in 0..nb {
            shapes.push(w.shape_ref(ob + 8 * i));
        }
        let oc = w.take(nc, 0x24);
        let mut fonts = Vec::with_capacity(nc);
        for i in 0..nc {
            let e = oc + 0x24 * i;
            let flags = w.u16(e + 0x20);
            let k = w.u32(e + 0x10) as usize;
            let mut font = Font { scale: w.u16(e + 0x1a), ..Default::default() };
            let mut sets: Vec<Vec<ShapeRef>> = Vec::new();
            for bit in [2u16, 4] {
                if flags & bit != 0 {
                    let a = w.take(k, 8);
                    sets.push((0..k).map(|j| w.shape_ref(a + 8 * j)).collect());
                }
            }
            font.glyphs = sets.pop().unwrap_or_default();
            let adv = w.take(k, 2);
            font.advance = (0..k).map(|j| w.u16(adv + 2 * j) as i16).collect();
            if flags & 1 != 0 {
                let m = w.align();
                w.cur += 0x200;
                font.map = (0..256).map(|j| w.u16(m + 2 * j)).collect();
            } else {
                w.take(k, 2);
            }
            fonts.push(font);
        }
        let od = w.take(nd, 0x1c);
        let mut statics = Vec::with_capacity(nd);
        for i in 0..nd {
            let n = w.u32(od + 0x1c * i + 0x18) as usize;
            let off = w.take(n, 1);
            statics.push(if n == 0 { Vec::new() } else { decode_static(&b[off..off + n], &chars) });
        }
        let oe = w.take(ne, 0x20);
        let texts = (0..ne)
            .map(|i| {
                let e = oe + 0x20 * i;
                let mut raw = [0u8; 32];
                raw.copy_from_slice(&b[e..e + 32]);
                TextField { var: w.u16(e), text: w.u16(e + 2), font: w.u16(e + 4), size: w.u16(e + 6), color: w.u32(e + 8), raw }
            })
            .collect();
        let of = w.take(nf, 0xc);
        for i in 0..nf {
            let e = of + 0xc * i;
            let g = w.take(1, 0x18);
            w.clip(g, &strings);
            let n = w.u16(e + 8) as usize;
            let a = w.take(n, 0x10);
            for j in 0..n {
                let m = w.u32(a + 0x10 * j + 4) as usize;
                w.take(m, 1);
            }
        }
        let og = w.take(ng, 0x18);
        let mut clips = Vec::with_capacity(ng);
        for i in 0..ng {
            clips.push(w.clip(og + 0x18 * i, &strings));
        }
        let ox = w.take(1, 0x18);
        let root = w.clip(ox, &strings);
        let oh = w.take(nh, 0x14);
        let matrices = (0..nh)
            .map(|i| {
                let e = oh + 0x14 * i;
                let f = |o: usize| w.u32(e + o) as i32 as f32 / 65536.0;
                [f(0), f(4), f(8), f(12), w.u16(e + 16) as i16 as f32, w.u16(e + 18) as i16 as f32]
            })
            .collect();
        let oi = w.take(ni, 8);
        let cxforms = (0..ni).map(|i| [0, 2, 4, 6].map(|o| w.u16(oi + 8 * i + o) as i16)).collect();
        let oj = w.take(nj, 1);
        if w.cur != b.len() || oj != pool {
            return None;
        }
        Some(Flp { chars, shapes, fonts, texts, statics, clips, root, matrices, cxforms, strings: b[oj..oj + nj].to_vec() })
    }

    /// The string pool's offset: found by walking the layout without keeping anything.
    fn find_pool(b: &[u8]) -> Option<usize> {
        // The pool is the last array: its end is the end of the record and its length is the header's u16 at 0x58.
        let nj = le_u16(b, 0x58) as usize;
        b.len().checked_sub(nj)
    }

    pub fn string(&self, off: u16) -> Option<&str> {
        if off == 0xffff || off as usize >= self.strings.len() {
            return None;
        }
        let s = &self.strings[off as usize..];
        let e = s.iter().position(|&c| c == 0)?;
        std::str::from_utf8(&s[..e]).ok()
    }

    pub fn matrix(&self, idx: u16) -> [f32; 6] {
        if idx == 0 { [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] } else { self.matrices[idx as usize] }
    }
}

/// The movie variables the engine sets from the message table (`MSGS_TXT`, `*id*` lines each followed by the message text). A one-line message `n` becomes
/// `PS2_n`; a message of several lines becomes `PS2_na`, `PS2_nb` ... one per line (the movie's own names, e.g. `PS2_4015a`, `PS2_4015b`).
/// Tags in square brackets (`[XButton]` button icons, `[PS2_CyclopsEye_Count]` substitutions) are left out: the port has no icon glyphs for them yet.
pub fn message_vars(table: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut id: Option<String> = None;
    let mut lines: Vec<String> = Vec::new();
    let flush = |id: &mut Option<String>, lines: &mut Vec<String>, out: &mut Vec<(String, String)>| {
        if let Some(id) = id.take() {
            while lines.last().is_some_and(|l| l.is_empty()) {
                lines.pop();
            }
            if lines.len() <= 1 {
                out.push((format!("PS2_{id}"), lines.first().cloned().unwrap_or_default()));
            } else {
                for (k, l) in lines.iter().enumerate() {
                    out.push((format!("PS2_{id}{}", (b'a' + k as u8) as char), l.clone()));
                }
            }
        }
        lines.clear();
    };
    for raw in table.split('\n') {
        let line = raw.trim_end_matches('\r');
        if line.len() > 2 && line.starts_with('*') && line.ends_with('*') && line[1..line.len() - 1].chars().all(|c| c.is_ascii_digit()) {
            flush(&mut id, &mut lines, &mut out);
            id = Some(line[1..line.len() - 1].to_string());
            continue;
        }
        let mut s = String::new();
        let mut depth = 0;
        for c in line.chars() {
            match c {
                '[' => depth += 1,
                ']' if depth > 0 => depth -= 1,
                _ if depth == 0 => s.push(c),
                _ => {}
            }
        }
        lines.push(s.trim().to_string());
    }
    flush(&mut id, &mut lines, &mut out);
    out
}

/// Decodes a static text command stream (`FUN_001550f0`). A record starting with a byte below 0x80 is a glyph run: the byte is the glyph count, followed
/// by that many `(glyph u16, advance i16)` pairs. A record with bit 7 set is a command whose flag bits say what follows, in this order: bit 3 font (character
/// index u16, size u16 in 1/1024), bit 2 colour (4 bytes B, G, R, A), bit 1 pen x (i16), bit 0 pen y (i16). The record after a command is always a glyph run.
/// The pen x moves on by each advance and stays after the run.
fn decode_static(s: &[u8], chars: &[(u16, u16)]) -> Vec<StaticRun> {
    let mut out = Vec::new();
    let (mut font, mut size, mut color, mut x, mut y) = (None::<u16>, 1.0f32, [1.0f32; 4], 0.0f32, 0.0f32);
    let (mut pos, mut after_command) = (0usize, false);
    while pos < s.len() {
        let b = s[pos];
        let is_command = b & 0x80 != 0 && !after_command;
        after_command = is_command;
        if !is_command {
            let count = b as usize;
            let mut glyphs = Vec::with_capacity(count);
            let mut p = pos + 1;
            let first_x = x;
            for _ in 0..count {
                if p + 4 > s.len() {
                    break;
                }
                let g = u16::from_le_bytes([s[p], s[p + 1]]);
                let adv = i16::from_le_bytes([s[p + 2], s[p + 3]]);
                glyphs.push((g, adv));
                x += adv as f32;
                p += 4;
            }
            if let Some(f) = font {
                out.push(StaticRun { font: f, size, color, x: first_x, y, glyphs });
            }
            pos = p;
            continue;
        }
        let mut p = pos + 1;
        let rd = |p: usize| -> Option<[u8; 2]> { s.get(p..p + 2).map(|v| [v[0], v[1]]) };
        if b & 8 != 0 {
            let (Some(ci), Some(sz)) = (rd(p), rd(p + 2)) else { break };
            font = chars.get(u16::from_le_bytes(ci) as usize).map(|c| c.1);
            size = u16::from_le_bytes(sz) as f32 / 1024.0;
            p += 4;
        }
        if b & 4 != 0 {
            let Some(c) = s.get(p..p + 4) else { break };
            color = [(c[2] as f32 + 1.0) / 256.0, (c[1] as f32 + 1.0) / 256.0, (c[0] as f32 + 1.0) / 256.0, (c[3] as f32 + 1.0) / 256.0];
            p += 4;
        }
        if b & 2 != 0 {
            let Some(v) = rd(p) else { break };
            x = i16::from_le_bytes(v) as f32;
            p += 2;
        }
        if b & 1 != 0 {
            let Some(v) = rd(p) else { break };
            y = i16::from_le_bytes(v) as f32;
            p += 2;
        }
        pos = p;
    }
    out
}

/// The texture names a movie's items index: the `TXR_*` records of the group (`GroupStart` .. `GroupEnd`) that precedes the movie record, in
/// order. `records` are `(tag, name)` pairs of the whole WAD in file order.
pub fn texture_group(records: &[(crate::wad::Tag, String)], flp_name: &str) -> Vec<String> {
    use crate::wad::Tag;
    let Some(at) = records.iter().position(|(t, n)| *t == Tag::Object && n == flp_name) else { return Vec::new() };
    let Some(start) = records[..at].iter().rposition(|(t, _)| *t == Tag::GroupStart) else { return Vec::new() };
    records[start + 1..at].iter().take_while(|(t, _)| *t != Tag::GroupEnd).filter(|(_, n)| n.starts_with("TXR_")).map(|(_, n)| n.clone()).collect()
}

// ---- the shape model ----

/// One vertex of a shape item in twips with texture coordinates 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vtx {
    pub x: f32,
    pub y: f32,
    pub u: f32,
    pub v: f32,
    /// The strip-restart bit: this vertex only continues the strip, no triangle ends on it.
    pub skip: bool,
}

/// A triangle strip.
#[derive(Debug, Clone, Default)]
pub struct ItemMesh {
    pub verts: Vec<Vtx>,
    pub textured: bool,
}

impl ItemMesh {
    /// The strip as a triangle list (indices into `verts`).
    pub fn triangles(&self) -> Vec<[usize; 3]> {
        (2..self.verts.len()).filter(|&i| !self.verts[i].skip).map(|i| [i - 2, i - 1, i]).collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShapeMesh {
    pub items: Vec<ItemMesh>,
}

/// The shape table of a `MDL_*` model used by a movie: shape id to its meshes.
pub fn parse_shapes(m: &[u8]) -> Vec<ShapeMesh> {
    if m.len() < 0x18 {
        return Vec::new();
    }
    let n = le_u16(m, 0x8) as usize;
    let offs: Vec<usize> = (0..n).map(|i| le_u32(m, 0x18 + 4 * i) as usize).chain(std::iter::once(m.len())).collect();
    (0..n).map(|i| parse_shape(m, offs[i], offs[i + 1].min(m.len()))).collect()
}

fn parse_shape(m: &[u8], start: usize, end: usize) -> ShapeMesh {
    let mut items: Vec<ItemMesh> = Vec::new();
    let mut uv: Vec<(f32, f32)> = Vec::new();
    let mut k = start;
    while k + 4 <= end {
        let w = le_u32(m, k);
        let cmd = (w >> 24) as u8;
        let num = ((w >> 16) & 0xff) as usize;
        if (0x60..=0x7f).contains(&cmd) {
            let vn = ((cmd >> 2) & 3) as usize;
            let vl = (cmd & 3) as usize;
            let comps = vn + 1;
            let size = match vl {
                0 => 4 * comps * num,
                1 => 2 * comps * num,
                2 => comps * num,
                _ => 2 * num,
            };
            let size = (size + 3) & !3;
            let d = k + 4;
            if d + size > m.len() {
                break;
            }
            match cmd {
                0x60 => {
                    items.push(ItemMesh::default());
                    uv.clear();
                }
                0x65 => uv = (0..num).map(|i| (le_u16(m, d + 4 * i) as i16 as f32 / 4096.0, le_u16(m, d + 4 * i + 2) as i16 as f32 / 4096.0)).collect(),
                0x64 => uv = (0..num).map(|i| (le_f32(m, d + 8 * i), le_f32(m, d + 8 * i + 4))).collect(),
                0x6d => {
                    if let Some(item) = items.last_mut() {
                        item.textured = uv.len() == num;
                        item.verts = (0..num)
                            .map(|i| {
                                let o = d + 8 * i;
                                let (u, v) = uv.get(i).copied().unwrap_or((0.0, 0.0));
                                Vtx { x: le_u16(m, o) as i16 as f32, y: le_u16(m, o + 2) as i16 as f32, u, v, skip: le_u16(m, o + 6) & 0x8000 != 0 }
                            })
                            .collect();
                    }
                }
                _ => {}
            }
            k = d + size;
        } else {
            k += 4;
        }
    }
    ShapeMesh { items }
}
