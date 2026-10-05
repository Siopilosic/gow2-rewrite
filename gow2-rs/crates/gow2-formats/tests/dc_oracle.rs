//! The Rust DC reader must reproduce the Python one (`tools/dcparse.py`, `tools/oracle_dc.py`) on every move of R_HERO00, and the
//! documented combo data must be in it (`docs/combat.md` section 5, `docs/kratos-data.md` 6.4).

use std::path::PathBuf;

use gow2_formats::{dc, wad};

const M64: u64 = u64::MAX;

fn fnv(h: &mut u64, x: i64) {
    *h = (*h ^ (x as u64 & M64)).wrapping_mul(1099511628211);
}

fn q(x: f32) -> i64 {
    // Python: int(round(x * 10000)), banker's rounding; halves times 1e4 are never exact ties in practice
    ((x as f64) * 10000.0).round_ties_even() as i64
}

fn load() -> Option<(dc::Dc, dc::MoveSet)> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_HERO00.WAD");
    let data = std::fs::read(p).ok()?;
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let d = dc::Dc::from_records(&recs)?;
    let set = d.move_set();
    Some((d, set))
}

fn branch_sum(h: &mut u64, set: &dc::MoveSet, b: &dc::Branch) {
    let t = b.target.map_or(0, |i| dc::hash(&set.moves[i].name, 0) as i64);
    for x in [t, b.flags_a as i64, b.flags_b as i64, q(b.win.0), q(b.win.1), q(b.start_time), b.tgt_health.0 as i64, b.tgt_health.1 as i64,
        b.own_health.0 as i64, b.own_health.1 as i64, b.b1e as i64, b.b1f as i64, b.button as i64, b.press as i64, b.stick as i64,
        b.unlock as i64, b.min_level as i64, b.has_context as i64]
    {
        fnv(h, x);
    }
}

#[test]
fn rust_dc_matches_python() {
    let Some((_d, set)) = load() else { return };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let expected = std::fs::read_to_string(root.join("tests/oracle/dc.tsv")).unwrap();
    let mut checked = 0;
    for line in expected.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        if f[1] == "entry" {
            let mut h: u64 = 1469598103934665603;
            for b in &set.entry {
                branch_sum(&mut h, &set, b);
            }
            assert_eq!(set.entry.len(), f[4].parse::<usize>().unwrap(), "entry branch count");
            assert_eq!(h.to_string(), f[10], "entry branches checksum");
            continue;
        }
        let off = usize::from_str_radix(f[1], 16).unwrap();
        let m = set.moves.iter().find(|m| m.off == off).unwrap_or_else(|| panic!("move at {off:x}"));
        assert_eq!(m.name, f[2]);
        assert_eq!(m.anim, f[3], "{} clip", m.name);
        assert_eq!((m.branches.len(), m.hits.len(), m.actions.len()), (f[4].parse().unwrap(), f[5].parse().unwrap(), f[6].parse().unwrap()), "{} counts", m.name);
        assert_eq!((q(m.rate), q(m.blend)), (f[7].parse().unwrap(), f[8].parse().unwrap()), "{} rate/blend", m.name);
        assert_eq!(format!("{:x}", m.flags), f[9], "{} flags", m.name);
        let mut h: u64 = 1469598103934665603;
        for b in &m.branches {
            branch_sum(&mut h, &set, b);
        }
        for c in &m.hits {
            for x in [q(c.win.0), q(c.win.1), q(c.damage), c.volume as i64, c.flags as i64, c.sub_hits as i64] {
                fnv(&mut h, x);
            }
            for v in [c.ground, c.air, c.block].iter().flatten() {
                fnv(&mut h, q(*v));
            }
        }
        for a in &m.actions {
            for x in [a.kind as i64, a.flags as i64, a.trigger as i64, a.min_level as i64, q(a.win.0), q(a.win.1)] {
                fnv(&mut h, x);
            }
        }
        assert_eq!(h.to_string(), f[10], "{} checksum", m.name);
        checked += 1;
    }
    eprintln!("{checked} moves matched the Python oracle");
    assert!(checked > 1000);
}

#[test]
fn the_blade_attachments_name_the_documented_joints() {
    let Some((d, _)) = load() else { return };
    let a = d.chained_attachments();
    assert_eq!(a.len(), 2);
    // docs/animation.md: left and right blade, hand / free / stowed slots, 0.35 m snap, chain joints
    let names = |x: &dc::Attachment| (x.object.clone(), x.hand.clone(), x.free.clone(), x.stowed.clone(), x.chain.clone());
    assert_eq!(names(&a[0]), ("goMAIBlade".into(), "LWeapIH".into(), "LWeapOH".into(), "LeftBladeBack".into(), ["LChain".into(), "LChainW".into()]));
    assert_eq!(names(&a[1]), ("goMAIBlade".into(), "RWeapIH".into(), "RWeapOH".into(), "RightBladeBack".into(), ["RChain".into(), "RChainW".into()]));
    assert!(a.iter().all(|x| x.chained && (x.snap_m - 0.35).abs() < 1e-6));
}

#[test]
fn the_slam_finisher_spawns_two_blasts_at_the_clip_joints() {
    let Some((_d, set)) = load() else { return };
    let m = &set.moves[set.find("MOV_SlamFinisher").unwrap()];
    assert!(m.hits.is_empty(), "the slam has no hit window of its own");
    let blasts: Vec<&dc::Blast> = m.actions.iter().filter_map(|a| a.blast.as_ref()).collect();
    assert_eq!(blasts.len(), 2);
    let names: Vec<(&str, &str)> = blasts.iter().map(|b| (b.name.as_str(), b.joint.as_str())).collect();
    assert_eq!(names, [("CNC_SLAM_CLOSE", "zeroJoint"), ("CNC_SLAM_FAR", "synchJoint")]);
    for b in &blasts {
        assert_eq!((b.shape, b.hit.damage, b.hit.volume), (1, 6.0, 8));
        assert!((b.duration - 0.1).abs() < 1e-3, "lasts {}", b.duration);
    }
    // the float list at +0x28: (2.5, 2.0) for the close blast, (3.5, 2.0) for the far one
    assert_eq!(blasts[0].keys, [2.5, 2.0]);
    assert_eq!(blasts[1].keys, [3.5, 2.0]);
    // 24 concussion records exist in the hero data; each action that points at one is decoded
    let n: usize = set.moves.iter().flat_map(|m| &m.actions).filter(|a| a.blast.is_some()).count();
    assert!(n >= 24, "{n} blast actions");
}

#[test]
fn the_documented_square_combo_is_in_the_data() {
    let Some((_d, set)) = load() else { return };
    let s1 = set.find("MOV_BasicSquare01").expect("MOV_BasicSquare01");
    let m = &set.moves[s1];
    assert_eq!(m.anim, "attComboSlash01");
    // docs/combat.md section 5: window 0.135 to 0.245, damage 2, ground impulse 800, volume 2
    let h = &m.hits[0];
    assert!((h.win.0 - 0.135).abs() < 0.001 && (h.win.1 - 0.245).abs() < 0.001, "{:?}", h.win);
    assert_eq!((h.damage, h.volume, h.ground[0]), (2.0, 2, 800.0));
    // button 2 pressed in [0, 0.275] goes to MOV_BasicSquare02 (docs/kratos-data.md 6.4)
    let to2 = m.branches.iter().find(|b| b.button == 2 && b.press == 1 && b.win.1 < 0.28 && b.target.is_some()).expect("square branch");
    assert_eq!(set.moves[to2.target.unwrap()].name, "MOV_BasicSquare02");
    // the entry list holds the Square attack for the ground
    let e = set.entry.iter().find(|b| b.name == "BRA_SquareAttack").expect("BRA_SquareAttack");
    assert_eq!(set.moves[e.target.unwrap()].name, "MOV_BasicSquare01");
    assert_eq!((e.button, e.press, e.flags_a & 0x1ff), (2, 1, 1));
}


