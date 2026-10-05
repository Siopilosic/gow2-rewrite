//! Hit resolution: what happens when an open hit window of a move touches a victim (`docs/combat.md`).
//!
//! The move system says which windows are open (`MoveSys::open_windows`). Here each open window is tested against the victims, a hit
//! is dealt once per (victim, window, sub-hit), damage is `window damage x multiplier` (section 4), the victim is knocked back by the
//! ground or air impulse (section 3: the three halves are a vector in the attacker's frame, divided by the victim's mass and
//! multiplied by 16, then replace the victim's velocity), and the actions the move attaches to a hit run on the next frame.
//!
//! Attack volumes are the game's collision balls (`gow2_formats::cdv`): a hit window names a volume id, the attacker carries balls with
//! those ids (the two blades 2 and 3, fists 4 and 5, feet 6 and 7, body 1, throw 9) at their joints, and a hit needs a matching ball to
//! touch the victim. A ball moves fast, so the path it travelled since the last tick is tested too. When no balls are given (tests, a
//! front end without the data) a reach in front of the attacker stands in (LOW).

use gow2_formats::dc::{Action, Blast, HitWindow};

use crate::moves::{Fired, MoveSys};
use crate::world::World;
use crate::{friction, Tuning, UNITS_PER_METRE};

/// Kratos's damage multiplier with the default blades at stage 5 on the captured difficulty (`docs/combat.md` 4.1: 1.0 x 1.0 x 1.0).
pub const DAMAGE_MULTIPLIER: f32 = 1.0;

/// Kratos's mass: `tuning + 0x1c` (`docs/combat.md` section 4.0).
pub const HERO_MASS: f32 = 100.0;

/// Something that can be hit.
#[derive(Debug, Clone, PartialEq)]
pub struct Victim {
    pub id: u32,
    pub pos: [f32; 3],
    /// Horizontal velocity (x, z) in units per second.
    pub vel: [f32; 2],
    pub vy: f32,
    pub airborne: bool,
    pub health: f32,
    pub max_health: f32,
    pub mass: f32,
    /// Body size for the reach test, in units.
    pub radius: f32,
    pub height: f32,
    /// Seconds since the last hit (for a hurt flash).
    pub since_hit: f32,
    /// Turned to stone by the Medusa magic (0 = not): it stands still and shatters on the next hit.
    pub petrify: f32,
    /// Seconds spent in the Medusa beam (it turns to stone after a short while).
    pub beam_time: f32,
}

impl Victim {
    pub fn new(id: u32, pos: [f32; 3], health: f32, mass: f32) -> Self {
        Victim { id, pos, vel: [0.0; 2], vy: 0.0, airborne: false, health, max_health: health, mass, radius: 12.0, height: 34.0, since_hit: 9.0, petrify: 0.0, beam_time: 0.0 }
    }

    pub fn alive(&self) -> bool {
        self.health > 0.0
    }

    /// Ground friction and gravity, standing on the world's floor.
    pub fn tick(&mut self, dt: f32, world: &dyn World, t: &Tuning) {
        self.since_hit += dt;
        if self.airborne {
            self.vy -= 50.0 * UNITS_PER_METRE * dt;
        } else {
            self.vel = friction(t.friction, dt, self.vel);
        }
        self.pos[0] += self.vel[0] * dt;
        self.pos[2] += self.vel[1] * dt;
        self.pos[1] += self.vy * dt;
        match world.floor(self.pos[0], self.pos[2], self.pos[1] + 12.0) {
            Some(fy) if self.pos[1] <= fy + 0.01 && self.vy <= 0.0 => {
                self.pos[1] = fy;
                self.vy = 0.0;
                self.airborne = false;
            }
            Some(_) | None => {
                if !self.airborne && world.floor(self.pos[0], self.pos[2], self.pos[1] + 12.0).is_none() {
                    self.airborne = true;
                }
            }
        }
    }
}

/// A collision ball in the world: its volume id, centre now and one tick ago, and radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBall {
    pub id: u32,
    pub centre: [f32; 3],
    pub prev: [f32; 3],
    pub radius: f32,
}

/// Who attacks: position, facing (x, z unit vector) and the collision balls.
#[derive(Debug, Clone, PartialEq)]
pub struct Attacker {
    pub pos: [f32; 3],
    pub facing: [f32; 2],
    pub balls: Vec<WorldBall>,
}

impl Attacker {
    pub fn new(pos: [f32; 3], facing: [f32; 2]) -> Self {
        Attacker { pos, facing, balls: Vec::new() }
    }
}

/// Whether a hit window with this volume selector accepts a ball with this id (`FUN_00247820`): `0x00` to `0x1f` must match, `0x20`
/// accepts the two blades (2, 3), `0x21` the arms (4, 5), `0x22` the legs (6, 7); `0x23` to `0x25` are the same pairs for the second
/// query mode and `0x26` accepts volume 1 there (`docs/combat.md` section 1).
pub fn volume_matches(selector: u8, id: u32) -> bool {
    match selector {
        0..=0x1f => id == selector as u32,
        0x20 | 0x23 => id == 2 || id == 3,
        0x21 | 0x24 => id == 4 || id == 5,
        0x22 | 0x25 => id == 6 || id == 7,
        0x26 => id == 1,
        _ => false,
    }
}

/// Distance from a point to the victim's axis (a vertical segment from its feet to the top of its head).
fn axis_distance(v: &Victim, p: [f32; 3]) -> f32 {
    let y = p[1].clamp(v.pos[1], v.pos[1] + v.height);
    ((p[0] - v.pos[0]).powi(2) + (p[1] - y).powi(2) + (p[2] - v.pos[2]).powi(2)).sqrt()
}

/// Does a ball, swept from where it was to where it is, reach the victim?
fn ball_touches(b: &WorldBall, v: &Victim) -> bool {
    (0..=4).any(|i| {
        let t = i as f32 / 4.0;
        let p = [0, 1, 2].map(|k| b.prev[k] + (b.centre[k] - b.prev[k]) * t);
        axis_distance(v, p) <= b.radius + v.radius
    })
}

/// The reach used for an attack volume id (`docs/combat.md` 2.1): blades 2, 3 and the pair selector 0x20, arms 4 and 5, legs 6 and
/// 7, the body 1. `(forward reach, half-angle cosine, vertical half range)` in units. LOW.
fn volume_reach(volume: u8) -> (f32, f32, f32) {
    match volume {
        2 | 3 | 0x20 | 0x23 | 0x26 | 0x0b => (3.6 * UNITS_PER_METRE, 0.2, 2.6 * UNITS_PER_METRE),
        4 | 5 | 0x21 | 0x24 => (1.8 * UNITS_PER_METRE, 0.5, 2.0 * UNITS_PER_METRE),
        6 | 7 | 0x22 | 0x25 => (2.0 * UNITS_PER_METRE, 0.5, 1.6 * UNITS_PER_METRE),
        1 => (1.6 * UNITS_PER_METRE, 0.3, 2.0 * UNITS_PER_METRE),
        _ => (2.4 * UNITS_PER_METRE, 0.3, 2.0 * UNITS_PER_METRE),
    }
}

/// Does the attack volume touch the victim?
pub fn touches(a: &Attacker, volume: u8, v: &Victim) -> bool {
    if !a.balls.is_empty() {
        return a.balls.iter().any(|b| volume_matches(volume, b.id) && ball_touches(b, v));
    }
    let (reach, cos_min, vert) = volume_reach(volume);
    let (dx, dz) = (v.pos[0] - a.pos[0], v.pos[2] - a.pos[2]);
    let d = (dx * dx + dz * dz).sqrt();
    if d > reach + v.radius {
        return false;
    }
    if d > v.radius {
        let c = (dx * a.facing[0] + dz * a.facing[1]) / d;
        // closer than the victim's own radius counts from any side; otherwise it must be in front
        if c < cos_min {
            return false;
        }
    }
    let (y0, y1) = (a.pos[1] - 8.0, a.pos[1] + vert);
    v.pos[1] + v.height >= y0 && v.pos[1] <= y1
}

/// What one hit did.
#[derive(Debug, Clone, PartialEq)]
pub struct HitReport {
    pub victim: u32,
    pub damage: f32,
    pub lethal: bool,
    /// The knock-back velocity given to the victim, world (x, y, z) in units per second.
    pub knock: [f32; 3],
    /// Window flags (`docs/combat.md` section 1) for the effect and for blocking rules.
    pub flags: u8,
    pub window: String,
}

/// The impulse of a hit window as a world velocity: the three halves are (forward, up, right) in the attacker's frame
/// (MEDIUM, `docs/combat.md` section 3), divided by the victim's mass and multiplied by 16.
pub fn knock_velocity(w: &HitWindow, a: &Attacker, v: &Victim) -> [f32; 3] {
    let imp = if v.airborne { w.air } else { w.ground };
    let k = 16.0 / v.mass.max(1.0);
    let (f, r) = (a.facing, [-a.facing[1], a.facing[0]]);
    [(imp[0] * f[0] + imp[2] * r[0]) * k, imp[1] * k, (imp[0] * f[1] + imp[2] * r[1]) * k]
}

/// Tests the open windows of the attacker's current move against the victims and deals the hits that have not been dealt.
///
/// A window flagged "lock-on target only" (`0x01`) hits any victim here, since this port has no lock-on yet.
pub fn resolve(sys: &mut MoveSys, a: &Attacker, victims: &mut [Victim], multiplier: f32) -> Vec<HitReport> {
    let windows: Vec<(usize, u32, HitWindow)> = sys.open_windows().into_iter().map(|(k, s, w)| (k, s, w.clone())).collect();
    let mut out = Vec::new();
    for (k, sub, w) in windows {
        for v in victims.iter_mut() {
            if !v.alive() || sys.already_hit(v.id, k, sub) || !touches(a, w.volume, v) {
                continue;
            }
            // stone shatters at the next hit
            let damage = if v.petrify > 0.0 { v.health } else { w.damage * multiplier };
            let lethal = v.health > 0.0 && damage >= v.health;
            v.health = (v.health - damage).max(0.0);
            let knock = knock_velocity(&w, a, v);
            v.vel = [knock[0], knock[2]];
            if knock[1] > 0.0 {
                v.vy = knock[1];
                v.airborne = true;
            }
            v.since_hit = 0.0;
            sys.register_hit(v.id, k, sub, lethal);
            out.push(HitReport { victim: v.id, damage, lethal, knock, flags: w.flags, window: w.name.clone() });
        }
    }
    out
}

/// The effect of an action, for the actions this port runs. Kinds and fields: `docs/combat.md` section 8 and `docs/kratos-data.md` 6.5.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// `tActionMeterAdjust`: meter 0 health, 1 the magic meter, 2 the god meter; amount may be relative.
    Meter { selector: u8, amount: f32, relative: bool },
    /// `tActionHitPause`: freeze attacker and victim for this long (seconds).
    HitPause(f32),
    /// `tActionSlowdown`: time scale for a duration (seconds).
    Slowdown { scale: f32, secs: f32 },
    /// `tActionCameraShake` / `tActionForceFeedback`: kept so a front end can react; the preset is an import.
    CameraShake,
    Rumble,
    /// `tActionSound`: the sound name hash.
    Sound(u32),
    /// `tActionConcussion`: a damage sphere placed at one of the clip's joints (`ActiveBlast`).
    Blast(Blast),
}

pub fn decode_action(a: &Action) -> Option<Effect> {
    if let Some(b) = &a.blast {
        return Some(Effect::Blast(b.clone()));
    }
    Some(match a.kind {
        0x02 => Effect::Meter { selector: a.raw[0x0c], amount: a.f32_at(8), relative: a.raw[0x0d] & 4 != 0 },
        0x15 => Effect::HitPause(a.half_at(8)),
        0x0c => Effect::Slowdown { scale: a.half_at(8), secs: a.half_at(10) },
        0x0a => Effect::CameraShake,
        0x0b => Effect::Rumble,
        // `tActionSound`, `tActionSoundOnEnemy` (trigger 1, on hit) and `tActionSoundWindow` all carry the sound name hash at +8 (`docs/audio.md` 1)
        0x07..=0x09 => Effect::Sound(a.u32_at(8)),
        _ => return None,
    })
}

/// The effects of the actions that fired.
pub fn effects(sys: &MoveSys, fired: &[Fired]) -> Vec<Effect> {
    fired.iter().filter_map(|f| decode_action(&sys.set.moves[f.mv].actions[f.action])).collect()
}

/// A concussion in flight (`docs/combat.md` section 13): a sphere placed once at a joint of the clip, whose radius grows from 0 to the
/// record's end value (metres x 16) over the blast's duration (shape kind 1; the other kinds are not ported). Each victim inside it is
/// hit once with the record's hit window.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveBlast {
    pub centre: [f32; 3],
    /// The attacker's facing when it was spawned, the frame the impulse is given in.
    pub facing: [f32; 2],
    pub age: f32,
    pub duration: f32,
    /// Radius at the end, in units.
    pub radius_end: f32,
    pub hit: HitWindow,
    pub multiplier: f32,
    done: Vec<u32>,
}

impl ActiveBlast {
    pub fn new(b: &Blast, centre: [f32; 3], facing: [f32; 2], multiplier: f32) -> Self {
        // the last key is the end value in metres
        let end = b.keys.last().copied().unwrap_or(2.0) * UNITS_PER_METRE;
        ActiveBlast { centre, facing, age: 0.0, duration: b.duration.max(1e-3), radius_end: end, hit: b.hit.clone(), multiplier, done: Vec::new() }
    }

    pub fn radius(&self) -> f32 {
        (self.age / self.duration).clamp(0.0, 1.0) * self.radius_end
    }

    pub fn finished(&self) -> bool {
        self.age >= self.duration
    }

    /// Grows the sphere and hits the victims it now reaches.
    pub fn tick(&mut self, dt: f32, victims: &mut [Victim]) -> Vec<HitReport> {
        self.age += dt;
        let r = self.radius();
        let a = Attacker::new(self.centre, self.facing);
        let mut out = Vec::new();
        for v in victims.iter_mut() {
            if !v.alive() || self.done.contains(&v.id) || axis_distance(v, self.centre) > r + v.radius {
                continue;
            }
            let damage = self.hit.damage * self.multiplier;
            let lethal = damage >= v.health;
            v.health = (v.health - damage).max(0.0);
            let knock = knock_velocity(&self.hit, &a, v);
            v.vel = [knock[0], knock[2]];
            if knock[1] > 0.0 {
                v.vy = knock[1];
                v.airborne = true;
            }
            v.since_hit = 0.0;
            self.done.push(v.id);
            out.push(HitReport { victim: v.id, damage, lethal, knock, flags: self.hit.flags, window: self.hit.name.clone() });
        }
        out
    }
}

/// The three meters (`docs/combat.md` section 6): health `+0x178`, the magic meter `0x335840` and the god meter `0x335848`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meters {
    pub health: f32,
    pub magic: f32,
    pub god: f32,
    pub health_max: f32,
    pub magic_max: f32,
    pub god_max: f32,
}

impl Default for Meters {
    fn default() -> Self {
        Meters { health: 200.0, magic: 200.0, god: 0.0, health_max: 200.0, magic_max: 200.0, god_max: 100.0 }
    }
}

impl Meters {
    pub fn adjust(&mut self, selector: u8, amount: f32, relative: bool) {
        let (v, max) = match selector {
            0 => (&mut self.health, self.health_max),
            1 => (&mut self.magic, self.magic_max),
            2 => (&mut self.god, self.god_max),
            _ => return,
        };
        let _ = relative; // the flag's meaning (set to an absolute value or add) is not confirmed; amounts here are added
        *v = (*v + amount).clamp(0.0, max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atk() -> Attacker {
        Attacker::new([0.0, 0.0, 0.0], [0.0, -1.0])
    }

    #[test]
    fn a_victim_in_front_is_touched_and_one_behind_is_not() {
        let front = Victim::new(1, [0.0, 0.0, -40.0], 100.0, 100.0);
        let behind = Victim::new(2, [0.0, 0.0, 40.0], 100.0, 100.0);
        assert!(touches(&atk(), 2, &front));
        assert!(!touches(&atk(), 2, &behind));
        let far = Victim::new(3, [0.0, 0.0, -300.0], 100.0, 100.0);
        assert!(!touches(&atk(), 2, &far));
    }

    #[test]
    fn the_knock_back_follows_the_documented_scale() {
        // docs/combat.md section 5: MOV_BasicSquare01 ground impulse 800 forward; 800 * 16 / mass 100 = 128 units/s
        let w = HitWindow { name: "w".into(), win: (0.0, 1.0), ground: [800.0, 0.0, 0.0], air: [200.0, 1000.0, 0.0], block: [0.0; 3], damage: 2.0, volume: 2, flags: 0, sub_hits: 1 };
        let v = Victim::new(1, [0.0, 0.0, -40.0], 100.0, 100.0);
        let k = knock_velocity(&w, &atk(), &v);
        assert!((k[0]).abs() < 1e-3 && (k[2] + 128.0).abs() < 1e-3 && k[1] == 0.0, "{k:?}");
        let mut air = v.clone();
        air.airborne = true;
        let k = knock_velocity(&w, &atk(), &air);
        assert!((k[1] - 160.0).abs() < 1e-3, "air lift {k:?}");
    }

    fn ball(id: u32, x: f32) -> WorldBall {
        WorldBall { id, centre: [x, 10.0, -40.0], prev: [x, 10.0, -40.0], radius: 13.2 }
    }

    #[test]
    fn selectors_pick_the_balls_the_data_says() {
        assert!(volume_matches(2, 2) && !volume_matches(2, 3));
        assert!(volume_matches(0x20, 2) && volume_matches(0x20, 3) && !volume_matches(0x20, 4));
        assert!(volume_matches(0x21, 4) && volume_matches(0x21, 5));
        assert!(volume_matches(0x22, 6) && volume_matches(0x22, 7));
        assert!(volume_matches(0x26, 1) && !volume_matches(0x26, 2));
    }

    #[test]
    fn a_ball_must_match_the_window_and_touch_the_victim() {
        let v = Victim::new(1, [0.0, 0.0, -40.0], 100.0, 100.0);
        let mut a = atk();
        a.balls = vec![ball(2, 0.0)];
        assert!(touches(&a, 2, &v), "the left blade ball on the victim");
        assert!(!touches(&a, 3, &v), "a window for the right blade ignores the left ball");
        assert!(touches(&a, 0x20, &v), "the pair selector takes either blade");
        a.balls = vec![ball(2, 60.0)];
        assert!(!touches(&a, 2, &v), "a ball 60 units to the side is out of reach");
    }

    #[test]
    fn a_fast_ball_hits_what_it_passed_through() {
        let v = Victim::new(1, [0.0, 0.0, -40.0], 100.0, 100.0);
        let mut a = atk();
        // it was on one side last tick and is on the other now: neither end touches, the path does
        a.balls = vec![WorldBall { id: 2, centre: [90.0, 10.0, -40.0], prev: [-90.0, 10.0, -40.0], radius: 13.2 }];
        assert!(touches(&a, 2, &v));
        a.balls = vec![WorldBall { id: 2, centre: [90.0, 10.0, -140.0], prev: [-90.0, 10.0, -140.0], radius: 13.2 }];
        assert!(!touches(&a, 2, &v), "a path 100 units away misses");
    }

    #[test]
    fn a_blast_grows_and_hits_each_victim_once() {
        let b = Blast {
            name: "CNC_T".into(),
            shape: 1,
            joint: "zeroJoint".into(),
            hit: HitWindow { name: "w".into(), win: (0.0, 1.0), ground: [250.0, -3000.0, 0.0], air: [250.0, -3000.0, 0.0], block: [0.0; 3], damage: 6.0, volume: 8, flags: 2, sub_hits: 1 },
            duration: 0.1,
            keys: vec![2.5, 2.0],
        };
        // centred 24 units ahead of Kratos; one victim 40 units from the centre, one 200 away
        let mut blast = ActiveBlast::new(&b, [0.0, 0.0, -24.0], [0.0, -1.0], 1.0);
        assert_eq!(blast.radius_end, 32.0);
        let mut vs = vec![Victim::new(1, [0.0, 0.0, -64.0], 100.0, 100.0), Victim::new(2, [0.0, 0.0, -224.0], 100.0, 100.0)];
        let mut hits = 0;
        for _ in 0..12 {
            hits += blast.tick(1.0 / 60.0, &mut vs).len();
        }
        assert!(blast.finished());
        assert_eq!(hits, 1, "the near victim once, the far one never");
        assert_eq!((vs[0].health, vs[1].health), (94.0, 100.0));
        // it did not reach at the very start
        let mut fresh = ActiveBlast::new(&b, [0.0, 0.0, -24.0], [0.0, -1.0], 1.0);
        let mut one = vec![Victim::new(1, [0.0, 0.0, -64.0], 100.0, 100.0)];
        assert!(fresh.tick(0.001, &mut one).is_empty(), "radius is still near 0");
    }

    #[test]
    fn meters_clamp() {
        let mut m = Meters::default();
        m.adjust(2, 0.25, false);
        m.adjust(2, 500.0, false);
        assert_eq!(m.god, 100.0);
        m.adjust(0, -1000.0, false);
        assert_eq!(m.health, 0.0);
    }
}





