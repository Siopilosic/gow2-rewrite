//! Where do the hero's clips move the body? Prints the pelvis and the three origin joints (`zeroJoint`, `synchJoint`, `linkJoint`) at the start and
//! end of each clip: `cargo run --example root_motion -- <hero WAD> <clip>...`

use gow2_formats::{skin, wad};
use gow2_skel::{world_matrices, Clip, Skeleton};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let first: std::collections::HashMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").expect("hero rig");
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let anm = first.get(rr.anm.as_deref().unwrap()).expect("anm");
    let idx = |n: &str| skel.names.iter().position(|x| x == n);
    let joints: Vec<(&str, usize)> = ["pelvis", "zeroJoint", "synchJoint", "linkJoint"].iter().filter_map(|n| idx(n).map(|i| (*n, i))).collect();
    println!("joints {:?}", joints);
    for name in &a[2..] {
        let Some(cc) = skin::clip_channels(anm, skel.len() as u32, Some(name)) else {
            println!("{name}: not found");
            continue;
        };
        let clip = Clip::bake(&cc, &skel);
        let pos = |t: f32| {
            let w = world_matrices(&skel, &clip.sample(&skel, t));
            joints.iter().map(|&(n, j)| (n, [w[j][12], w[j][13], w[j][14]])).collect::<Vec<_>>()
        };
        let (p0, p1, pm) = (pos(0.0), pos(clip.duration), pos(clip.duration * 0.5));
        println!("{name}: {:.2} s", clip.duration);
        for ((n, a), ((_, m), (_, b))) in p0.iter().zip(pm.iter().zip(p1.iter())) {
            println!("   {n:<11} start ({:7.1} {:7.1} {:7.1}) mid ({:7.1} {:7.1} {:7.1}) end ({:7.1} {:7.1} {:7.1})  delta ({:6.1} {:6.1} {:6.1})", a[0], a[1], a[2], m[0], m[1], m[2], b[0], b[1], b[2], b[0] - a[0], b[1] - a[1], b[2] - a[2]);
        }
    }
}
