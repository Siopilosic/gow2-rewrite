//! Prints how each model of a level WAD is placed: instance, go node, or record only (and which are rigged).
//! `cargo run --release -p gow2-formats --example place_probe -- ../extracted/pak/RHOD20.WAD`
use gow2_formats::{level, wad};

fn main() {
    let path = std::env::args().nth(1).expect("level WAD path");
    let data = std::fs::read(&path).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let pl = level::parse(&recs);
    println!("{} models, {} instances, {} go nodes, {} rigged", pl.models.len(), pl.instances.len(), pl.go_xf.len(), pl.rigged.len());
    for m in &pl.models {
        let n_inst = pl.instances.iter().filter(|i| &i.model == m).count();
        let go = pl.go_xf.get(m);
        let rigged = pl.rigged.contains(m);
        if rigged || n_inst > 0 || go.is_some() {
            println!("{m:24} rigged {rigged:5} instances {n_inst:3} go {}", go.map(|(_, p)| format!("{:.0?}", p)).unwrap_or_else(|| "-".into()));
        }
    }
    let xf = |m: &str| pl.xf(m);
    println!("primA44 xf {:?}", xf("primA44"));
    for i in pl.instances.iter().take(5) {
        println!("instance {} {} at {:.0?}", i.name, i.model, i.pos);
    }
}
