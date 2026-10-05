//! Level model placement (port of the placement part of `tools/gltf_export.py`, rules in `docs/models.md`).
//!
//! A level WAD holds models (`MDL_<name>_0`) and ways to place them:
//! * **ref instances**: a type-`0x00020001` record names a node (type `0x00010001`) that a following model reference binds to a
//!   model; its rows at `+0x20` (nine floats) and `+0x50` (three floats) are a rotation `R` and translation `W` (row vectors).
//!   The world position of a vertex `p` (stored units / 16) is `(p * s + o) * R + W`, with `o` and `s` from the model's own
//!   88-byte `MDL_<name>` record (`+0x38` offset, `+0x48` the quantisation factor, `s` = 1 / factor).
//! * **model record only**: models that no ref instance names sit at `p * s + o`.
//! * Models with a skeleton (doors, chunks, lamps) are animated objects and are not placed here.

use std::collections::{HashMap, HashSet};

use crate::{le_f32, le_u32, skin, wad::{Record, Tag}};

/// Offset and scale from a model's own 88-byte record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelXf {
    pub offset: [f32; 3],
    /// `1 / factor` (the factor at `+0x48`; 1.0 when it is 0).
    pub scale: f32,
}

/// One placed copy of a model.
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    /// Name of the ref record.
    pub name: String,
    /// Model name without the `MDL_` prefix.
    pub model: String,
    /// Rotation rows `R` (row vectors), nine floats.
    pub rot: [f32; 9],
    /// Translation `W`.
    pub pos: [f32; 3],
}

#[derive(Debug, Default, Clone)]
pub struct Placement {
    pub instances: Vec<Instance>,
    pub model_xf: HashMap<String, ModelXf>,
    /// Models that have a drawable `MDL_<name>_0` record (more than 64 bytes), in file order.
    pub models: Vec<String>,
    /// Models with a skeleton: animated objects, or large static rooms.
    pub rigged: HashSet<String>,
    /// Placement of rigged models by their go node: rotation rows (+0x20) and translation (+0x44) of the 104-byte
    /// record (header 1, 3) that precedes the model reference in its group. The root joint already carries the model's own
    /// offset and scale, so these are applied after the rig's joint matrices.
    pub go_xf: HashMap<String, ([f32; 9], [f32; 3])>,
}

impl Placement {
    /// Models drawn once at their model-record position: drawable, not rigged, and not named by any ref instance.
    pub fn plain_models(&self) -> Vec<&String> {
        let instanced: HashSet<&String> = self.instances.iter().map(|i| &i.model).collect();
        self.models.iter().filter(|m| !self.rigged.contains(*m) && !instanced.contains(m)).collect()
    }

    /// Transform of a model's own record (the identity when it has none).
    pub fn xf(&self, model: &str) -> ModelXf {
        self.model_xf.get(model).copied().unwrap_or(ModelXf { offset: [0.0; 3], scale: 1.0 })
    }
}

/// World position of a vertex given in model units (stored value / 16) for an instance of `model`.
pub fn place_instance(p: [f32; 3], xf: ModelXf, inst: &Instance) -> [f32; 3] {
    let q = [p[0] * xf.scale + xf.offset[0], p[1] * xf.scale + xf.offset[1], p[2] * xf.scale + xf.offset[2]];
    let r = &inst.rot;
    [
        q[0] * r[0] + q[1] * r[3] + q[2] * r[6] + inst.pos[0],
        q[0] * r[1] + q[1] * r[4] + q[2] * r[7] + inst.pos[1],
        q[0] * r[2] + q[1] * r[5] + q[2] * r[8] + inst.pos[2],
    ]
}

/// World position of a vertex of a model drawn at its model-record position.
pub fn place_plain(p: [f32; 3], xf: ModelXf) -> [f32; 3] {
    [p[0] * xf.scale + xf.offset[0], p[1] * xf.scale + xf.offset[1], p[2] * xf.scale + xf.offset[2]]
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| c as char).collect()
}

/// Reads the placement records of a level WAD.
pub fn parse(recs: &[Record]) -> Placement {
    let mut out = Placement::default();
    let mut node_model: HashMap<String, String> = HashMap::new();
    let mut refs: Vec<(String, String, [f32; 9], [f32; 3])> = Vec::new();
    let mut cur_node: Option<String> = None;
    for r in recs {
        if r.tag == Tag::GroupStart {
            cur_node = None;
        }
        if r.tag != Tag::Object {
            continue;
        }
        let body = r.data;
        if body.len() >= 4 {
            let tw = le_u32(body, 0);
            if tw == 0x0001_0001 {
                cur_node = Some(r.name.clone());
            } else if tw == 0x0002_0001 && body.len() >= 0x5c {
                let tgt = cstr(&body[4..28]);
                let mut rot = [0f32; 9];
                for (k, v) in rot.iter_mut().enumerate() {
                    *v = le_f32(body, 0x20 + 4 * k);
                }
                let pos = [le_f32(body, 0x50), le_f32(body, 0x54), le_f32(body, 0x58)];
                refs.push((r.name.clone(), tgt, rot, pos));
            }
        } else if body.is_empty() && r.name.starts_with("MDL_") {
            if let Some(n) = &cur_node {
                node_model.entry(n.clone()).or_insert_with(|| r.name[4..].to_string());
            }
        }
    }
    for r in recs {
        if r.tag == Tag::Object && r.name.starts_with("MDL_") && r.data.len() == 88 {
            let off = [le_f32(r.data, 0x38), le_f32(r.data, 0x3c), le_f32(r.data, 0x40)];
            let f = le_f32(r.data, 0x48);
            let f = if f == 0.0 { 1.0 } else { f };
            out.model_xf.insert(r.name[4..].to_string(), ModelXf { offset: off, scale: 1.0 / f });
        }
    }
    // mesh records carry at most 19 characters of `MDL_<name>_0` (see `wad::mesh_record_name`), so a long model name is found through its header record
    let headers: std::collections::HashMap<String, String> =
        recs.iter().filter(|r| r.tag == Tag::Object && r.name.starts_with("MDL_") && r.data.len() == 88).map(|r| (crate::wad::mesh_record_name(&r.name[4..]), r.name[4..].to_string())).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    for r in recs {
        if r.tag == Tag::Object && !r.data.is_empty() && r.name.ends_with("_0") && r.data.len() > 64 && seen.insert(r.name.as_str()) {
            if let Some(m) = headers.get(r.name.as_str()) {
                out.models.push(m.clone());
            } else if r.name.starts_with("MDL_") {
                out.models.push(r.name[4..r.name.len() - 2].to_string());
            }
        }
    }
    for (name, tgt, rot, pos) in refs {
        if let Some(m) = node_model.get(&tgt) {
            out.instances.push(Instance { name, model: m.clone(), rot, pos });
        }
    }
    for (i, r) in recs.iter().enumerate() {
        if r.tag == Tag::Object && r.data.len() == 104 && crate::le_u16(r.data, 0) == 1 && crate::le_u16(r.data, 2) == 3 {
            for r2 in recs.iter().skip(i + 1).take(15) {
                if r2.tag == Tag::Object && !r2.data.is_empty() && r2.name.starts_with("go") {
                    break;
                }
                if r2.tag == Tag::Object && r2.data.is_empty() && r2.name.starts_with("MDL_") {
                    let mut rot = [0f32; 9];
                    for (k, v) in rot.iter_mut().enumerate() {
                        *v = le_f32(r.data, 0x20 + 4 * k);
                    }
                    let pos = [le_f32(r.data, 0x44), le_f32(r.data, 0x48), le_f32(r.data, 0x4c)];
                    out.go_xf.entry(r2.name[4..].to_string()).or_insert((rot, pos));
                    break;
                }
            }
        }
    }
    for rr in skin::find_rigs(recs) {
        let nj = le_u32(rr.rig, 4) as usize;
        // a one-joint model whose own record carries no offset keeps its place in the root joint (RHOD20 `primA44`, the floor of the
        // opening room, sat at the origin before this rule)
        let record_offset = out.model_xf.get(&rr.model).map_or([0.0; 3], |x| x.offset);
        let root_moves = nj == 1 && record_offset == [0.0; 3] && {
            let rig = skin::parse_rig(rr.rig);
            rig.mats.first().is_some_and(|m| m[12] != 0.0 || m[13] != 0.0 || m[14] != 0.0)
        };
        if nj > 1 || rr.anm.is_some() || root_moves {
            out.rigged.insert(rr.model);
        }
    }
    out
}

