//! Dumps the first clip of an `ANM_` record that animates materials or emitters (track types 3, 8, 10): for every block the keys of every slot.
//! `cargo run --release -p gow2-fx --example anm_dump -- ../extracted/pak/R_WEAPON0_5.WAD ANM_blade3b`
use gow2_formats::{anm, wad};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let r = recs.iter().find(|r| r.name == a[2] && !r.data.is_empty()).expect("record");
    let b = r.data;
    let u16at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let (nt, ng) = (u16at(0x10) as usize, u16at(0x12) as usize);
    println!("{} tracks, {} groups", nt, ng);
    let mut blk = 0;
    let c = anm::clips(b);
    println!("{} clips", c.len());
    let c = c[0];
    let dur = f32::from_le_bytes(b[c + 0x14..c + 0x18].try_into().unwrap());
    println!("clip duration {dur}");
    for i in 0..nt {
        let o = 0x18 + 4 * ng + 8 * i;
        let (ttype, nsub) = (u16at(o), b[o + 3] as usize);
        println!("track {i}: type {ttype} subs {nsub} (bytes {:02x?})", &b[o..o + 8]);
        for _ in 0..nsub {
            let q = c + 0x60 + 16 * blk;
            let (nseg, tab) = (u16at(q + 2) as usize, u32::from_le_bytes(b[q + 8..q + 12].try_into().unwrap()) as usize);
            let dt = f32::from_le_bytes(b[q + 12..q + 16].try_into().unwrap());
            let kind = if matches!(ttype, 3 | 8 | 10) { anm::Kind::Trans } else { anm::Kind::Rot };
            print!("  block {blk}: dt {dt:.4}, {nseg} segments;");
            for s in 0..nseg {
                if let Some(seg) = anm::decode_segment(b, c + tab + 12 * s, kind) {
                    for (slot, cv) in &seg.curves {
                        let keys: Vec<String> = cv.iter().take(6).map(|(f, v)| format!("{f}:{v:.3}")).collect();
                        print!(" slot {slot} [{}{}]", keys.join(" "), if cv.len() > 6 { " ..." } else { "" });
                    }
                }
            }
            println!();
            blk += 1;
        }
    }
}
