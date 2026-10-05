//! Lists the script actions (kind 0x0d) of the moves whose name contains a string, with the script class resolved from its hash.
//! `cargo run --release -p gow2-formats --example script_probe -- ../extracted/pak/R_HERO01.WAD Medusa`
use gow2_formats::{dc::Dc, wad};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let dc = Dc::from_records(&recs).expect("DC");
    let ms = dc.move_set();
    for m in ms.moves.iter().filter(|m| m.name.contains(a[2].as_str())) {
        println!("{} (clip {}, {} actions)", m.name, m.anim, m.actions.len());
        for ac in &m.actions {
            let hash = ac.u32_at(8);
            let class = dc.names.get(&hash).cloned().unwrap_or_else(|| format!("{hash:#010x}"));
            if ac.kind == 0x0d || a.get(3).is_some() {
                println!("    kind {:#04x} {:<26} win {:.2}..{:.2} trig {} params {:02x?}", ac.kind, class, ac.win.0, ac.win.1, ac.trigger, &ac.raw[0x0c..0x20]);
            }
        }
    }
}
