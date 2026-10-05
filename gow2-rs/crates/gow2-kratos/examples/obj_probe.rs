//! Samples a clip of a rigged object (the Medusa head, the wind bow, a sub-weapon) and prints the local TRS of its first joints and the bounds of the world joint positions.
//! `cargo run --example obj_probe -- <WAD> <model> <clip> [fractions]`

use gow2_formats::{skin, wad};
use gow2_skel::{world_matrices, Clip, Skeleton};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let first: std::collections::BTreeMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == a[2]).expect("rig");
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let anm = first[rr.anm.as_deref().unwrap()];
    println!("{} joints; bind root {:?}", skel.len(), skel.bind[0]);
    let Some(cc) = skin::clip_channels(anm, skel.len() as u32, Some(&a[3])) else {
        println!("{}: not decodable", a[3]);
        return;
    };
    let animated: Vec<String> = cc.joints.keys().map(|j| skel.names[*j as usize].clone()).collect();
    println!("animated joints: {animated:?}");
    let clip = Clip::bake(&cc, &skel);
    let fr: Vec<f32> = a.get(4).map(|s| s.split(',').filter_map(|v| v.parse().ok()).collect()).unwrap_or_else(|| vec![0.0, 0.5]);
    for f in fr {
        let pose = clip.sample(&skel, f * clip.duration);
        let w = world_matrices(&skel, &pose);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for m in &w {
            for k in 0..3 {
                lo[k] = lo[k].min(m[12 + k]);
                hi[k] = hi[k].max(m[12 + k]);
            }
        }
        println!("t {f}: dur {:.2}", clip.duration);
        for j in 0..skel.len().min(4) {
            println!("   {:12} t {:.1?} s {:.2?} q {:.2?}", skel.names[j], pose[j].t, pose[j].s, pose[j].q);
        }
        println!("   joint bounds {lo:.1?} .. {hi:.1?}");
    }
}
