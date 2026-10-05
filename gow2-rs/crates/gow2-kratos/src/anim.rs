//! Which animation Kratos shows, and how far through it.
//!
//! On the ground locomotion follows the game's rule that the walk cycle is **distance driven** (`docs/character-update.md`
//! 7.2: the playback speed is the distance moved divided by the cycle length, so the feet do not slide). The game's own
//! `walkBlend` node (five embedded clips, `docs/animation.md`) is not decoded yet; this mixes the two walk clips it is built
//! from by speed (MEDIUM: a stand-in for the real blend).
//!
//! In the air the clips follow the move table: `MOV_Jump` is `navJump`, `MOV_VJump` (straight up) is `navJumpUp`,
//! `MOV_DblJump` is `navDoubleJump`, `MOV_Fall` is `navFall`, `MOV_HighFall` is `navFallLoop`, `MOV_Land` is `navLand`
//! (`analysis/dc/R_HERO00/moves.tsv`). Which one plays follows `Character_StartJump` / `EnterFall` (`docs/character-update.md` 8.1):
//! the straight-up variants when the horizontal speed is within 1 m/s, the high fall below -40 m/s.

use crate::{Events, Mode, UNITS_PER_METRE};
use gow2_skel::{world_matrices, Clip, Skeleton};

pub const IDLE: &str = "navIdle";
pub const WALK_SLOW: &str = "navWalkSlow";
pub const WALK_FAST: &str = "navWalkFast";
pub const JUMP: &str = "navJump";
pub const JUMP_UP: &str = "navJumpUp";
pub const DOUBLE_JUMP: &str = "navDoubleJump";
pub const FALL: &str = "navFall";
pub const FALL_LOOP: &str = "navFallLoop";
pub const LAND: &str = "navLand";

/// Every clip the locomotion can ask for.
pub const ALL_CLIPS: [&str; 9] = [IDLE, WALK_SLOW, WALK_FAST, JUMP, JUMP_UP, DOUBLE_JUMP, FALL, FALL_LOOP, LAND];

/// What the animation system needs to know about a clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipInfo {
    pub duration: f32,
    /// Ground speed of the clip's feet in units per second (0 for clips that stay in place).
    pub ground_speed: f32,
}

/// Ground speed of a walking or running clip: the horizontal speed at which the planted foot moves backwards relative to
/// the pelvis. A foot counts as planted while it is within one unit of the lowest height it reaches.
pub fn ground_speed(skel: &Skeleton, clip: &Clip) -> f32 {
    let idx = |n: &str| skel.names.iter().position(|x| x == n);
    let (Some(lf), Some(rf), Some(pel)) = (idx("lMetatarsal"), idx("rMetatarsal"), idx("pelvis")) else { return 0.0 };
    let frames = ((clip.duration / clip.dt).round() as usize).max(2);
    let sample = |f: usize| {
        let w = world_matrices(skel, &clip.sample(skel, f as f32 * clip.dt));
        let p = |j: usize| [w[j][12], w[j][13], w[j][14]];
        (p(lf), p(rf), p(pel))
    };
    let poses: Vec<_> = (0..=frames).map(sample).collect();
    let lowest = poses.iter().map(|(l, r, _)| l[1].min(r[1])).fold(f32::MAX, f32::min);
    let (mut sum, mut n) = (0.0f32, 0usize);
    for w in poses.windows(2) {
        for (a, b) in [(w[0].0, w[1].0), (w[0].1, w[1].1)] {
            if a[1] < lowest + 1.0 && b[1] < lowest + 1.0 {
                // movement of the foot relative to the pelvis
                let (dx, dz) = ((b[0] - w[1].2[0]) - (a[0] - w[0].2[0]), (b[2] - w[1].2[2]) - (a[2] - w[0].2[2]));
                sum += (dx * dx + dz * dz).sqrt() / clip.dt;
                n += 1;
            }
        }
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f32
    }
}

/// One layer of the pose to show: a clip, the time in it, and its weight.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub clip: &'static str,
    pub time: f32,
    pub weight: f32,
}

/// The body's air state for one tick, as the animation needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirInfo {
    pub mode: Mode,
    /// Vertical velocity in units per second.
    pub vy: f32,
    /// Events of this tick.
    pub events: Events,
}

impl AirInfo {
    pub const GROUND: AirInfo = AirInfo { mode: Mode::Ground, vy: 0.0, events: Events { jumped: false, double_jumped: false, landed: false, started_falling: false } };
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Ground,
    /// A clip started by a jump, a fall or a landing; `looped` clips repeat, others hold their last frame.
    Action { clip: &'static str, time: f32, looped: bool },
}

/// Idle, walk, jump, fall and land state.
#[derive(Debug, Clone)]
pub struct Locomotion {
    idle_time: f32,
    /// Walk cycle phase, 0..1, shared by both walk clips.
    phase: f32,
    /// How much of the ground pose is the walk (0 = idle, 1 = walking); eased over `FADE` seconds.
    walk_weight: f32,
    state: State,
    /// Layers of the previous update, and how long the fade from them has run (when the state changed).
    last: Vec<Layer>,
    key: &'static str,
    fade_from: Vec<Layer>,
    fade_t: f32,
}

/// Seconds to fade between idle and walk (LOW: the game's blend times are not decoded).
pub const FADE: f32 = 0.15;
/// Seconds to fade when the air state changes (LOW).
pub const AIR_FADE: f32 = 0.08;
/// Vertical speed beyond which the high-fall clip plays: -40 m/s (`tuning+0x3c`, CONFIRMED).
pub const HIGH_FALL_SPEED: f32 = 40.0 * UNITS_PER_METRE;
/// Horizontal speed within which the straight-up jump and fall clips play: 1 m/s (HIGH, `docs/character-update.md` 8.1).
pub const STRAIGHT_SPEED: f32 = UNITS_PER_METRE;

impl Default for Locomotion {
    fn default() -> Self {
        Locomotion { idle_time: 0.0, phase: 0.0, walk_weight: 0.0, state: State::Ground, last: Vec::new(), key: "ground", fade_from: Vec::new(), fade_t: 1.0 }
    }
}

impl Locomotion {
    /// Advance by `dt` seconds. `speed` is the horizontal speed in units per second; `info` gives clip durations and ground speeds.
    pub fn update(&mut self, dt: f32, speed: f32, air: AirInfo, info: &dyn Fn(&str) -> ClipInfo) -> Vec<Layer> {
        // 1. state changes from this tick's events and the air mode
        let ev = air.events;
        let straight = speed < STRAIGHT_SPEED;
        if ev.double_jumped {
            self.state = State::Action { clip: DOUBLE_JUMP, time: 0.0, looped: false };
        } else if ev.jumped {
            self.state = State::Action { clip: if straight { JUMP_UP } else { JUMP }, time: 0.0, looped: false };
        } else if ev.landed {
            self.state = State::Action { clip: LAND, time: 0.0, looped: false };
        } else if air.mode == Mode::Falling {
            let high = air.vy < -HIGH_FALL_SPEED;
            let want = if high { FALL_LOOP } else { FALL };
            match &self.state {
                // switch to the fall at the apex, or to the high fall once fast enough
                State::Action { clip, .. } if *clip == want => {}
                State::Action { clip, .. } if (*clip == JUMP || *clip == JUMP_UP || *clip == DOUBLE_JUMP) && !ev.started_falling && !high => {}
                _ => self.state = State::Action { clip: want, time: 0.0, looped: want == FALL_LOOP },
            }
        }
        // 2. advance
        let mut ground_now = false;
        match &mut self.state {
            State::Action { clip, time, looped } => {
                *time += dt;
                let d = info(clip).duration.max(0.01);
                if *looped {
                    *time %= d;
                }
                if *clip == LAND && air.mode == Mode::Ground {
                    // landing ends when the clip ends, or earlier once Kratos moves off
                    if *time >= d || (speed > 20.0 && *time > 0.2) {
                        ground_now = true;
                    }
                } else if air.mode == Mode::Ground && *clip != LAND {
                    ground_now = true;
                }
            }
            State::Ground => {}
        }
        if ground_now {
            self.state = State::Ground;
        }
        // 3. build the layers
        let layers = match &self.state {
            State::Action { clip, time, .. } => {
                let d = info(clip).duration;
                vec![Layer { clip, time: time.min(d), weight: 1.0 }]
            }
            State::Ground => self.ground_layers(dt, speed, info),
        };
        // 4. cross-fade when the primary clip changes
        let key = layers.first().map_or("ground", |l| if matches!(self.state, State::Ground) { "ground" } else { l.clip });
        if key != self.key {
            self.fade_from = self.last.clone();
            self.fade_t = 0.0;
            self.key = key;
        }
        self.last = layers.clone();
        if self.fade_t < AIR_FADE && !self.fade_from.is_empty() {
            self.fade_t += dt;
            let k = (self.fade_t / AIR_FADE).clamp(0.0, 1.0);
            let mut out: Vec<Layer> = self.fade_from.iter().map(|l| Layer { clip: l.clip, time: l.time, weight: l.weight * (1.0 - k) }).collect();
            out.extend(layers.iter().map(|l| Layer { clip: l.clip, time: l.time, weight: l.weight * k }));
            out.retain(|l| l.weight > 0.0005);
            return out;
        }
        layers
    }

    fn ground_layers(&mut self, dt: f32, speed: f32, info: &dyn Fn(&str) -> ClipInfo) -> Vec<Layer> {
        let (idle, slow, fast) = (info(IDLE), info(WALK_SLOW), info(WALK_FAST));
        let moving = speed > 1.0;
        let target = if moving { 1.0 } else { 0.0 };
        self.walk_weight += (target - self.walk_weight) * (dt / FADE).min(1.0);
        if (self.walk_weight - target).abs() < 0.002 {
            self.walk_weight = target;
        }
        self.idle_time = (self.idle_time + dt) % idle.duration.max(0.01);
        // blend the two walk clips by speed between their own ground speeds
        let (vs, vf) = (slow.ground_speed.max(1.0), fast.ground_speed.max(slow.ground_speed + 1.0));
        let w_fast = ((speed - vs) / (vf - vs)).clamp(0.0, 1.0);
        if moving {
            // cycle length in units is the weighted mix of the two clips' cycle lengths
            let cycle = (1.0 - w_fast) * slow.ground_speed * slow.duration + w_fast * fast.ground_speed * fast.duration;
            self.phase = (self.phase + speed * dt / cycle.max(1.0)).fract();
        }
        let mut out = Vec::new();
        let ww = self.walk_weight;
        if ww < 1.0 {
            out.push(Layer { clip: IDLE, time: self.idle_time, weight: 1.0 - ww });
        }
        if ww > 0.0 {
            if w_fast < 1.0 {
                out.push(Layer { clip: WALK_SLOW, time: self.phase * slow.duration, weight: ww * (1.0 - w_fast) });
            }
            if w_fast > 0.0 {
                out.push(Layer { clip: WALK_FAST, time: self.phase * fast.duration, weight: ww * w_fast });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(n: &str) -> ClipInfo {
        match n {
            IDLE => ClipInfo { duration: 2.0, ground_speed: 0.0 },
            WALK_SLOW => ClipInfo { duration: 1.33, ground_speed: 30.0 },
            WALK_FAST => ClipInfo { duration: 0.67, ground_speed: 100.0 },
            JUMP | JUMP_UP => ClipInfo { duration: 0.6, ground_speed: 0.0 },
            DOUBLE_JUMP => ClipInfo { duration: 0.67, ground_speed: 0.0 },
            FALL => ClipInfo { duration: 0.77, ground_speed: 0.0 },
            FALL_LOOP => ClipInfo { duration: 1.0, ground_speed: 0.0 },
            LAND => ClipInfo { duration: 0.53, ground_speed: 0.0 },
            _ => ClipInfo { duration: 1.0, ground_speed: 0.0 },
        }
    }

    fn total(layers: &[Layer]) -> f32 {
        layers.iter().map(|l| l.weight).sum()
    }

    fn ev(f: impl FnOnce(&mut Events)) -> Events {
        let mut e = Events::default();
        f(&mut e);
        e
    }

    #[test]
    fn standing_still_plays_only_the_idle() {
        let mut l = Locomotion::default();
        let out = l.update(0.016, 0.0, AirInfo::GROUND, &info);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].clip, IDLE);
        assert!((out[0].weight - 1.0).abs() < 1e-6);
    }

    #[test]
    fn layer_weights_always_sum_to_one() {
        let mut l = Locomotion::default();
        for speed in [0.0, 5.0, 40.0, 70.0, 120.0, 120.0, 0.0, 0.0] {
            for _ in 0..20 {
                let out = l.update(0.016, speed, AirInfo::GROUND, &info);
                assert!((total(&out) - 1.0).abs() < 1e-4, "speed {speed}: {out:?}");
            }
        }
    }

    #[test]
    fn full_speed_ends_on_the_fast_walk_only() {
        let mut l = Locomotion::default();
        let mut out = Vec::new();
        for _ in 0..60 {
            out = l.update(0.016, 120.0, AirInfo::GROUND, &info);
        }
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].clip, WALK_FAST);
    }

    #[test]
    fn the_cycle_advances_with_distance_not_time() {
        let (mut a, mut b) = (Locomotion::default(), Locomotion::default());
        for _ in 0..30 {
            a.update(0.016, 60.0, AirInfo::GROUND, &info);
            b.update(0.016, 120.0, AirInfo::GROUND, &info);
        }
        assert!(b.phase != a.phase);
    }

    fn air(mode: Mode, vy: f32, events: Events) -> AirInfo {
        AirInfo { mode, vy, events }
    }

    #[test]
    fn a_standing_jump_plays_the_straight_up_clip_and_a_running_jump_the_forward_one() {
        let mut l = Locomotion::default();
        let out = l.update(0.016, 0.0, air(Mode::Rising, 240.0, ev(|e| e.jumped = true)), &info);
        assert!(out.iter().any(|x| x.clip == JUMP_UP), "{out:?}");
        let mut l = Locomotion::default();
        let out = l.update(0.016, 100.0, air(Mode::Rising, 240.0, ev(|e| e.jumped = true)), &info);
        assert!(out.iter().any(|x| x.clip == JUMP), "{out:?}");
    }

    #[test]
    fn jump_then_fall_then_land_then_idle() {
        let mut l = Locomotion::default();
        l.update(0.016, 0.0, air(Mode::Rising, 240.0, ev(|e| e.jumped = true)), &info);
        for _ in 0..10 {
            l.update(0.016, 0.0, air(Mode::Rising, 100.0, Events::default()), &info);
        }
        let out = l.update(0.016, 0.0, air(Mode::Falling, 20.0, ev(|e| e.started_falling = true)), &info);
        assert!(out.iter().any(|x| x.clip == FALL), "{out:?}");
        // far past the fade
        for _ in 0..20 {
            l.update(0.016, 0.0, air(Mode::Falling, -200.0, Events::default()), &info);
        }
        let out = l.update(0.016, 0.0, air(Mode::Ground, 0.0, ev(|e| e.landed = true)), &info);
        assert!(out.iter().any(|x| x.clip == LAND), "{out:?}");
        for _ in 0..60 {
            l.update(0.016, 0.0, AirInfo::GROUND, &info);
        }
        let out = l.update(0.016, 0.0, AirInfo::GROUND, &info);
        assert_eq!(out[0].clip, IDLE, "{out:?}");
    }

    #[test]
    fn a_long_fall_switches_to_the_high_fall_clip() {
        let mut l = Locomotion::default();
        l.update(0.016, 0.0, air(Mode::Falling, 0.0, ev(|e| e.started_falling = true)), &info);
        for _ in 0..30 {
            l.update(0.016, 0.0, air(Mode::Falling, -700.0, Events::default()), &info);
        }
        let out = l.update(0.016, 0.0, air(Mode::Falling, -700.0, Events::default()), &info);
        assert!(out.iter().any(|x| x.clip == FALL_LOOP), "{out:?}");
    }

    #[test]
    fn air_layer_weights_sum_to_one_through_a_whole_jump() {
        let mut l = Locomotion::default();
        let seq: Vec<AirInfo> = std::iter::once(air(Mode::Rising, 240.0, ev(|e| e.jumped = true)))
            .chain((0..15).map(|_| air(Mode::Rising, 100.0, Events::default())))
            .chain(std::iter::once(air(Mode::Falling, 20.0, ev(|e| e.started_falling = true))))
            .chain((0..20).map(|_| air(Mode::Falling, -200.0, Events::default())))
            .chain(std::iter::once(air(Mode::Ground, 0.0, ev(|e| e.landed = true))))
            .chain((0..60).map(|_| AirInfo::GROUND))
            .collect();
        for a in seq {
            let out = l.update(0.016, 0.0, a, &info);
            assert!((total(&out) - 1.0).abs() < 1e-3, "{out:?}");
        }
    }
}
