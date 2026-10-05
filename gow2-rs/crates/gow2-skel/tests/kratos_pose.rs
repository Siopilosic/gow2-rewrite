//! Kratos pose sanity checks against `extracted/pak/R_HERO00.WAD` (skipped when absent).

use std::path::PathBuf;

use gow2_formats::{skin, wad};
use gow2_skel::{mat_mul, skin_matrices, world_matrices, Clip, Skeleton};

fn load() -> Option<Vec<u8>> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_HERO00.WAD");
    std::fs::read(p).ok()
}

#[test]
fn bind_pose_skins_to_identity() {
    let Some(data) = load() else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    assert_eq!(skel.len(), 123);
    // world matrices rebuilt from the baked bind TRS must equal the stored-matrix world (to f32 noise)
    let world = world_matrices(&skel, &skel.bind);
    for (j, (w, b)) in world.iter().zip(&skel.bind_world).enumerate() {
        for k in 0..16 {
            assert!((w[k] - b[k]).abs() < 1e-2 * (1.0 + b[k].abs()), "joint {j} elem {k}: {} vs {}", w[k], b[k]);
        }
    }
    for (j, m) in skin_matrices(&skel, &world).iter().enumerate() {
        let id = gow2_skel::IDENTITY;
        for k in 0..16 {
            assert!((m[k] - id[k]).abs() < 1e-2, "joint {j} skin elem {k}: {}", m[k]);
        }
    }
    let _ = mat_mul(&skel.bind_world[0], &skel.inv_bind[0]);
}

#[test]
fn attack_clip_moves_joints() {
    let Some(data) = load() else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let anm = recs.iter().find(|r| Some(&r.name) == rr.anm.as_ref() && !r.data.is_empty()).unwrap().data;
    let cc = skin::clip_channels(anm, skel.len() as u32, Some("attBrutalSlash")).expect("clip");
    let clip = Clip::bake(&cc, &skel);
    assert!(clip.duration > 0.1);
    let a = world_matrices(&skel, &clip.sample(&skel, 0.0));
    let b = world_matrices(&skel, &clip.sample(&skel, clip.duration * 0.5));
    let moved = a.iter().zip(&b).filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| (p - q).abs() > 1e-3)).count();
    assert!(moved > 10, "only {moved} joints moved");
    assert!(b.iter().all(|m| m.iter().all(|v| v.is_finite())));
}
