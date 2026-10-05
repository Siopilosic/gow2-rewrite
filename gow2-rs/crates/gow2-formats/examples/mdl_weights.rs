//! Histogram of the blend weight field (position w, low 15 bits) of a model's vertices, and of how the two palette slots of a tag relate.
//! `cargo run --release -p gow2-formats --example mdl_weights -- ../extracted/pak/R_HERO01.WAD hero`
use std::collections::BTreeMap;

use gow2_formats::{mdl, wad};

fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().expect("WAD path");
    let model = a.next().expect("model name");
    let data = std::fs::read(&path).unwrap();
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let name = wad::mesh_record_name(&model);
    let blob = recs.iter().find(|r| r.name == name && !r.data.is_empty()).expect("model blob").data;
    let mut hist: BTreeMap<i64, usize> = BTreeMap::new();
    let (mut over, mut total, mut same_slot_weighted, mut diff_slot_zero) = (0, 0, 0, 0);
    let mut high_bits: BTreeMap<i64, usize> = BTreeMap::new();
    for part in mdl::parts(blob) {
        for &(g, s, e) in &part.packets {
            if g != 0 {
                continue;
            }
            for b in mdl::batches(blob, s, e) {
                let (Some(p), Some(h)) = (&b.pos, &b.hdr) else { continue };
                let mut start = 0usize;
                for t in h {
                    let nl = (t[0] & 0x7FFF) as usize;
                    let base = (t[3] & 0x3ff) / 4;
                    let second = (t[3] >> 12) & 0xf;
                    *high_bits.entry((t[3] >> 10) & 3).or_default() += 1;
                    for v in &p[start..(start + nl).min(p.len())] {
                        let w = v[3] & 0x7fff;
                        total += 1;
                        *hist.entry(w / 256).or_default() += 1;
                        if w > 4096 {
                            over += 1;
                        }
                        if base == second && w > 0 {
                            same_slot_weighted += 1;
                        }
                        if base != second && w == 0 {
                            diff_slot_zero += 1;
                        }
                    }
                    start += nl;
                }
            }
        }
    }
    println!("vertices {total}; weight above 4096: {over}; same base/second slot but weight > 0: {same_slot_weighted}; different slots with weight 0: {diff_slot_zero}");
    println!("tag bits 10-11 histogram {high_bits:?}");
    println!("weight histogram in steps of 256 (of 4096): {hist:?}");
}
