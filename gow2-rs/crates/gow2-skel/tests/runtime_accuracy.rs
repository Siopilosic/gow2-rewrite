//! Accuracy of the rotation decoding against the game's own joint rotations.
//!
//! `tests/oracle/runtime_pose.tsv` holds Kratos's runtime LOCAL joint matrices read from PCSX2 RAM captures (written by
//! `tools/export_runtime_pose.py`). In both captures the game plays `navIdle`; the frames that match its pelvis
//! translation are 9 (`ingame1`) and 18 (`ingame2`). Partial rotation channels (fewer than four stored components) are
//! rotation vectors; four-component channels are quaternions.

use std::{collections::BTreeMap, path::PathBuf};

use gow2_formats::{skin, wad};
use gow2_skel::{Clip, Skeleton};

fn ang(a: [f64; 4], b: [f64; 4]) -> f64 {
    let na = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    2.0 * (a.iter().zip(&b).map(|(x, y)| x * y).sum::<f64>() / (na * nb)).abs().min(1.0).acos().to_degrees()
}

#[test]
fn navidle_matches_runtime_pose() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Ok(data) = std::fs::read(root.join("../../../extracted/pak/R_HERO01.WAD")) else { return };
    let Ok(rt) = std::fs::read_to_string(root.join("tests/oracle/runtime_pose.tsv")) else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let a = recs.iter().find(|r| Some(&r.name) == rr.anm.as_ref() && !r.data.is_empty()).unwrap().data;
    let mut caps: BTreeMap<String, Vec<[f32; 16]>> = BTreeMap::new();
    for line in rt.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        let mut m = [0f32; 16];
        for k in 0..16 {
            m[k] = f[2 + k].parse().unwrap();
        }
        caps.entry(f[0].to_string()).or_default().push(m);
    }
    let cc = skin::clip_channels(a, skel.len() as u32, Some("navIdle")).unwrap();
    let clip = Clip::bake(&cc, &skel);
    for (cap, frame) in [("ingame1", 9usize), ("ingame2", 18usize)] {
        let rtq: Vec<[f64; 4]> = caps[cap].iter().map(|m| skin::mat_to_trs(m).1).collect();
        let local = clip.sample(&skel, frame as f32 * clip.dt);
        let (mut quat_worst, mut sum, mut n) = (0f64, 0.0, 0);
        for (j, k) in &cc.joints {
            let name = &skel.names[*j as usize];
            // fingers, weapon and chain joints, and the numbered skirt joints are driven by game code (fist pose, blades, cloth)
            if k[0].is_empty() || name.starts_with("joint") || ["Index", "Ring", "Thumb", "Weap", "Chain"].iter().any(|s| name.contains(s)) {
                continue;
            }
            let e = ang(local[*j as usize].q, rtq[*j as usize]);
            if k[0].len() == 4 {
                quat_worst = quat_worst.max(e);
            }
            sum += e;
            n += 1;
        }
        let mean = sum / n as f64;
        eprintln!("{cap}: mean body-joint rotation error {mean:.2} deg over {n} joints; worst four-component joint {quat_worst:.2} deg");
        assert!(quat_worst < 0.5, "{cap}: four-component joints must be exact, worst {quat_worst}");
        assert!(mean < 4.0, "{cap}: mean error {mean}");
        // translation: the pelvis position matches the game
        let t = skin::mat_to_trs(&caps[cap][2]).0;
        let d = ((local[2].t[0] - t[0]).powi(2) + (local[2].t[1] - t[1]).powi(2) + (local[2].t[2] - t[2]).powi(2)).sqrt();
        assert!(d < 0.05, "{cap}: pelvis translation off by {d}");
    }
}
