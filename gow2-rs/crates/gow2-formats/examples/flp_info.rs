//! Summary of an FLP movie in a WAD: counts, root labels, named root layers, strings.
//! `cargo run --release -p gow2-formats --example flp_info -- ../extracted/pak/R_SHELLA.WAD FLP_ShellA`
use gow2_formats::{flp, wad};

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().expect("WAD path");
    let name = a.next().expect("FLP record name");
    let data = std::fs::read(&path).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let m = recs.iter().find(|r| r.name == name && !r.data.is_empty()).expect("movie record");
    let Some(f) = flp::Flp::parse(m.data) else {
        println!("layout walk failed");
        return;
    };
    println!("chars {} shapes {} clips {} fonts {} root frames {} layers {}", f.chars.len(), f.shapes.len(), f.clips.len(), f.fonts.len(), f.root.frames, f.root.layers.len());
    let names: Vec<&str> = f.root.layers.iter().flatten().filter_map(|k| f.string(k.name)).collect();
    println!("root layer names: {names:?}");
    println!("root labels: {:?}", f.root.frame_info.iter().filter_map(|i| i.label.as_ref().map(|l| (i.frame, l.clone()))).collect::<Vec<_>>());
    if std::env::var_os("FLP_FIELDS").is_some() {
        for (i, t) in f.texts.iter().enumerate() {
            println!("E{i}: var {:?} text {:?} font {} size {} color {:08x} raw[16..32] {:02x?}", f.string(t.var), f.string(t.text), t.font, t.size, t.color, &t.raw[16..32]);
        }
    }
    println!("static texts: {}", f.statics.len());
    for (i, runs) in f.statics.iter().enumerate().take(40) {
        let text: Vec<String> = runs
            .iter()
            .map(|r| {
                let font = &f.fonts[r.font as usize];
                let s: String = r.glyphs.iter().map(|&(g, _)| font.map.iter().position(|&m| m == g).map_or('?', |c| c as u8 as char)).collect();
                format!("{s:?}@({:.0},{:.0}) size {:.2}", r.x, r.y, r.size)
            })
            .collect();
        println!("  D{i}: {}", text.join(" | "));
    }
    let tags: Vec<(wad::Tag, String)> = recs.iter().map(|r| (r.tag, r.name.clone())).collect();
    println!("texture group: {} {:?}", flp::texture_group(&tags, &name).len(), flp::texture_group(&tags, &name).iter().take(8).collect::<Vec<_>>());
    let mdl = recs.iter().filter(|r| r.name.starts_with("MDL_") && r.name.ends_with("_0") && !r.data.is_empty()).map(|r| r.name.as_str()).collect::<Vec<_>>();
    println!("mesh records: {mdl:?}");
}

