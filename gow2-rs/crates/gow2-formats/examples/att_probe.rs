//! Prints the attachment records (`ATT_*`) of a hero WAD: object, slot joints, snap distance, chained flag.
//! `cargo run --release -p gow2-formats --example att_probe -- ../extracted/pak/R_HERO01.WAD`
use gow2_formats::{dc::Dc, wad};

fn main() {
    let path = std::env::args().nth(1).expect("hero WAD");
    let data = std::fs::read(&path).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let dc = Dc::from_records(&recs).expect("DC");
    for (att, count) in [("ATT_Chains", 2), ("ATT_Medusa", 2), ("ATT_WindBow", 2), ("ATT_Bone", 2), ("ATT_Hammer", 2), ("ATT_Olympus", 2)] {
        for a in dc.attachments(att, count) {
            println!("{att:12} object {:16} hand {:10} free {:10} stowed {:14} snap {:.2} m chained {} chain {:?}", a.object, a.hand, a.free, a.stowed, a.snap_m, a.chained, a.chain);
        }
    }
}

