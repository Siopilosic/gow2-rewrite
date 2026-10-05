//! Ground speeds of the walk clips, measured from the planted foot (the cycle length of the distance-driven walk depends on them).
use std::path::PathBuf;

use gow2_formats::{skin, wad};
use gow2_kratos::anim::ground_speed;
use gow2_skel::{Clip, Skeleton};

#[test]
fn walk_clip_ground_speeds() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_HERO01.WAD");
    let Ok(data) = std::fs::read(p) else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let a = recs.iter().find(|r| Some(&r.name) == rr.anm.as_ref() && !r.data.is_empty()).unwrap().data;
    let mut speeds = Vec::new();
    for name in ["navIdle", "navWalkSlow", "navWalkFast", "navWalkFastL", "navWalkFastR", "attBrutalRun"] {
        let cc = skin::clip_channels(a, skel.len() as u32, Some(name)).unwrap();
        let clip = Clip::bake(&cc, &skel);
        let v = ground_speed(&skel, &clip);
        eprintln!("{name:14} duration {:.2} s, ground speed {:6.1} units/s = {:.2} m/s, cycle {:.1} units", clip.duration, v, v / 16.0, v * clip.duration);
        speeds.push((name, v));
    }
    let get = |n: &str| speeds.iter().find(|s| s.0 == n).unwrap().1;
    assert!(get("navIdle") < 5.0, "the idle stays in place");
    assert!(get("navWalkFast") > get("navWalkSlow"), "fast walk covers ground faster");
}
