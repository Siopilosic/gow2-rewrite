//! Looks for joints that make parts of the hero vanish in a clip pose: odd scales, and the size of each part's skinned bounding box against its bind box.
//! `cargo run --example pose_check -- <hero WAD> <clip> <seconds>`

use std::collections::BTreeMap;

use gow2_formats::{skin, wad};
use gow2_skel::{local_parts, skin_matrices, skin_positions_blend, world_matrices, Clip, Skeleton};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let first: BTreeMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let anm = first[rr.anm.as_deref().unwrap()];
    let sm = skin::mesh_joints(first[wad::mesh_record_name("hero").as_str()], 4096.0);
    let part_local = local_parts(&sm, &skel);
    let local: Vec<bool> = sm.part.iter().map(|&p| part_local[p as usize]).collect();
    println!("local parts: {:?}", part_local.iter().enumerate().filter(|(_, l)| **l).map(|(i, _)| i).collect::<Vec<_>>());
    let cc = skin::clip_channels(anm, skel.len() as u32, Some(&a[2])).expect("clip");
    let clip = Clip::bake(&cc, &skel);
    let pose = clip.sample(&skel, a[3].parse().unwrap());
    for (j, p) in pose.iter().enumerate() {
        let bad = p.s.iter().any(|s| !(0.6..1.6).contains(&s.abs())) || p.t.iter().any(|t| !t.is_finite()) || p.q.iter().any(|q| !q.is_finite());
        if bad {
            println!("joint {j} {} scale {:.2?} t {:.1?}", skel.names[j], p.s, p.t);
        }
    }
    let world = world_matrices(&skel, &pose);
    let sk = skin_matrices(&skel, &world);
    let bind: Vec<[f32; 3]> = sm.verts.iter().map(|v| [v[0] as f32 / 16.0, v[1] as f32 / 16.0, v[2] as f32 / 16.0]).collect();
    let out = skin_positions_blend(&bind, &sm.joints, &sm.joints2, &sm.weight, &local, &sk, &world);
    let mut parts: BTreeMap<u32, ([f32; 3], [f32; 3], [f32; 3], [f32; 3], usize)> = BTreeMap::new();
    for (i, p) in out.iter().enumerate() {
        let e = parts.entry(sm.part[i] as u32).or_insert(([f32::MAX; 3], [f32::MIN; 3], [f32::MAX; 3], [f32::MIN; 3], 0));
        for k in 0..3 {
            e.0[k] = e.0[k].min(bind[i][k]);
            e.1[k] = e.1[k].max(bind[i][k]);
            e.2[k] = e.2[k].min(p[k]);
            e.3[k] = e.3[k].max(p[k]);
        }
        e.4 += 1;
    }
    for (id, (bl, bh, pl, ph, n)) in parts {
        let vol = |l: [f32; 3], h: [f32; 3]| (h[0] - l[0]) * (h[1] - l[1]) * (h[2] - l[2]);
        println!("part {id:2}{} {n:5} verts bind box {:7.0} posed box {:7.0}  posed y {:.0}..{:.0}", if part_local[id as usize] { " (local)" } else { "" }, vol(bl, bh), vol(pl, ph), pl[1], ph[1]);
    }
}
