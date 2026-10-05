//! The armoured Kratos (R_HERO01) has joint-local armour plates; the base model does not.
use std::path::PathBuf;

use gow2_formats::{skin, wad};
use gow2_skel::{local_parts, Skeleton};

fn load(name: &str) -> Option<(Vec<u8>, String)> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak").join(name);
    std::fs::read(p).ok().map(|d| (d, name.to_string()))
}

fn locals(name: &str) -> Option<Vec<usize>> {
    let (data, _) = load(name)?;
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero")?;
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let blob = recs.iter().find(|r| r.name == "MDL_hero_0" && !r.data.is_empty())?.data;
    let sm = skin::mesh_joints(blob, 4096.0);
    Some(local_parts(&sm, &skel).iter().enumerate().filter(|(_, l)| **l).map(|(i, _)| i).collect())
}

#[test]
fn armour_pauldrons_are_joint_local() {
    if let Some(l) = locals("R_HERO01.WAD") {
        assert_eq!(l, vec![0], "part 0 (pauldrons on the clavicles and humeri) is the only joint-local part");
    }
    if let Some(l) = locals("R_HERO00.WAD") {
        assert!(l.is_empty(), "the base model has no joint-local parts");
    }
}
