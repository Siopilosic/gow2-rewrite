//! The Rust ANM decoder must reproduce the Python decoder (`tools/anm_decode.py`) exactly.
//! Expected values come from `tools/oracle_anm.py`: per ANM_ record the clip, segment and failure counts and
//! a 64-bit FNV checksum over every (slot, frame, value bits). No game data is stored.

use std::path::PathBuf;

use gow2_formats::{anm, wad};

fn mix(h: &mut u64, x: u64) {
    *h = (*h ^ x).wrapping_mul(1099511628211);
}

#[test]
fn rust_anm_matches_python() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let expected = std::fs::read_to_string(root.join("gow2-rs/crates/gow2-formats/tests/oracle/anm.tsv")).unwrap();
    let mut checked = 0;
    let mut cache: Option<(String, Vec<u8>)> = None;
    for line in expected.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        let (w, name) = (f[0], f[1]);
        let want: Vec<usize> = f[2..5].iter().map(|s| s.parse().unwrap()).collect();
        if cache.as_ref().map(|c| c.0.as_str()) != Some(w) {
            let p = root.join("extracted/pak").join(format!("{w}.WAD"));
            let Ok(data) = std::fs::read(&p) else {
                eprintln!("{} not found; skipping", p.display());
                continue;
            };
            cache = Some((w.to_string(), data));
        }
        let data = &cache.as_ref().unwrap().1;
        let rec = wad::records(data)
            .find(|r| r.tag == wad::Tag::Object && r.name == name && r.data.len() > 0x40)
            .expect("record present");
        let b = rec.data;
        let kinds = anm::block_kinds(b).expect("tracks");
        let (mut nclips, mut nsegs, mut nfail) = (0, 0, 0);
        let mut h: u64 = 1469598103934665603;
        for c in anm::clips(b).into_iter().take(40) {
            nclips += 1;
            // the oracle counts segments from the block headers; a clip with a bad header is skipped whole
            let Some(blocks) = anm::decode_clip(b, c, &kinds) else { continue };
            for blk in blocks {
                for seg in blk.segments {
                    nsegs += 1;
                    let Some(seg) = seg else {
                        nfail += 1;
                        continue;
                    };
                    for (slot, frames) in &seg.curves {
                        for (frame, v) in frames {
                            mix(&mut h, *slot as u64);
                            mix(&mut h, *frame as u64);
                            mix(&mut h, v.to_bits());
                        }
                    }
                }
            }
        }
        assert_eq!(vec![nclips, nsegs, nfail], want, "{w} {name} counts");
        assert_eq!(h.to_string(), f[5], "{w} {name} checksum");
        checked += 1;
    }
    eprintln!("{checked} ANM records matched the Python oracle");
}
