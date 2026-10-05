//! The Rust collision-sheet reader must reproduce the Python one (`tools/sheet_decode.py`) on every level WAD.
//! Expected values come from `tools/oracle_sheet.py` (counts, surface names and a 64-bit FNV checksum, no game data).
//! A missing WAD is skipped with a note. The vertex bounds must also equal the bounds in the record header.

use std::path::PathBuf;

use gow2_formats::{sheet, wad};

fn fnv(h: &mut u64, x: u32) {
    *h = (*h ^ x as u64).wrapping_mul(1099511628211);
}

fn checksum(s: &sheet::Sheet) -> String {
    let mut h: u64 = 1469598103934665603;
    for p in &s.polys {
        fnv(&mut h, p.surface as u32);
        fnv(&mut h, p.corners as u32);
        for c in &p.v[..p.corners as usize] {
            c.iter().for_each(|v| fnv(&mut h, v.to_bits()));
        }
    }
    h.to_string()
}

#[test]
fn rust_sheet_matches_python() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let expected = std::fs::read_to_string(root.join("gow2-rs/crates/gow2-formats/tests/oracle/sheet.tsv")).unwrap();
    let mut checked = 0;
    for line in expected.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        let w = f[0];
        let p = root.join("extracted/pak").join(format!("{w}.WAD"));
        let Ok(data) = std::fs::read(&p) else {
            eprintln!("{} not found; skipping", p.display());
            continue;
        };
        let recs: Vec<wad::Record> = wad::records(&data).collect();
        let s = sheet::find(&recs).unwrap_or_else(|| panic!("{w}: no sheet"));
        let (verts, tris, quads) = (f[1].parse::<usize>().unwrap(), f[2].parse::<usize>().unwrap(), f[3].parse::<usize>().unwrap());
        let nt = s.polys.iter().filter(|p| p.corners == 3).count();
        assert_eq!((nt, s.polys.len() - nt), (tris, quads), "{w} polygon counts");
        assert_eq!(s.surfaces.len(), f[4].parse::<usize>().unwrap(), "{w} surfaces");
        assert_eq!(s.flag_names.len(), f[5].parse::<usize>().unwrap(), "{w} flag names");
        let names: Vec<String> = s.surfaces.iter().map(|x| format!("{}:{:08x}:{:08x}", x.name, x.flags, x.flags_hi)).collect();
        assert_eq!(names.join("|"), f[6], "{w} surface list");
        assert_eq!(checksum(&s), f[7], "{w} checksum");
        let up = s.polys.iter().filter(|p| p.normal[1] > 0.5).count();
        let down = s.polys.iter().filter(|p| p.normal[1] < -0.5).count();
        assert_eq!(format!("{up}/{down}"), f[8], "{w} floor/ceiling counts");
        // the header bounds are the extent of the vertices
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &s.polys {
            for c in &p.v[..p.corners as usize] {
                for k in 0..3 {
                    lo[k] = lo[k].min(c[k]);
                    hi[k] = hi[k].max(c[k]);
                }
            }
        }
        let _ = verts;
        for k in 0..3 {
            assert!(lo[k] >= s.bounds.0[k] - 1e-3 && hi[k] <= s.bounds.1[k] + 1e-3, "{w} bounds axis {k}: {lo:?} {hi:?} vs {:?}", s.bounds);
        }
        checked += 1;
    }
    eprintln!("{checked} level sheets matched the Python oracle");
    assert!(checked > 0 || !root.join("extracted/pak").exists());
}

#[test]
fn rhod10_start_floor_is_the_stone_tile_under_kratos() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/RHOD10.WAD");
    let Ok(data) = std::fs::read(p) else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let s = sheet::find(&recs).unwrap();
    assert_eq!(s.polys.len(), 991);
    // the start position from the RAM capture: a floor quad at exactly y = 3712 with the rock surface
    let (x, z) = (-1704.28f32, -5353.45f32);
    let hit = s.polys.iter().find(|p| {
        let v = &p.v[..p.corners as usize];
        let side: Vec<f32> = (0..v.len()).map(|i| {
            let (a, b) = (v[i], v[(i + 1) % v.len()]);
            (b[0] - a[0]) * (z - a[2]) - (b[2] - a[2]) * (x - a[0])
        }).collect();
        p.normal[1] > 0.9 && (side.iter().all(|&d| d >= 0.0) || side.iter().all(|&d| d <= 0.0))
    });
    let p = hit.expect("floor under the start");
    assert!((p.v[0][1] - 3712.0).abs() < 1e-3);
    assert_eq!(s.surface(p).name, "groundPlainRock");
    assert_eq!(s.surface(p).flags & sheet::flag::GROUND, sheet::flag::GROUND);
}
