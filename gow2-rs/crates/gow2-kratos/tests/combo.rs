//! The move graph on the real hero data: a Square tap starts the first slash, taps inside the windows chain the combo, and the
//! stance and the end of the move behave as `docs/kratos-data.md` 6.4 describes. Skipped when the WAD is absent.

use std::path::PathBuf;
use std::sync::Arc;

use gow2_formats::{dc, wad};
use gow2_kratos::moves::{pad, Env, Input, MoveSys, Pad};

fn load() -> Option<Arc<dc::MoveSet>> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/R_HERO00.WAD");
    let data = std::fs::read(p).ok()?;
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    Some(Arc::new(dc::Dc::from_records(&recs)?.move_set()))
}

const DT: f32 = 1.0 / 60.0;

/// Every clip is one second long here; the real lengths come from the animation data.
fn one_second(_: &str) -> Option<f32> {
    Some(1.0)
}

struct Rig {
    sys: MoveSys,
    pad: Pad,
    env: Env,
}

impl Rig {
    fn new(set: Arc<dc::MoveSet>) -> Rig {
        Rig { sys: MoveSys::new(set), pad: Pad::default(), env: Env::default() }
    }
    /// One tick with the given held buttons.
    fn tick(&mut self, buttons: u32) {
        self.pad = Pad { cur: buttons, prev: self.pad.cur };
        self.sys.update(DT, &Input { pad: self.pad, ..Default::default() }, &self.env, &one_second);
    }
    fn name(&self) -> String {
        self.sys.current().map_or("-".into(), |(m, _)| m.name.clone())
    }
    fn run(&mut self, secs: f32) {
        for _ in 0..(secs / DT).round() as usize {
            self.tick(0);
        }
    }
}

#[test]
fn a_square_tap_from_standing_starts_the_first_slash() {
    let Some(set) = load() else { return };
    let mut r = Rig::new(set);
    r.tick(0);
    assert_eq!(r.name(), "-", "standing runs no move");
    r.tick(pad::SQUARE);
    assert_eq!(r.name(), "MOV_BasicSquare01");
    // the clip is the documented one
    assert_eq!(r.sys.current().unwrap().0.anim, "attComboSlash01");
}

#[test]
fn taps_inside_the_windows_chain_the_combo_in_order() {
    let Some(set) = load() else { return };
    let mut r = Rig::new(set);
    r.tick(pad::SQUARE);
    let mut seen = vec![r.name()];
    // tap Square every 0.3 s of game time for a few seconds
    for i in 0..60 * 4 {
        r.tick(if i % 18 == 0 { pad::SQUARE } else { 0 });
        let n = r.name();
        if seen.last() != Some(&n) {
            seen.push(n);
        }
    }
    assert_eq!(&seen[..3], ["MOV_BasicSquare01", "MOV_BasicSquare02", "MOV_BasicSquare03"], "{seen:?}");
}

#[test]
fn an_early_tap_waits_for_the_end_of_its_window() {
    let Some(set) = load() else { return };
    let mut r = Rig::new(set);
    r.tick(pad::SQUARE);
    r.run(0.05);
    r.tick(pad::SQUARE); // buffered: window 0 to 0.2749, not immediate
    assert_eq!(r.name(), "MOV_BasicSquare01", "does not cut the first slash short");
    r.run(0.15);
    assert_eq!(r.name(), "MOV_BasicSquare01");
    r.run(0.12); // t is now past 0.2749
    assert_eq!(r.name(), "MOV_BasicSquare02");
}

#[test]
fn a_late_tap_is_immediate() {
    let Some(set) = load() else { return };
    let mut r = Rig::new(set);
    r.tick(pad::SQUARE);
    r.run(0.45);
    assert_eq!(r.name(), "MOV_BasicSquare01");
    r.tick(pad::SQUARE); // window 0.2749 to 1 has flag 0x400: at once
    assert_eq!(r.name(), "MOV_BasicSquare02");
}

#[test]
fn without_input_the_combo_ends_in_the_combat_stance_and_a_new_tap_starts_over() {
    let Some(set) = load() else { return };
    let mut r = Rig::new(set);
    r.tick(pad::SQUARE);
    r.run(1.05);
    assert_eq!(r.name(), "MOV_CombatIdle", "the 0x0b end branch leads to the stance");
    r.run(0.5);
    assert_eq!(r.name(), "MOV_CombatIdle");
    // an attack button ends the stance and the entry branch starts the slash in the same tick
    r.tick(pad::SQUARE);
    assert_eq!(r.name(), "MOV_BasicSquare01");
}

#[test]
fn the_stance_ends_when_walking_or_after_the_time_limit() {
    let Some(set) = load() else { return };
    let mut r = Rig::new(set.clone());
    r.tick(pad::SQUARE);
    r.run(1.05);
    assert!(r.sys.in_stance());
    assert!(r.sys.leave_stance_when(true, 4.0), "walking leaves the stance");
    assert!(!r.sys.is_busy());
    // standing still: stays until the limit
    let mut r = Rig::new(set);
    r.tick(pad::SQUARE);
    r.run(1.05);
    r.run(2.0);
    assert!(!r.sys.leave_stance_when(false, 4.0));
    r.run(2.5);
    assert!(r.sys.leave_stance_when(false, 4.0), "the limit ends it");
}

#[test]
fn triangle_starts_the_slam_and_the_block_button_the_parry() {
    let Some(set) = load() else { return };
    let mut r = Rig::new(set.clone());
    r.tick(pad::TRIANGLE);
    assert_eq!(r.name(), "MOV_BasicTriangle01");
    let mut r = Rig::new(set);
    r.tick(pad::BLOCK);
    assert_eq!(r.name(), "MOV_Parry");
}

#[test]
fn the_air_has_its_own_attacks() {
    let Some(set) = load() else { return };
    let mut r = Rig::new(set);
    r.env.state_mask = 2;
    r.tick(pad::SQUARE);
    assert_eq!(r.name(), "MOV_AirSquare01");
}

#[test]
fn the_first_slash_hits_a_dummy_once_for_two_damage_and_knocks_it_back() {
    use gow2_kratos::combat::{effects, resolve, Attacker, Effect, Meters, Victim};
    let Some(set) = load() else { return };
    let mut r = Rig::new(set);
    let a = Attacker::new([0.0; 3], [0.0, -1.0]);
    let mut victims = vec![Victim::new(1, [0.0, 0.0, -40.0], 100.0, 100.0)];
    let mut meters = Meters::default();
    let (mut hits, mut pause) = (0, 0.0f32);
    r.tick(pad::SQUARE);
    for _ in 0..90 {
        r.pad = Pad { cur: 0, prev: r.pad.cur };
        let tick = r.sys.update(DT, &Input { pad: r.pad, ..Default::default() }, &r.env, &one_second);
        for h in resolve(&mut r.sys, &a, &mut victims, 1.0) {
            hits += 1;
            assert_eq!(h.damage, 2.0);
            assert!((h.knock[2] + 128.0).abs() < 1.0, "{:?}", h.knock);
        }
        for e in effects(&r.sys, &tick.fired) {
            match e {
                Effect::Meter { selector, amount, relative } => meters.adjust(selector, amount, relative),
                Effect::HitPause(s) => pause = pause.max(s),
                _ => {}
            }
        }
    }
    assert_eq!(hits, 1, "one window, one victim, one hit");
    assert_eq!(victims[0].health, 98.0);
    // the on-hit actions ran on the frame after the hit: the god meter rose and a hit pause was requested
    assert!(meters.god > 0.0, "god meter {}", meters.god);
    assert!(pause > 0.0, "hit pause {pause}");
}

#[test]
fn a_hit_window_opens_inside_the_documented_span_and_deals_once() {
    let Some(set) = load() else { return };
    let mut r = Rig::new(set);
    r.tick(pad::SQUARE);
    assert!(r.sys.open_windows().is_empty());
    r.run(0.15); // 0.135 to 0.245 is the window
    let w = r.sys.open_windows();
    assert_eq!(w.len(), 1, "{w:?}");
    let (k, sub, win) = (w[0].0, w[0].1, w[0].2.clone());
    assert_eq!((win.damage, win.volume, sub), (2.0, 2, 1));
    assert!(!r.sys.already_hit(7, k, sub));
    r.sys.register_hit(7, k, sub, false);
    assert!(r.sys.already_hit(7, k, sub), "the same victim is not hit twice by one window");
    assert!(!r.sys.already_hit(8, k, sub));
}


