//! Characters use two-joint blended skinning: a tag's low bits name the base joint, its high nibble a second joint,
//! and the position w field is the second joint's pull (12-bit, 4096 = 1.0). Regression guard for that finding.
use std::path::PathBuf;

use gow2_formats::{skin, wad};

fn blended(name: &str, model: &str) -> Option<(usize, usize)> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak").join(name);
    let data = std::fs::read(p).ok()?;
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let blob = recs.iter().find(|r| r.name == format!("MDL_{model}_0") && !r.data.is_empty())?.data;
    let sm = skin::mesh_joints(blob, 4096.0);
    assert_eq!(sm.joints.len(), sm.joints2.len());
    assert_eq!(sm.joints.len(), sm.weight.len());
    let blended = (0..sm.joints.len()).filter(|&i| sm.weight[i] > 0.0 && sm.joints2[i] != sm.joints[i]).count();
    Some((blended, sm.joints.len()))
}

#[test]
fn kratos_vertices_blend_two_joints() {
    if let Some((b, n)) = blended("R_HERO00.WAD", "hero") {
        assert!(b * 4 > n, "expected a large share of blended vertices, got {b} of {n}");
    }
    if let Some((b, n)) = blended("R_HERO01.WAD", "hero") {
        assert!(b * 4 > n, "expected a large share of blended vertices, got {b} of {n}");
    }
}
