//! Rigs, joint-bound meshes and ANM_ clip channels (port of `tools/rig_anim.py`, `docs/animation.md`).
//!
//! Rig record (unnamed tag-1 record that follows a `go<name>` node; header u16 1, u16 1):
//! `+0x04` joint count, 16-byte joint entries at `+0x18` (s16 firstChild, nextSibling, parent, ?), names, then
//! row-vector 4x4 local matrices (translation in row 3) at `align16(0x18 + 40 * nj) + 0x30`.
//! Joint binding: each part has a palette of `u16(part + 10)` s32 joint indices after its DMA entry table; a
//! batch header's GIF tag covers NLOOP vertices and its w word selects the palette slot `(w & 0x3ff) / 4`.
//! Vertices are joint-local.

use std::collections::BTreeMap;

use crate::{
    anm::{self, Kind},
    le_u16, le_u32, mdl,
    wad::{Record, Tag},
};

fn le_i16(b: &[u8], o: usize) -> i16 {
    le_u16(b, o) as i16
}

fn le_f32(b: &[u8], o: usize) -> f32 {
    f32::from_bits(le_u32(b, o))
}

/// A rig record's joint tree.
#[derive(Debug, Clone)]
pub struct Rig {
    pub names: Vec<String>,
    pub parents: Vec<i16>,
    /// Row-vector local matrices, row-major.
    pub mats: Vec<[f32; 16]>,
}

/// A model with its rig: MDL name (without `MDL_`), rig record, ANM_ name if any.
#[derive(Debug, Clone)]
pub struct RigRef<'a> {
    pub model: String,
    pub rig: &'a [u8],
    pub anm: Option<String>,
}

/// `go<name>` groups hold {rig, ANM_ ref, MDL_ ref, ESC_ ref}: model name -> rig bytes and ANM name.
pub fn find_rigs<'a>(recs: &[Record<'a>]) -> Vec<RigRef<'a>> {
    let mut out: Vec<RigRef<'a>> = Vec::new();
    for (i, r) in recs.iter().enumerate() {
        if r.tag != Tag::Object
            || !r.name.is_empty()
            || r.data.len() < 0x28
            || (le_u16(r.data, 0), le_u16(r.data, 2)) != (1, 1)
        {
            continue;
        }
        let (mut anm_name, mut mdl_name) = (None, None);
        for r2 in recs.iter().skip(i + 1).take(11) {
            if r2.tag != Tag::Object || !r2.data.is_empty() {
                break;
            }
            if r2.name.starts_with("ANM_") && anm_name.is_none() {
                anm_name = Some(r2.name.clone());
            } else if r2.name.starts_with("MDL_") && mdl_name.is_none() {
                mdl_name = Some(r2.name[4..].to_string());
            }
        }
        if let Some(m) = mdl_name {
            if !out.iter().any(|x| x.model == m) {
                out.push(RigRef { model: m, rig: r.data, anm: anm_name });
            }
        }
    }
    out
}

/// Joint names: 24-byte strings from `0x14 + 16 * nj`.
pub fn rig_joint_names(b: &[u8]) -> Vec<String> {
    let nj = le_u32(b, 4) as usize;
    let base = 0x14 + 16 * nj;
    (0..nj)
        .map(|i| {
            let s = &b[base + 24 * i..base + 24 * (i + 1)];
            let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
            s[..end].iter().map(|&c| c as char).collect()
        })
        .collect()
}

pub fn parse_rig(b: &[u8]) -> Rig {
    let nj = le_u32(b, 4) as usize;
    let parents = (0..nj).map(|j| le_i16(b, 0x18 + 16 * j + 4)).collect();
    let x = (0x18 + 40 * nj + 15) & !15;
    let mats = (0..nj)
        .map(|j| {
            let mut m = [0f32; 16];
            for (k, v) in m.iter_mut().enumerate() {
                *v = le_f32(b, x + 0x30 + 64 * j + 4 * k);
            }
            m
        })
        .collect();
    Rig { names: rig_joint_names(b), parents, mats }
}

/// Row-vector GoW matrix -> (translation, quaternion xyzw, scale); the glTF TRS convention.
pub fn mat_to_trs(m: &[f32; 16]) -> ([f64; 3], [f64; 4], [f64; 3]) {
    let m: Vec<f64> = m.iter().map(|&x| x as f64).collect();
    let t = [m[12], m[13], m[14]];
    let rows = [[m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]];
    let s: Vec<f64> = rows
        .iter()
        .map(|r| {
            let l = (0.0 + r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
            if l == 0.0 { 1.0 } else { l }
        })
        .collect();
    let r = |i: usize, j: usize| rows[j][i] / s[j];
    let tr = r(0, 0) + r(1, 1) + r(2, 2);
    let q = if tr > 0.0 {
        let w = (1.0 + tr).sqrt() * 2.0;
        [(r(2, 1) - r(1, 2)) / w, (r(0, 2) - r(2, 0)) / w, (r(1, 0) - r(0, 1)) / w, w / 4.0]
    } else if r(0, 0) > r(1, 1) && r(0, 0) > r(2, 2) {
        let w = (1.0 + r(0, 0) - r(1, 1) - r(2, 2)).sqrt() * 2.0;
        [w / 4.0, (r(0, 1) + r(1, 0)) / w, (r(0, 2) + r(2, 0)) / w, (r(2, 1) - r(1, 2)) / w]
    } else if r(1, 1) > r(2, 2) {
        let w = (1.0 + r(1, 1) - r(0, 0) - r(2, 2)).sqrt() * 2.0;
        [(r(0, 1) + r(1, 0)) / w, w / 4.0, (r(1, 2) + r(2, 1)) / w, (r(0, 2) - r(2, 0)) / w]
    } else {
        let w = (1.0 + r(2, 2) - r(0, 0) - r(1, 1)).sqrt() * 2.0;
        [(r(0, 2) + r(2, 0)) / w, (r(1, 2) + r(2, 1)) / w, w / 4.0, (r(1, 0) - r(0, 1)) / w]
    };
    let n = (0.0 + q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let n = if n == 0.0 { 1.0 } else { n };
    (t, [q[0] / n, q[1] / n, q[2] / n, q[3] / n], [s[0], s[1], s[2]])
}

/// A joint-bound mesh (group 0): positions, UVs, colours, a joint per vertex and triangles with their material slot.
#[derive(Debug, Default, Clone)]
pub struct SkinMesh {
    pub verts: Vec<[i64; 3]>,
    pub uvs: Vec<[f64; 2]>,
    pub cols: Vec<[i64; 4]>,
    pub joints: Vec<i64>,
    pub tris: Vec<([u32; 3], u16)>,
    /// Index of the model part (in mdl::parts order) each vertex came from.
    pub part: Vec<u16>,
    /// Second joint of the two-joint blend: palette slot in the high nibble of the GIF tag word (equal to joints when absent).
    pub joints2: Vec<i64>,
    /// Pull of the second joint, 0..1: the low 15 bits of the position w field over 4096 (MEDIUM, from the fit tests).
    pub weight: Vec<f32>,
}

pub fn mesh_joints(blob: &[u8], uvscale: f64) -> SkinMesh {
    let mut m = SkinMesh::default();
    for (part_index, part) in mdl::parts(blob).into_iter().enumerate() {
        let c = part.offset;
        let slot = (le_u32(blob, c + 8) & 0xFFFF) as u16;
        let n10 = le_u16(blob, c + 10) as usize;
        let nent = blob[c + 0x18] as usize * le_u32(blob, c + 0xC) as usize * le_u32(blob, c + 4) as usize;
        let pal: Vec<i64> = if n10 > 0 {
            (0..n10).map(|i| le_u32(blob, c + 0x20 + nent * 16 + 4 * i) as i32 as i64).collect()
        } else {
            vec![0]
        };
        for &(g, s, e) in &part.packets {
            if g != 0 {
                continue;
            }
            for b in mdl::batches(blob, s, e) {
                let Some(p) = b.pos else { continue };
                let base = m.verts.len() as u32;
                m.verts.extend(p.iter().map(|v| [v[0], v[1], v[2]]));
                m.part.extend(std::iter::repeat(part_index as u16).take(p.len()));
                m.weight.extend(p.iter().map(|v| ((v[3] & 0x7FFF) as f32 / 4096.0).clamp(0.0, 1.0)));
                match &b.uv {
                    Some(uv) => m.uvs.extend(uv.iter().map(|t| [t[0] as f64 / uvscale, t[1] as f64 / uvscale])),
                    None => m.uvs.extend(std::iter::repeat([0.0, 0.0]).take(p.len())),
                }
                match b.col {
                    Some(c) => m.cols.extend(c),
                    None => m.cols.extend(std::iter::repeat([128; 4]).take(p.len())),
                }
                let mut jv: Vec<i64> = Vec::new();
                let mut jv2: Vec<i64> = Vec::new();
                for tag in b.hdr.iter().flatten() {
                    let nl = (tag[0] & 0x7FFF) as usize;
                    let ps = ((tag[3] & 0x3FF) / 4) as usize;
                    let hs = ((tag[3] >> 12) & 0xF) as usize;
                    let j = if ps < pal.len() { pal[ps] } else { pal[0] };
                    let j2 = if hs < pal.len() { pal[hs] } else { j };
                    jv.extend(std::iter::repeat(j).take(nl));
                    jv2.extend(std::iter::repeat(j2).take(nl));
                }
                jv.extend(std::iter::repeat(pal[0]).take(p.len()));
                jv2.extend(std::iter::repeat(pal[0]).take(p.len()));
                jv.truncate(p.len());
                jv2.truncate(p.len());
                m.joints.extend(jv);
                m.joints2.extend(jv2);
                for n in 2..p.len() {
                    if p[n][3] & 0x8000 == 0 {
                        let t = [base + n as u32 - 2, base + n as u32 - 1, base + n as u32];
                        m.tris.push((if n % 2 == 0 { t } else { [t[1], t[0], t[2]] }, slot));
                    }
                }
            }
        }
    }
    m
}

/// Clip name: string at `clip + 0x24` (the hash at `+0x20`). Empty when the bytes are not a name.
pub fn clip_name(a: &[u8], c: usize) -> String {
    let Some(raw) = a.get(c + 0x24..c + 0x3c) else { return String::new() };
    let end = raw.iter().position(|&x| x == 0).unwrap_or(raw.len());
    let raw = &raw[..end];
    if !raw.is_empty() && raw.iter().all(|&x| (48..123).contains(&x) || x == 46 || x == 95) {
        raw.iter().map(|&x| x as char).collect()
    } else {
        String::new()
    }
}

/// Per joint: rot / trans / scale curves, each component -> frame -> value.
pub type JointCurves = [BTreeMap<u32, BTreeMap<i64, f64>>; 3];
pub type Channels = BTreeMap<u32, JointCurves>;

/// A decoded clip: sample interval, duration in seconds, per-joint channels.
#[derive(Debug, Clone)]
pub struct ClipChannels {
    pub dt: f32,
    pub duration: f32,
    pub joints: Channels,
}

/// A clip of a transform-track ANM. `name` selects by clip name; `None` takes the first clip.
pub fn clip_channels(a: &[u8], nj: u32, name: Option<&str>) -> Option<ClipChannels> {
    let ng = le_u16(a, 0x12) as usize;
    let tracks: Vec<(u16, u8, u8)> = if ng == 0 {
        vec![(0, 0, 3)]
    } else {
        (0..le_u16(a, 0x10) as usize)
            .map(|i| {
                let o = 0x18 + 4 * ng + 8 * i;
                (le_u16(a, o), a[o + 2], a[o + 3])
            })
            .collect()
    };
    if tracks.is_empty() || tracks[0].0 != 0 {
        return None;
    }
    let all = anm::clips(a);
    let c = match name {
        None => *all.first()?,
        Some(n) => *all.iter().find(|&&x| clip_name(a, x) == n)?,
    };
    let duration = le_f32(a, c + 0x14);
    let mut dt = 1.0f32 / 30.0;
    let mut joints: Channels = BTreeMap::new();
    for (k, kind) in [Kind::Rot, Kind::Trans, Kind::Scale].into_iter().enumerate().take(tracks[0].2 as usize) {
        let blk = c + 0x60 + 16 * k;
        let nseg = le_u16(a, blk + 2) as usize;
        let tab = le_u32(a, blk + 8) as usize;
        dt = le_f32(a, blk + 12);
        let mut shared = BTreeMap::new();
        let mut segs: Vec<usize> = (0..nseg).map(|s| c + tab + 12 * s).collect();
        // time order: by first frame, absolute keys before deltas
        segs.sort_by_key(|&sg| anm::segment_order(a, sg).unwrap_or((0, 0)));
        for sg in segs {
            let Some(seg) = anm::decode_segment_acc(a, sg, kind, &mut shared) else { continue };
            for (sl, cv) in seg.curves {
                let (jnt, comp) = (sl / 4, sl % 4);
                if jnt < nj && !cv.is_empty() {
                    joints.entry(jnt).or_default()[k].entry(comp).or_default().extend(cv);
                }
            }
        }
    }
    Some(ClipChannels { dt, duration, joints })
}
