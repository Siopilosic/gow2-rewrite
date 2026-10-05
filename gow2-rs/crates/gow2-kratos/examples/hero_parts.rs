//! Lists the parts of the hero model: triangles, material slot and material name, bounding box, texture alpha. `cargo run --example hero_parts -- <hero WAD>`

use std::collections::BTreeMap;

use gow2_formats::{mdl, skin, texture::TextureStore, wad};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let first: BTreeMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
    let blob = first[wad::mesh_record_name("hero").as_str()];
    let sm = skin::mesh_joints(blob, 4096.0);
    let mats = mdl::model_materials(&recs, "hero");
    let store = TextureStore::new(recs.iter().filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)));
    println!("{} vertices, {} triangles, {} materials: {:?}", sm.verts.len(), sm.tris.len(), mats.len(), mats);
    let mut by_slot: BTreeMap<u16, (usize, [f32; 3], [f32; 3])> = BTreeMap::new();
    for (t, s) in &sm.tris {
        let e = by_slot.entry(*s).or_insert((0, [f32::MAX; 3], [f32::MIN; 3]));
        e.0 += 1;
        for &v in t {
            for k in 0..3 {
                let x = sm.verts[v as usize][k] as f32 / 16.0;
                e.1[k] = e.1[k].min(x);
                e.2[k] = e.2[k].max(x);
            }
        }
    }
    for (slot, (n, lo, hi)) in &by_slot {
        let name = mats.get(*slot as usize).cloned().unwrap_or_default();
        let alpha = store.material_texture(&name).map(|t| {
            let (mut min, mut max, mut part) = (255u8, 0u8, 0usize);
            for p in t.rgba.chunks(4) {
                min = min.min(p[3]);
                max = max.max(p[3]);
                if p[3] < 250 {
                    part += 1;
                }
            }
            format!("{}x{} alpha {min}..{max}, {:.0} % not opaque", t.width, t.height, 100.0 * part as f32 / (t.rgba.len() / 4) as f32)
        });
        println!("slot {slot:2} {name:<24} {n:5} tris  y {:.0}..{:.0}  x {:.0}..{:.0}  {alpha:?}", lo[1], hi[1], lo[0], hi[0]);
    }
    let parts: std::collections::BTreeSet<_> = sm.part.iter().collect();
    println!("{} distinct part ids", parts.len());
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "hero").unwrap();
    let skel = gow2_skel::Skeleton::from_rig(&skin::parse_rig(rr.rig));
    for (i, n) in skel.names.iter().enumerate().skip(80) {
        let m = &skel.bind_world[i];
        println!("joint {i:3} {n:<16} parent {:3} bind pos ({:6.1} {:6.1} {:6.1})", skel.parents[i], m[12], m[13], m[14]);
    }
}


