//! Prints the fields of a model's MAT_ records that the draw loop turns into per-object constants: blend word +0x38, shader mask +0x40, RGBA tint +0x60..+0x70 and +0x74.
//! `cargo run --release --example mat_tint -- <WAD> <model>`
use gow2_formats::{mdl, wad};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let first: std::collections::BTreeMap<&str, &[u8]> = recs.iter().filter(|r| !r.data.is_empty()).map(|r| (r.name.as_str(), r.data)).collect();
    for m in mdl::model_materials(&recs, &a[2]) {
        let Some(b) = first.get(m.as_str()).filter(|b| b.len() == 120) else {
            println!("{m}: no 120-byte record");
            continue;
        };
        let u = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        println!(
            "{m:20} +38 {:08x} +40 {:08x} tint {:.2} {:.2} {:.2} {:.2} +74 {} floats +08 {:.2} {:.2} {:.2} +28 {:.2}",
            u(0x38),
            u(0x40),
            f(0x60),
            f(0x64),
            f(0x68),
            f(0x6c),
            u(0x74),
            f(0x08),
            f(0x0c),
            f(0x10),
            f(0x28)
        );
    }
}
