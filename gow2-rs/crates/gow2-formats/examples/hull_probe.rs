//! Lists the collision-ball hulls (`CDV_go*`) of a level WAD with the world position of each ball, placed by the object's own `go*` node
//! (rotation rows at +0x20, translation at +0x44 of the 104-byte record).
//! `cargo run --release -p gow2-formats --example hull_probe -- ../extracted/pak/RHOD10.WAD`
use gow2_formats::{cdv, wad};

fn le_f32(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn main() {
    let path = std::env::args().nth(1).expect("level WAD path");
    let data = std::fs::read(&path).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    for r in recs.iter().filter(|r| r.tag == wad::Tag::Object && r.name.starts_with("CDV_") && !r.data.is_empty()) {
        let go = &r.name[4..];
        let Some(h) = cdv::parse(r.data) else { continue };
        let node = recs.iter().find(|n| n.tag == wad::Tag::Object && n.name == go && n.data.len() == 104);
        let (rot, pos) = node.map_or(([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], [0.0; 3]), |n| {
            let mut rot = [0f32; 9];
            for (k, v) in rot.iter_mut().enumerate() {
                *v = le_f32(n.data, 0x20 + 4 * k);
            }
            (rot, [le_f32(n.data, 0x44), le_f32(n.data, 0x48), le_f32(n.data, 0x4c)])
        });
        println!("{} node {} balls {} at {:.0?} materials {:?}", r.name, node.is_some(), h.balls.len(), pos, h.materials.iter().map(|m| (m.name.as_str(), m.id)).collect::<Vec<_>>());
        for b in h.balls.iter().take(6) {
            let c = b.centre;
            let w = [
                c[0] * rot[0] + c[1] * rot[3] + c[2] * rot[6] + pos[0],
                c[0] * rot[1] + c[1] * rot[4] + c[2] * rot[7] + pos[1],
                c[0] * rot[2] + c[1] * rot[5] + c[2] * rot[8] + pos[2],
            ];
            println!("    joint {} local {:.0?} r {:.1} world {:.0?} material {}", b.joint, c, b.radius, w, h.materials.get(b.material as usize).map_or("-", |m| m.name.as_str()));
        }
    }
}

