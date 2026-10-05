//! Effect models of a magic WAD: for every rigged model, its record scale and offset, joints, clips (name, duration) and the bounds of its vertices (placed by the joint world
//! matrices, as `gow2-bevy` does for non-hero models) in the bind pose and in each clip at 0, 1/3, 2/3 of its length.
//! `cargo run --release --example fx_probe -- ../extracted/pak/R_M_WIND0.WAD [model]`
use std::collections::BTreeMap;

use gow2_formats::{anm, skin, wad};
use gow2_skel::{transform_point, world_matrices, Clip, Skeleton};

fn bounds(p: &[[f32; 3]]) -> String {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in p {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    format!("x {:.0}..{:.0} y {:.0}..{:.0} z {:.0}..{:.0}", lo[0], hi[0], lo[1], hi[1], lo[2], hi[2])
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let only = a.get(2).cloned();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let first: BTreeMap<&str, &[u8]> = recs.iter().filter(|r| r.tag == wad::Tag::Object && !r.data.is_empty()).fold(BTreeMap::new(), |mut m, r| {
        m.entry(r.name.as_str()).or_insert(r.data);
        m
    });
    for rr in skin::find_rigs(&recs) {
        if only.as_ref().is_some_and(|o| *o != rr.model) {
            continue;
        }
        let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
        let rec = first.get(format!("MDL_{}", rr.model).as_str());
        let (off, scale) = rec.filter(|r| r.len() >= 0x4c).map_or(([0.0; 3], 0.0), |r| {
            let f = |o: usize| f32::from_le_bytes(r[o..o + 4].try_into().unwrap());
            ([f(0x38), f(0x3c), f(0x40)], f(0x48))
        });
        println!("== {} joints {} record offset {off:?} record scale {scale}", rr.model, skel.len());
        let Some(blob) = first.get(wad::mesh_record_name(&rr.model).as_str()) else {
            println!("   no mesh");
            continue;
        };
        let sm = skin::mesh_joints(blob, 4096.0);
        let pos: Vec<[f32; 3]> = sm.verts.iter().map(|v| [v[0] as f32 / 16.0, v[1] as f32 / 16.0, v[2] as f32 / 16.0]).collect();
        let place = |local: &[gow2_skel::Trs]| -> Vec<[f32; 3]> {
            let w = world_matrices(&skel, local);
            pos.iter().enumerate().map(|(i, p)| w.get(sm.joints[i] as usize).map_or(*p, |m| transform_point(m, *p))).collect()
        };
        println!("   {} vertices; raw bounds {}; bind pose bounds {}", pos.len(), bounds(&pos), bounds(&place(&skel.bind)));
        let root = &skel.bind[0];
        println!("   root joint bind: t {:.1?} q {:.2?} s {:.3?}", root.t, root.q, root.s);
        let Some(anm_data) = rr.anm.as_deref().and_then(|n| first.get(n)) else { continue };
        for c in anm::clips(anm_data) {
            let name = skin::clip_name(anm_data, c);
            let Some(cc) = skin::clip_channels(anm_data, skel.len() as u32, Some(&name)) else { continue };
            let clip = Clip::bake(&cc, &skel);
            let b: Vec<String> = [0.0f32, 0.33, 0.66].iter().map(|f| bounds(&place(&clip.sample(&skel, f * clip.duration)))).collect();
            println!("   clip {name:24} {:.2} s, {} animated joints; bounds t0 {} | t1 {} | t2 {}", clip.duration, cc.joints.len(), b[0], b[1], b[2]);
        }
    }
}
