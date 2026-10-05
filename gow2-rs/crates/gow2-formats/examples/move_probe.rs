//! Prints the entry branches that lead to the moves whose name contains a string: `cargo run --example move_probe -- <hero WAD> <substring>`.

use gow2_formats::{dc::Dc, wad};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read wad");
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let dc = Dc::from_records(&recs).expect("no DC data");
    let ms = dc.move_set();
    let pat = a[2].as_str();
    println!("{} moves, {} entry branches", ms.moves.len(), ms.entry.len());
    for (i, b) in ms.entry.iter().enumerate() {
        let tgt = b.target.map(|t| ms.moves[t].name.as_str()).unwrap_or("-");
        if tgt.contains(pat) || b.name.contains(pat) {
            println!("entry {i:3} {:<28} -> {tgt:<28} button {:#04x} press {} stick {:#04x} flags {:#x}/{:#x} win {:?} health {:?}/{:?} unlock {} lvl {} b1e {} b1f {}", b.name, b.button, b.press, b.stick, b.flags_a, b.flags_b, b.win, b.tgt_health, b.own_health, b.unlock, b.min_level, b.b1e, b.b1f);
        }
    }
    for (i, m) in ms.moves.iter().enumerate() {
        if m.name.contains(pat) {
            println!("move {i} {} clip {} flags {:#x} rate {} blend {} hits {} actions {}", m.name, m.anim, m.flags, m.rate, m.blend, m.hits.len(), m.actions.len());
            if a.get(3).is_some() {
                for ac in &m.actions {
                    println!("    action {:<28} kind {:#04x} trig {} flags {:#x} win {:.2}..{:.2} raw {:02x?}", ac.name, ac.kind, ac.trigger, ac.flags, ac.win.0, ac.win.1, &ac.raw[8..0x20]);
                }
                for b in &m.branches {
                    let tgt = b.target.map(|t| ms.moves[t].name.as_str()).unwrap_or("-");
                    println!("    branch {:<26} -> {tgt:<22} button {:#04x} press {} stick {:#04x} flagsA {:#x} flagsB {:#x} win {:?} unlock {}", b.name, b.button, b.press, b.stick, b.flags_a, b.flags_b, b.win, b.unlock);
                }
            }
        }
    }
}


