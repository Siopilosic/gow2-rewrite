//! Prints the joints whose local scale in a clip differs from the bind scale: `cargo run --release --example clip_scales -- <hero WAD> <clip>[,<clip>...]`
use gow2_formats::{skin, wad};
use gow2_skel::{Clip, Skeleton};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let first: std::collections::BTreeMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let anm = first[rr.anm.as_deref().unwrap()];
    for name in a[2].split(',') {
        let Some(cc) = skin::clip_channels(anm, skel.len() as u32, Some(name)) else {
            println!("{name}: not decodable");
            continue;
        };
        let clip = Clip::bake(&cc, &skel);
        println!("{name}: duration {:.2}", clip.duration);
        // how far each of the spine joints turns away from the bind rotation (degrees)
        for jn in ["pelvis", "vertebrae1", "vertebrae2", "vertebrae3", "vertebrae4", "neck", "head"] {
            if let Some(j) = skel.names.iter().position(|n| n == jn) {
                let pose = clip.sample(&skel, 0.0);
                let (q, b) = (pose[j].q, skel.bind[j].q);
                let dot: f64 = q.iter().zip(&b).map(|(x, y)| x * y).sum::<f64>().abs().min(1.0);
                let bi: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
                println!("  {jn:11} rotation from bind {:.1} deg (has rot channel {}, bind |q| {bi:.2})", 2.0 * dot.acos().to_degrees(), clip.joints[j].rot.is_some());
            }
        }
        for f in [0.0f32, 0.5] {
            let pose = clip.sample(&skel, f * clip.duration);
            for (j, p) in pose.iter().enumerate() {
                let b = skel.bind[j];
                let d = (0..3).map(|k| (p.s[k] - b.s[k]).abs()).fold(0.0, f64::max);
                let has = clip.joints[j].scale.is_some();
                if d > 0.03 || has {
                    println!("  t {f:.1} joint {j:3} {:14} scale {:.2} {:.2} {:.2} (bind {:.2} {:.2} {:.2}) channel {has}", skel.names[j], p.s[0], p.s[1], p.s[2], b.s[0], b.s[1], b.s[2]);
                }
            }
        }
    }
}
