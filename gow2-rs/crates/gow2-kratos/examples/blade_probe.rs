//! Where are the blade slot joints relative to the hands? `cargo run --example blade_probe -- <hero WAD> <clip>...`

use gow2_formats::{dc, skin, wad};
use gow2_skel::{world_matrices, Clip, Skeleton};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let d = dc::Dc::from_records(&recs).expect("dc");
    let atts = d.chained_attachments();
    let first: std::collections::HashMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let anm = first.get(rr.anm.as_deref().unwrap()).unwrap();
    let idx = |n: &str| skel.names.iter().position(|x| x.eq_ignore_ascii_case(n));
    for at in &atts {
        println!("{}: stowed {} hand {} free {} chain {:?} snap {} m", at.object, at.stowed, at.hand, at.free, at.chain, at.snap_m);
    }
    println!("joints with Weap/Blade/Wrist/Hand: {:?}", skel.names.iter().enumerate().filter(|(_, n)| ["weap", "blade", "wrist", "hand", "chain"].iter().any(|k| n.to_lowercase().contains(k))).map(|(i, n)| format!("{i}:{n}")).collect::<Vec<_>>());
    for name in &a[2..] {
        let Some(cc) = skin::clip_channels(anm, skel.len() as u32, Some(name)) else { println!("{name}: not found"); continue };
        let clip = Clip::bake(&cc, &skel);
        for f in [0.0f32, 0.5] {
            let w = world_matrices(&skel, &clip.sample(&skel, clip.duration * f));
            println!("{name} at {:.0} %:", f * 100.0);
            for at in &atts {
                let p = |n: &str| idx(n).map(|i| [w[i][12], w[i][13], w[i][14]]);
                let det = |n: &str| idx(n).map(|i| {
                    let m = &w[i];
                    m[0] * (m[5] * m[10] - m[6] * m[9]) - m[1] * (m[4] * m[10] - m[6] * m[8]) + m[2] * (m[4] * m[9] - m[5] * m[8])
                });
                let dist = |x: Option<[f32; 3]>, y: Option<[f32; 3]>| match (x, y) {
                    (Some(x), Some(y)) => ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt(),
                    _ => f32::NAN,
                };
                let (fp, hp, bp) = (p(&at.free), p(&at.hand), p(&at.stowed));
                println!("  {}: |free-hand| {:.1} |free-back| {:.1}; hand {:?} det(hand) {:?} det(free) {:?} det(back) {:?}", at.object, dist(fp, hp), dist(fp, bp), hp.map(|v| v.map(|x| x.round())), det(&at.hand).map(|x| (x * 1000.0).round() / 1000.0), det(&at.free).map(|x| (x * 1000.0).round() / 1000.0), det(&at.stowed).map(|x| (x * 1000.0).round() / 1000.0));
            }
        }
    }
}
