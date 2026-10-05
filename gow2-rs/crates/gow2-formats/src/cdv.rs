//! Collision-ball definitions (`CDV_*`, collision server `0x10`, header type `0x00010010`, kind string `BallHull`): the volumes a character
//! or a weapon touches others with (`docs/combat.md` 2.1).
//!
//! Layout (CONFIRMED against `CDV_gohero` and `CDV_gomaiblade`; the joint and material index sections were read on 2026-10-04):
//! ```text
//! +0x00 type 0x00010010   +0x04 "BallHull"        +0x0c record size
//! +0x14 ball count        +0x1c bounding sphere (x, y, z, r)
//! +0x34 material count    +0x38 u32[8] section offsets:
//!   [1] 0x5c materials: 0x40 bytes each, name[32], +0x2c hash, +0x30 material id
//!   [2] per ball: the joint index it is attached to (u8)
//!   [3] per ball: the index of its material (u8)
//!   [4] balls: f32 x, y, z, r in the space of the ball's joint
//! ```
//! A ball's centre is a point in its joint's local frame, so the world centre is `centre x joint world matrix`. For a model whose rig
//! scales its joint (the blade's rig is 1/64) the scale is part of that matrix, which is why the blade's ball of radius 844.8 is a
//! 13.2-unit sphere in the world.

use crate::{fixed_str, le_f32, le_u32};

#[derive(Debug, Clone, PartialEq)]
pub struct Ball {
    /// Centre in the local frame of `joint`.
    pub centre: [f32; 3],
    pub radius: f32,
    /// Joint index in the owner's rig.
    pub joint: u8,
    /// Index into `BallHull::materials`.
    pub material: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub name: String,
    /// The attack-volume id that hit windows select (`docs/combat.md` 2.1).
    pub id: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BallHull {
    pub materials: Vec<Material>,
    pub balls: Vec<Ball>,
}

impl BallHull {
    /// The volume id of a ball.
    pub fn id_of(&self, b: &Ball) -> u32 {
        self.materials.get(b.material as usize).map_or(0, |m| m.id)
    }
}

/// Parses the payload of a `CDV_*` record; `None` when it is not a ball hull or the sections do not fit.
pub fn parse(p: &[u8]) -> Option<BallHull> {
    if p.len() < 0x58 || le_u32(p, 0) != 0x0001_0010 || &p[4..12] != b"BallHull" {
        return None;
    }
    let (nballs, nmat) = (le_u32(p, 0x14) as usize, le_u32(p, 0x34) as usize);
    let off: Vec<usize> = (0..8).map(|i| le_u32(p, 0x38 + 4 * i) as usize).collect();
    let (mat, joint, matidx, balls) = (off[1], off[2], off[3], off[4]);
    if mat + 0x40 * nmat > p.len() || joint + nballs > p.len() || matidx + nballs > p.len() || balls + 16 * nballs > p.len() {
        return None;
    }
    let materials = (0..nmat).map(|i| Material { name: fixed_str(&p[mat + 0x40 * i..mat + 0x40 * i + 32]), id: le_u32(p, mat + 0x40 * i + 0x30) }).collect();
    let balls = (0..nballs)
        .map(|i| {
            let o = balls + 16 * i;
            Ball { centre: [le_f32(p, o), le_f32(p, o + 4), le_f32(p, o + 8)], radius: le_f32(p, o + 12), joint: p[joint + i], material: p[matidx + i] }
        })
        .collect();
    Some(BallHull { materials, balls })
}
