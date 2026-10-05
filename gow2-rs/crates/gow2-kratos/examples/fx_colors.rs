//! Vertex colour ranges of a model (RGBA as stored, 128 = 1.0) and of its UVs: `cargo run --release --example fx_colors -- <WAD> <model>`
use gow2_formats::{skin, wad};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let name = wad::mesh_record_name(&a[2]);
    let blob = recs.iter().find(|r| r.name == name && !r.data.is_empty()).expect("mesh").data;
    let sm = skin::mesh_joints(blob, 4096.0);
    for k in 0..4 {
        let (mut lo, mut hi, mut sum) = (i64::MAX, i64::MIN, 0i64);
        for c in &sm.cols {
            lo = lo.min(c[k]);
            hi = hi.max(c[k]);
            sum += c[k];
        }
        println!("channel {k}: min {lo} max {hi} mean {:.1}", sum as f64 / sm.cols.len().max(1) as f64);
    }
    // the textures of the model's materials: size, mean colour and alpha
    let store = gow2_formats::texture::TextureStore::new(recs.iter().filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)));
    for m in gow2_formats::mdl::model_materials(&recs, &a[2]) {
        match store.material_texture(&m) {
            Some(t) => {
                let n = (t.rgba.len() / 4).max(1) as f64;
                let mean = |k: usize| t.rgba.chunks(4).map(|p| p[k] as f64).sum::<f64>() / n;
                println!("material {m}: texture {}x{} mean rgba {:.0} {:.0} {:.0} {:.0}", t.width, t.height, mean(0), mean(1), mean(2), mean(3));
            }
            None => println!("material {m}: no texture"),
        }
    }
    let (mut ulo, mut uhi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for u in &sm.uvs {
        for k in 0..2 {
            ulo[k] = ulo[k].min(u[k]);
            uhi[k] = uhi[k].max(u[k]);
        }
    }
    println!("uv range u {:.2}..{:.2} v {:.2}..{:.2}; {} vertices {} triangles", ulo[0], uhi[0], ulo[1], uhi[1], sm.verts.len(), sm.tris.len());
}
