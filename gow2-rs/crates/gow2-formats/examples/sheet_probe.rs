//! Lists the collision polygons of a level near a point: `cargo run --example sheet_probe -- <level.WAD> x y z radius`.

use gow2_formats::{sheet, wad};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let sh = sheet::find(&recs).expect("no collision sheet");
    let c: Vec<f32> = a[2..5].iter().map(|s| s.parse().unwrap()).collect();
    let r: f32 = a[5].parse().unwrap();
    println!("{} polys, {} surfaces", sh.polys.len(), sh.surfaces.len());
    let mut seen = std::collections::BTreeMap::new();
    for p in &sh.polys {
        let n = p.corners as usize;
        let near = p.v[..n].iter().any(|v| ((v[0] - c[0]).powi(2) + (v[1] - c[1]).powi(2) + (v[2] - c[2]).powi(2)).sqrt() < r);
        if near {
            let s = sh.surface(p);
            *seen.entry((s.name.clone(), s.flags, s.flags_hi)).or_insert(0) += 1;
        }
    }
    for ((name, f, hi), n) in seen {
        println!("{n:4} x {name:<24} flags {f:#010x} hi {hi:#010x}");
    }
}
