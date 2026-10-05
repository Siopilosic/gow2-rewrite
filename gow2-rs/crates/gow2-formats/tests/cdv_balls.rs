//! The collision balls of Kratos and of his blade (`docs/combat.md` 2.1): the documented radii, the volume ids, and the joints the
//! balls hang on (which also have to be sensible joints of the hero rig).

use std::path::PathBuf;

use gow2_formats::{cdv, skin, wad};

fn hull(w: &str, name: &str) -> Option<cdv::BallHull> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak").join(format!("{w}.WAD"));
    let data = std::fs::read(p).ok()?;
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let r = recs.iter().find(|r| r.tag == wad::Tag::Object && r.name == name && !r.data.is_empty())?;
    cdv::parse(r.data)
}

#[test]
fn kratos_has_seven_balls_with_the_documented_radii_and_volume_ids() {
    let Some(h) = hull("R_HERO00", "CDV_gohero") else { return };
    assert_eq!(h.balls.len(), 7);
    // docs/combat.md 2.1: radii 6, 6, 6.8, 6.8, 12, 13 and 27 units
    let r: Vec<f32> = h.balls.iter().map(|b| (b.radius * 10.0).round() / 10.0).collect();
    assert_eq!(r, [6.0, 6.0, 6.8, 6.8, 12.0, 13.0, 27.0]);
    let ids: Vec<u32> = h.balls.iter().map(|b| h.id_of(b)).collect();
    // left arm 4, right arm 5, left leg 6, right leg 7, body 1 twice, throw 9
    assert_eq!(ids, [4, 5, 6, 7, 1, 1, 9]);
    let names: Vec<&str> = h.materials.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["leftArmCollisionMat", "rightArmCollisionMat", "leftLegCollisionMat", "rightLegCollisionMat", "bodyCollisionMat", "throwCollisionMat"]);
}

#[test]
fn the_balls_hang_on_the_fists_the_feet_and_the_pelvis_of_the_rig() {
    let Some(h) = hull("R_HERO00", "CDV_gohero") else { return };
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_HERO00.WAD");
    let data = std::fs::read(p).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let names = gow2_formats_joint_names(&rr);
    let joint = |i: usize| names[h.balls[i].joint as usize].to_ascii_lowercase();
    assert!(joint(0).starts_with('l') && (joint(0).contains("wrist") || joint(0).contains("arm") || joint(0).contains("hand")), "left arm ball on {}", joint(0));
    assert!(joint(1).starts_with('r') && (joint(1).contains("wrist") || joint(1).contains("arm") || joint(1).contains("hand")), "right arm ball on {}", joint(1));
    assert!(joint(2).starts_with('l') && (joint(2).contains("metatarsal") || joint(2).contains("tibia") || joint(2).contains("femur") || joint(2).contains("leg")), "left leg ball on {}", joint(2));
    assert!(joint(3).starts_with('r') && (joint(3).contains("metatarsal") || joint(3).contains("tibia") || joint(3).contains("femur") || joint(3).contains("leg")), "right leg ball on {}", joint(3));
    for i in 4..7 {
        assert_eq!(joint(i), "pelvis", "body ball {i}");
    }
}

fn gow2_formats_joint_names(rr: &skin::RigRef<'_>) -> Vec<String> {
    skin::parse_rig(rr.rig).names
}

#[test]
fn the_blade_ball_is_a_sphere_in_the_middle_of_the_blade() {
    let Some(h) = hull("R_WEAPON0_5", "CDV_gomaiblade") else { return };
    assert_eq!(h.balls.len(), 2);
    let b = &h.balls[0];
    assert_eq!(h.id_of(b), 2, "weaponLeftMat");
    assert!((b.radius - 844.8).abs() < 0.1);
    // at the rig's 1/64 scale: centre z -4.98 (the blade mesh spans z -16.8 to 1.3) and radius 13.2 units
    assert!((b.centre[2] / 64.0 + 4.985).abs() < 0.01 && (b.radius / 64.0 - 13.2).abs() < 0.01);
    assert_eq!(h.id_of(&h.balls[1]), 0x0b, "weaponTwinCollisionMat");
}



