//! The static world collision of a level: the `RIB_sheet` record (collision server `0x10`, header type `0x00000010`).
//!
//! Layout, from the loader `FUN_00179338` (`docs/collision.md` section 3):
//! ```text
//! +0x00 u32 type (0x10)         +0x04 u32 record size
//! +0x10 f32[3] bounds min       +0x20 f32[3] bounds max
//! +0x3e u16 triangle count      +0x40 u16 quad count      +0x42 u16 vertex count
//! +0x44 u16 surface stride (used when the count is 0)     +0x48 u16 surface count
//! +0x4a u16 flag-name count     +0x50 u32[8] section offsets (the last one is the end)
//! section 0  (+0x50)  broad-phase nodes, not decoded
//! section 1  (+0x54)  surfaces: `+0x48` records of 64 bytes
//! section 2  (+0x58)  flag-name table: `+0x4a` records of 76 bytes
//! section 3  (+0x5c)  leaf polygon lists, not decoded here
//! section 4  (+0x60)  triangles: 8 bytes, u16 surface << 4, u16 v0, v1, v2
//! section 5  (+0x64)  quads: 10 bytes, u16 surface << 4, u16 v0, v1, v2, v3 (a convex loop)
//! section 6  (+0x68)  vertices: f32 x, y, z in world units
//! ```
//! The game builds a plane for every polygon at load time (normal from the corners, normalised); we do the same. Every polygon
//! names a surface; a surface carries a 32-bit flag word at `+0x18` whose bits are named by the flag-name table.

use crate::{fixed_str, le_f32, le_u16, le_u32, wad};

/// Names of the low flag bits, from the flag-name table of every level (`Ground`, `Water`, ...). Index = bit number.
pub mod flag {
    pub const GROUND: u32 = 1 << 0;
    pub const WATER: u32 = 1 << 1;
    pub const NARROW: u32 = 1 << 2;
    pub const CLIMBABLE: u32 = 1 << 3;
    pub const LADDER: u32 = 1 << 4;
    pub const SLIDE: u32 = 1 << 5;
    pub const DEATH: u32 = 1 << 6;
    pub const DEATH_SINK: u32 = 1 << 7;
    pub const CEILING: u32 = 1 << 8;
    pub const NARROW_WALL_PRESS: u32 = 1 << 9;
    pub const AI_BLOCK: u32 = 1 << 16;
    pub const CLIMB_GUIDE: u32 = 1 << 17;
    pub const PUSH_PULL_SLIDE: u32 = 1 << 18;
    pub const BACK_PRESS: u32 = 1 << 19;
    pub const NO_PLAYER_COLLISION: u32 = 1 << 20;
    pub const NO_AI_COLLISION: u32 = 1 << 21;
    pub const NO_PLAYER_USE: u32 = 1 << 22;
    pub const NO_AI_USE: u32 = 1 << 23;
    pub const GENERAL_GUIDE: u32 = 1 << 24;
    pub const NO_DIVING: u32 = 1 << 25;
    pub const NO_PUSH_PULL_COLLISION: u32 = 1 << 26;
    pub const COMBAT_GUIDE: u32 = 1 << 27;
    pub const NARROW_NO_BB: u32 = 1 << 28;
    pub const NO_CSM_COLLISION: u32 = 1 << 29;
    pub const HOD_LEAP: u32 = 1 << 30;

    /// A query skips every polygon whose flags share a bit with its mask. The character move code ORs these bases with the
    /// character's own `+0x288 & 0x4300000` (`NoPlayerCollision`, `NoAICollision`, `NoPushPullCollision`).
    /// Floor and step probes (`0x10042`): water, death, AI blocks.
    pub const SKIP_FLOOR: u32 = 0x0001_0042;
    /// Horizontal sweeps (`0x41010042`): additionally the general guides and the leap markers.
    pub const SKIP_WALK: u32 = 0x4101_0042;
    /// What the hero carries from `+0x288`: he does not collide with `NoPlayerCollision` surfaces (name and bit agree; the value
    /// of `+0x288` for the hero is not read from RAM, MEDIUM).
    pub const HERO_OWN: u32 = NO_PLAYER_COLLISION;
}

/// A surface type named by a polygon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    pub name: String,
    /// The low flag word (`bitfield0`, record `+0x18`).
    pub flags: u32,
    /// The high flag word (record `+0x1c`).
    pub flags_hi: u32,
}

/// One collision polygon: a triangle or a convex quad with its plane.
#[derive(Debug, Clone, PartialEq)]
pub struct Poly {
    pub surface: u8,
    /// Corners in loop order; the fourth is unused for a triangle.
    pub v: [[f32; 3]; 4],
    pub corners: u8,
    /// Unit normal. A triangle uses `(v1-v0) x (v2-v0)`, a quad the cross product of its diagonals `(v2-v0) x (v3-v1)`.
    pub normal: [f32; 3],
}

impl Poly {
    /// The polygon as triangles with the same winding (a quad is split along `v0`-`v2`).
    pub fn triangles(&self) -> impl Iterator<Item = [[f32; 3]; 3]> + '_ {
        let n = if self.corners == 4 { 2 } else { 1 };
        (0..n).map(move |i| if i == 0 { [self.v[0], self.v[1], self.v[2]] } else { [self.v[0], self.v[2], self.v[3]] })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Sheet {
    pub bounds: ([f32; 3], [f32; 3]),
    pub surfaces: Vec<Surface>,
    pub polys: Vec<Poly>,
    /// Names from the flag-name table, in file order.
    pub flag_names: Vec<String>,
}

impl Sheet {
    pub fn surface(&self, p: &Poly) -> &Surface {
        &self.surfaces[p.surface as usize]
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(n: [f32; 3]) -> [f32; 3] {
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 1e-9 {
        [n[0] / l, n[1] / l, n[2] / l]
    } else {
        [0.0; 3]
    }
}

/// Parses the payload of a `RIB_sheet` record. `None` when the layout is inconsistent (a section outside the record, an index
/// past the vertex table).
pub fn parse(p: &[u8]) -> Option<Sheet> {
    if p.len() < 0x70 || le_u32(p, 0) != 0x10 {
        return None;
    }
    let off: Vec<usize> = (0..8).map(|i| le_u32(p, 0x50 + 4 * i) as usize).collect();
    if off.windows(2).any(|w| w[0] > w[1]) || off[7] > p.len() {
        return None;
    }
    let (ntri, nquad, nvert) = (le_u16(p, 0x3e) as usize, le_u16(p, 0x40) as usize, le_u16(p, 0x42) as usize);
    let (nsurf, nflag) = (le_u16(p, 0x48) as usize, le_u16(p, 0x4a) as usize);
    // the game divides the section by the count to get the record stride (64 in every level)
    let stride = if nsurf == 0 { le_u16(p, 0x44) as usize } else { (off[2] - off[1]) / nsurf };
    if stride < 0x20 || off[4] + 8 * ntri > off[5] || off[5] + 10 * nquad > off[6] || off[6] + 12 * nvert > off[7] || off[1] + stride * nsurf > off[2] || off[2] + 76 * nflag > off[3] {
        return None;
    }
    let vert = |i: u16| -> Option<[f32; 3]> {
        let i = i as usize;
        (i < nvert).then(|| {
            let o = off[6] + 12 * i;
            [le_f32(p, o), le_f32(p, o + 4), le_f32(p, o + 8)]
        })
    };
    let surfaces: Vec<Surface> = (0..nsurf)
        .map(|i| {
            let o = off[1] + stride * i;
            Surface { name: fixed_str(&p[o..o + 24]), flags: le_u32(p, o + 0x18), flags_hi: le_u32(p, o + 0x1c) }
        })
        .collect();
    let flag_names = (0..nflag).map(|i| fixed_str(&p[off[2] + 76 * i..off[2] + 76 * i + 0x3c])).collect();
    let mut polys = Vec::with_capacity(ntri + nquad);
    for i in 0..ntri {
        let o = off[4] + 8 * i;
        let s = le_u16(p, o) >> 4;
        let (a, b, c) = (vert(le_u16(p, o + 2))?, vert(le_u16(p, o + 4))?, vert(le_u16(p, o + 6))?);
        if s as usize >= nsurf {
            return None;
        }
        polys.push(Poly { surface: s as u8, v: [a, b, c, [0.0; 3]], corners: 3, normal: unit(cross(sub(b, a), sub(c, a))) });
    }
    for i in 0..nquad {
        let o = off[5] + 10 * i;
        let s = le_u16(p, o) >> 4;
        let (a, b, c, d) = (vert(le_u16(p, o + 2))?, vert(le_u16(p, o + 4))?, vert(le_u16(p, o + 6))?, vert(le_u16(p, o + 8))?);
        if s as usize >= nsurf {
            return None;
        }
        polys.push(Poly { surface: s as u8, v: [a, b, c, d], corners: 4, normal: unit(cross(sub(c, a), sub(d, b))) });
    }
    let bounds = ([le_f32(p, 0x10), le_f32(p, 0x14), le_f32(p, 0x18)], [le_f32(p, 0x20), le_f32(p, 0x24), le_f32(p, 0x28)]);
    Some(Sheet { bounds, surfaces, polys, flag_names })
}

/// The collision sheet of a level WAD, if it has one.
pub fn find(recs: &[wad::Record]) -> Option<Sheet> {
    let r = recs.iter().find(|r| r.tag == wad::Tag::Object && r.name == "RIB_sheet")?;
    parse(r.data)
}
