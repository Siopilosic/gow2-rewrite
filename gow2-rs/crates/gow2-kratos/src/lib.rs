//! Kratos's controller logic, ported from the decompiled game (`docs/character-update.md` section 6). No engine types:
//! the front end (Bevy today) feeds it input and a time step and reads back position, heading and the animation to show.
//!
//! Units: the game works in 16 world units per metre (`UNITS_PER_METRE`); tuning is stored in metres. Positions here are
//! world units, speeds are units per second, angles are radians. Heading 0 faces -z (the way the model's toes point in the
//! bind pose); the facing vector is `(-sin h, -cos h)` in (x, z).

pub mod anim;
pub mod blades;
pub mod combat;
pub mod enemy;
pub mod magic;
pub mod moves;
pub mod world;

use std::f32::consts::PI;

/// 16 world units per metre (HIGH, `docs/character-update.md` 6.3).
pub const UNITS_PER_METRE: f32 = 16.0;

/// The fixed simulation rate: the PS2 NTSC frame rate.
pub const TICK_HZ: f32 = 59.94;

/// Ground locomotion tuning. The Kratos profile is CONFIRMED from RAM (`docs/character-update.md` 6.4); all six of his
/// moveset entries hold the same profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    /// Minimum moving speed once the stick is past the dead zone, m/s (`[0]`).
    pub min_speed: f32,
    /// Full-stick target speed, m/s (`[1]`).
    pub target_speed: f32,
    /// Acceleration, m/s^2 (`[2]`).
    pub accel: f32,
    /// Deceleration, m/s^2 (`[3]`).
    pub decel: f32,
    /// Turn rate: fraction of the remaining angle covered per 1/60 s (`[4]`).
    pub turn_rate: f32,
    /// Ground friction coefficient used when there is no stick input. The real value comes from the ground collider
    /// material and is not decoded; this default stops Kratos about as fast as the documented deceleration (LOW).
    pub friction: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning { min_speed: 0.9, target_speed: 7.5, accel: 50.0, decel: 50.0, turn_rate: 0.25, friction: 50.0 / 9.82 }
    }
}

/// One tick of player input after the front end has turned the pad into a stick direction in world space.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StickInput {
    /// Stick direction in the world (x, z), any length; its length is the stick magnitude.
    pub dir: [f32; 2],
}

impl StickInput {
    pub fn magnitude(&self) -> f32 {
        (self.dir[0] * self.dir[0] + self.dir[1] * self.dir[1]).sqrt().min(1.0)
    }
}

/// Wraps an angle into (-pi, pi].
pub fn wrap_angle(a: f32) -> f32 {
    let mut a = a % (2.0 * PI);
    if a > PI {
        a -= 2.0 * PI;
    } else if a <= -PI {
        a += 2.0 * PI;
    }
    a
}

/// Heading that faces the world direction `d` (x, z).
pub fn heading_of(d: [f32; 2]) -> f32 {
    (-d[0]).atan2(-d[1])
}

/// The speed approach of `FUN_00221ad8(target, current, accel, decel, min)` (`docs/character-update.md` 6.3, HIGH): a target
/// below 0.0001 becomes 0, otherwise it is raised to at least `min`; then `current` steps toward it by `accel` when speeding up
/// and `decel` when slowing down, without overshoot. All values here are in the same unit (the caller scales by dt).
pub fn approach_speed(target: f32, current: f32, accel_step: f32, decel_step: f32, min: f32) -> f32 {
    let mut t = target;
    if t.abs() < 0.0001 {
        t = 0.0;
    } else if t.abs() < min {
        t = min * t.signum();
    }
    if t > current {
        (current + accel_step).min(t)
    } else {
        (current - decel_step).max(t)
    }
}

/// Coulomb friction `FUN_0021afa8(mu, dt, v)`: above 1.6 units/s the speed shrinks by `mu * 9.82 * 16 * dt`, otherwise it stops.
pub fn friction(mu: f32, dt: f32, v: [f32; 2]) -> [f32; 2] {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len > 1.6 {
        let k = (1.0 - mu * 9.82 * UNITS_PER_METRE * dt / len).clamp(0.0, 1.0);
        [v[0] * k, v[1] * k]
    } else {
        [0.0, 0.0]
    }
}

/// Highest step Kratos walks up, and the drop he still sticks to the floor over, in world units (LOW: the capsule step height is not decoded).
pub const STEP_UP: f32 = 10.0;
pub const STEP_DOWN: f32 = 14.0;
/// Body size used for walls and ceilings: Kratos's tuning holds capsule sizes 0.6 and 2.2 m at `+0x24` and `+0x28`
/// (`docs/character-update.md`; which is radius and which is height is MEDIUM), times 16 units per metre.
pub const BODY_RADIUS: f32 = 0.6 * UNITS_PER_METRE;
pub const BODY_HEIGHT: f32 = 2.2 * UNITS_PER_METRE;

/// Locomotion state (the low 16 bits of `+0x170`, `docs/character-update.md` 1.1 and 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// State 1.
    Ground,
    /// State 4: jumping, going up.
    Rising,
    /// State 8: in the air, coming down.
    Falling,
}

/// What happened during a tick, for the animation system and effects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Events {
    pub jumped: bool,
    pub double_jumped: bool,
    pub landed: bool,
    /// The rise ended and the fall began this tick.
    pub started_falling: bool,
}

/// Player input for one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Controls {
    pub stick: StickInput,
    /// The jump button was pressed since the last tick (an edge, not a held state).
    pub jump: bool,
    /// A velocity (x, z, units per second) the animation drives: the root motion of a move. It replaces the stick locomotion and friction.
    pub drive: Option<[f32; 2]>,
}

/// Kratos's body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub pos: [f32; 3],
    /// Yaw in radians; see the module note.
    pub heading: f32,
    /// Current scalar move speed in units per second (`+0x2e0`).
    pub speed: f32,
    /// Horizontal velocity in units per second (`+0xe0`, x and z).
    pub vel: [f32; 2],
    /// Vertical velocity in units per second (`+0xe0`, y).
    pub vy: f32,
    pub mode: Mode,
    /// The double jump has been used since leaving the ground (`+0x378 & 4`).
    pub double_used: bool,
}

/// Air and jump tuning, Kratos values CONFIRMED from RAM (`docs/character-update.md` 8.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirTuning {
    /// Jump launch speed, m/s (`+0x78`).
    pub jump_speed: f32,
    /// Double-jump launch speed, m/s (`+0x7c`).
    pub double_jump_speed: f32,
    /// Rise turns into a fall when the vertical speed drops below this, m/s (`+0x88`).
    pub fall_switch: f32,
    /// Double jump allowed while `double_window_low < vy < double_window_high`, m/s (`+0x84`, `+0x80`).
    pub double_window: (f32, f32),
    /// Terminal fall speed, m/s (`+0x38`).
    pub terminal: f32,
    /// Horizontal acceleration in the air, m/s^2 (`+0x40`).
    pub accel: f32,
    /// Air steering factor (`+0x8c`, times the air-control ramp `+0x350`): used here as the turn-rate factor in the air (LOW).
    pub control: f32,
    /// Gravity, m/s^2 (`DAT_002d86c8`, 50.0 in the ELF; scaled per move by the move system, scale 1 here).
    pub gravity: f32,
}

impl Default for AirTuning {
    fn default() -> Self {
        AirTuning { jump_speed: 15.25, double_jump_speed: 15.25, fall_switch: 2.0, double_window: (-100.0, 20.0), terminal: 50.0, accel: 7.5, control: 0.05, gravity: 50.0 }
    }
}

impl Body {
    pub fn new(pos: [f32; 3], heading: f32) -> Self {
        Body { pos, heading, speed: 0.0, vel: [0.0, 0.0], vy: 0.0, mode: Mode::Ground, double_used: false }
    }

    /// Facing vector (x, z).
    pub fn facing(&self) -> [f32; 2] {
        [-self.heading.sin(), -self.heading.cos()]
    }

    /// One ground tick without jumping (kept for the simple tests and callers that only walk).
    pub fn tick(&mut self, dt: f32, input: StickInput, t: &Tuning) {
        self.tick_with(dt, Controls { stick: input, jump: false, drive: None }, t, &AirTuning::default());
    }

    /// One tick on flat ground at y = 0 (`docs/character-update.md` 6.2 and 8): stick locomotion on the ground, the air handler
    /// with gravity, the jump and the double jump, and landing. No slopes or collision yet.
    pub fn tick_with(&mut self, dt: f32, c: Controls, t: &Tuning, a: &AirTuning) -> Events {
        self.tick_world(dt, c, t, a, &world::FlatGround(0.0))
    }

    /// Like `tick_with`, on a level: Kratos follows the floor while walking (up steps of STEP_UP, down to STEP_DOWN), falls when
    /// the floor drops away, and lands on the first floor he meets. A walk-off ledge cancels the double jump (EnterFall mode 2,
    /// docs/character-update.md 8.1). No walls or ceilings yet.
    pub fn tick_world(&mut self, dt: f32, c: Controls, t: &Tuning, a: &AirTuning, world: &dyn world::World) -> Events {
        let mut ev = Events::default();
        let prev_y = self.pos[1];
        let m = UNITS_PER_METRE;

        // jump requests (Character_TryJump, 8.2)
        if c.jump {
            match self.mode {
                Mode::Ground => {
                    self.vy = a.jump_speed * m;
                    self.mode = Mode::Rising;
                    self.double_used = false;
                    ev.jumped = true;
                }
                Mode::Rising | Mode::Falling => {
                    let vy_mps = self.vy / m;
                    if !self.double_used && vy_mps > a.double_window.0 && vy_mps < a.double_window.1 {
                        // the vertical velocity is zeroed, then the launch speed is added
                        self.vy = a.double_jump_speed * m;
                        self.mode = Mode::Rising;
                        self.double_used = true;
                        ev.double_jumped = true;
                    }
                }
            }
        }

        // rise turns into a fall near the apex (8.2 step 1)
        if self.mode == Mode::Rising && self.vy < a.fall_switch * m {
            self.mode = Mode::Falling;
            ev.started_falling = true;
        }

        let mag = c.stick.magnitude();
        let airborne = self.mode != Mode::Ground;
        if let (Some(v), false) = (c.drive, airborne) {
            self.vel = v;
            self.speed = (v[0] * v[0] + v[1] * v[1]).sqrt();
        } else if !airborne {
            if mag <= 0.0 {
                // no stick: the speed is zeroed and friction acts on the velocity
                self.speed = 0.0;
                self.vel = friction(t.friction, dt, self.vel);
            } else {
                let target = mag * t.target_speed * m;
                self.speed = approach_speed(target, self.speed, t.accel * m * dt, t.decel * m * dt, t.min_speed * m);
                self.turn_toward(c.stick.dir, t.turn_rate, dt);
                let f = self.facing();
                self.vel = [f[0] * self.speed, f[1] * self.speed];
            }
        } else if mag > 0.0 {
            // in the air the speed is not cleared each tick; it approaches the target at the air acceleration, and
            // steering is much weaker (MEDIUM for the acceleration, LOW for the turn factor)
            let target = mag * t.target_speed * m;
            self.speed = approach_speed(target, self.speed, a.accel * m * dt, a.accel * m * dt, t.min_speed * m);
            self.turn_toward(c.stick.dir, a.control, dt);
            let f = self.facing();
            self.vel = [f[0] * self.speed, f[1] * self.speed];
        }
        // with no stick in the air the velocity simply carries on (no friction in the air)

        // gravity unless at rest on the ground (6.2 step 4)
        if airborne {
            self.vy -= a.gravity * m * dt;
            self.vy = self.vy.max(-a.terminal * m);
        }

        self.pos[0] += self.vel[0] * dt;
        self.pos[2] += self.vel[1] * dt;
        self.pos[1] += self.vy * dt;

        // walls: slide out of them, and lose the velocity that points into them. On the ground contacts below the step
        // height are kerbs, not walls.
        let y0 = self.pos[1] + if airborne { 0.0 } else { STEP_UP };
        let push = world.push_out(self.pos, BODY_RADIUS, y0, (self.pos[1] + BODY_HEIGHT - BODY_RADIUS).max(y0));
        if push != [0.0, 0.0] {
            self.pos[0] += push[0];
            self.pos[2] += push[1];
            let len = (push[0] * push[0] + push[1] * push[1]).sqrt();
            let n = [push[0] / len, push[1] / len];
            let into = self.vel[0] * n[0] + self.vel[1] * n[1];
            if into < 0.0 {
                self.vel = [self.vel[0] - into * n[0], self.vel[1] - into * n[1]];
            }
        }

        // ceiling: a rising body hits its head and starts to fall
        if self.mode == Mode::Rising && self.vy > 0.0 {
            if let Some(cy) = world.ceiling(self.pos[0], self.pos[2], prev_y + BODY_HEIGHT - 0.5) {
                if self.pos[1] + BODY_HEIGHT > cy {
                    self.pos[1] = cy - BODY_HEIGHT;
                    self.vy = 0.0;
                    self.mode = Mode::Falling;
                    ev.started_falling = true;
                }
            }
        }

        if airborne {
            // land on the highest floor that was at or below the start-of-tick height
            if self.vy <= 0.0 {
                if let Some(fy) = world.floor(self.pos[0], self.pos[2], prev_y.max(self.pos[1]) + 0.5) {
                    if self.pos[1] <= fy {
                        self.pos[1] = fy;
                        self.vy = 0.0;
                        self.mode = Mode::Ground;
                        self.double_used = false;
                        ev.landed = true;
                    }
                }
            }
        } else {
            match world.floor(self.pos[0], self.pos[2], self.pos[1] + STEP_UP) {
                Some(fy) if fy >= self.pos[1] - STEP_DOWN => self.pos[1] = fy,
                _ => {
                    // the floor dropped away: walk off the ledge into a fall, with no double jump
                    self.mode = Mode::Falling;
                    self.vy = 0.0;
                    self.double_used = true;
                    ev.started_falling = true;
                }
            }
        }
        ev
    }

    fn turn_toward(&mut self, dir: [f32; 2], rate: f32, dt: f32) {
        // turn toward the stick direction: an exponential approach, a fraction of the remaining angle per tick
        let desired = heading_of(dir);
        let diff = wrap_angle(desired - self.heading);
        if diff.abs() >= 1e-4 {
            self.heading = wrap_angle(self.heading + diff * (rate * dt * 60.0).min(1.0));
        }
    }

    /// Horizontal speed in metres per second.
    pub fn speed_mps(&self) -> f32 {
        (self.vel[0] * self.vel[0] + self.vel[1] * self.vel[1]).sqrt() / UNITS_PER_METRE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(body: &mut Body, secs: f32, stick: [f32; 2]) {
        let dt = 1.0 / TICK_HZ;
        for _ in 0..(secs / dt).round() as usize {
            body.tick(dt, StickInput { dir: stick }, &Tuning::default());
        }
    }

    #[test]
    fn reaches_full_speed_in_a_sixth_of_a_second() {
        let mut b = Body::new([0.0; 3], 0.0);
        run(&mut b, 0.15, [0.0, -1.0]);
        assert!((b.speed_mps() - 7.5).abs() < 0.1, "speed {}", b.speed_mps());
    }

    #[test]
    fn small_stick_still_moves_at_the_minimum_speed() {
        let mut b = Body::new([0.0; 3], 0.0);
        run(&mut b, 0.5, [0.0, -0.01]);
        assert!(b.speed_mps() >= 0.9 - 1e-3, "speed {}", b.speed_mps());
        assert!(b.speed_mps() < 1.0 + 1e-3);
    }

    #[test]
    fn stops_quickly_without_input() {
        let mut b = Body::new([0.0; 3], 0.0);
        run(&mut b, 0.5, [0.0, -1.0]);
        run(&mut b, 0.25, [0.0, 0.0]);
        assert!(b.speed_mps() < 0.2, "speed {}", b.speed_mps());
    }

    #[test]
    fn turns_toward_the_stick_by_a_fraction_per_tick() {
        let mut b = Body::new([0.0; 3], 0.0);
        // desired heading for world direction +x is atan2(-1, 0) = -pi/2
        let dt = 1.0 / 60.0;
        b.tick(dt, StickInput { dir: [1.0, 0.0] }, &Tuning::default());
        let expected = -PI / 2.0 * 0.25;
        assert!((b.heading - expected).abs() < 1e-4, "heading {} expected {}", b.heading, expected);
        run(&mut b, 1.0, [1.0, 0.0]);
        assert!((b.heading + PI / 2.0).abs() < 0.01);
    }

    #[test]
    fn turns_the_short_way_round() {
        let mut b = Body::new([0.0; 3], 3.0);
        // the desired heading -3.0 is 0.28 rad away through pi, not 6 rad the other way
        let target = -3.0f32;
        let stick = [-target.sin(), -target.cos()];
        b.tick(1.0 / 60.0, StickInput { dir: stick }, &Tuning::default());
        assert!(b.heading > 3.0 || b.heading < -3.0, "heading {}", b.heading);
    }

    fn jump_run(body: &mut Body, secs: f32, stick: [f32; 2], jump_at: &[usize]) -> (Vec<(usize, Events)>, f32) {
        let dt = 1.0 / TICK_HZ;
        let (t, a) = (Tuning::default(), AirTuning::default());
        let mut events = Vec::new();
        let mut apex = 0.0f32;
        for i in 0..(secs / dt).round() as usize {
            let ev = body.tick_with(dt, Controls { stick: StickInput { dir: stick }, jump: jump_at.contains(&i), ..Default::default() }, &t, &a);
            apex = apex.max(body.pos[1]);
            if ev != Events::default() {
                events.push((i, ev));
            }
        }
        (events, apex)
    }

    #[test]
    fn a_jump_rises_about_2_3_metres_and_lands() {
        let mut b = Body::new([0.0; 3], 0.0);
        let (events, apex) = jump_run(&mut b, 1.5, [0.0, 0.0], &[0]);
        // v^2 / 2g with the switch to falling at 2 m/s: (15.25^2 - 2^2) / 100 = 2.29 m = 36.7 units
        assert!((apex - 36.7).abs() < 2.5, "apex {apex}");
        assert!(events.first().unwrap().1.jumped);
        assert!(events.iter().any(|e| e.1.started_falling));
        assert!(events.last().unwrap().1.landed);
        assert_eq!(b.mode, Mode::Ground);
        assert_eq!(b.pos[1], 0.0);
    }

    #[test]
    fn the_whole_jump_takes_about_a_second_to_land_after_the_apex() {
        let mut b = Body::new([0.0; 3], 0.0);
        let (events, _) = jump_run(&mut b, 2.0, [0.0, 0.0], &[0]);
        let land = events.iter().find(|e| e.1.landed).unwrap().0 as f32 / TICK_HZ;
        // up for ~0.27 s, then a ~2.3 m fall under 50 m/s^2 takes ~0.3 s
        assert!(land > 0.5 && land < 0.8, "landed after {land} s");
    }

    #[test]
    fn a_double_jump_adds_the_same_height_again_but_only_once() {
        let mut b = Body::new([0.0; 3], 0.0);
        let dt = 1.0 / TICK_HZ;
        // second press near the apex (0.3 s), a third press right after must do nothing
        let second = (0.3 / dt) as usize;
        let (events, apex) = jump_run(&mut b, 2.0, [0.0, 0.0], &[0, second, second + 20]);
        assert_eq!(events.iter().filter(|e| e.1.double_jumped).count(), 1);
        assert!(apex > 60.0, "double jump apex {apex}");
    }

    #[test]
    fn jumping_in_the_air_without_a_press_does_nothing() {
        let mut b = Body::new([0.0; 3], 0.0);
        let (events, _) = jump_run(&mut b, 0.2, [0.0, 0.0], &[]);
        assert!(events.is_empty());
    }

    #[test]
    fn horizontal_speed_carries_through_a_jump() {
        let mut b = Body::new([0.0; 3], 0.0);
        let dt = 1.0 / TICK_HZ;
        let t = Tuning::default();
        for _ in 0..40 {
            b.tick(dt, StickInput { dir: [0.0, -1.0] }, &t);
        }
        let z0 = b.pos[2];
        let (_, _) = jump_run(&mut b, 0.5, [0.0, 0.0], &[0]);
        // with the stick released in the air the velocity carries on (no friction in the air): about 120 units/s for the airborne time
        assert!(z0 - b.pos[2] > 40.0, "moved {} in the air", z0 - b.pos[2]);
    }

    #[test]
    fn air_steering_is_weaker_than_ground_steering() {
        let (mut ground, mut air) = (Body::new([0.0; 3], 0.0), Body::new([0.0; 3], 0.0));
        let dt = 1.0 / TICK_HZ;
        let (t, a) = (Tuning::default(), AirTuning::default());
        air.tick_with(dt, Controls { stick: StickInput::default(), jump: true, ..Default::default() }, &t, &a);
        for _ in 0..3 {
            ground.tick_with(dt, Controls { stick: StickInput { dir: [1.0, 0.0] }, jump: false, ..Default::default() }, &t, &a);
            air.tick_with(dt, Controls { stick: StickInput { dir: [1.0, 0.0] }, jump: false, ..Default::default() }, &t, &a);
        }
        // after a few ticks the ground turn (rate 0.25) is well ahead of the air turn (factor 0.05)
        assert!(ground.heading.abs() > air.heading.abs() * 3.0, "ground {} air {}", ground.heading, air.heading);
    }

    fn level_run(body: &mut Body, world: &dyn world::World, secs: f32, stick: [f32; 2]) -> Vec<Events> {
        let dt = 1.0 / TICK_HZ;
        let (t, a) = (Tuning::default(), AirTuning::default());
        let mut out = Vec::new();
        for _ in 0..(secs / dt).round() as usize {
            let ev = body.tick_world(dt, Controls { stick: StickInput { dir: stick }, jump: false, ..Default::default() }, &t, &a, world);
            if ev != Events::default() {
                out.push(ev);
            }
        }
        out
    }

    /// Two floors: a low one for z > -100 and a platform 30 units higher beyond it, with a gap between at z -100..-110.
    fn ledge_world() -> world::TriWorld {
        let quad = |y: f32, z0: f32, z1: f32| {
            vec![[[-200.0, y, z0], [200.0, y, z0], [200.0, y, z1]], [[-200.0, y, z0], [200.0, y, z1], [-200.0, y, z1]]]
        };
        let mut tris = quad(0.0, -100.0, 200.0);
        tris.extend(quad(-30.0, -400.0, -110.0));
        world::TriWorld::new(tris, 16.0)
    }

    #[test]
    fn walking_off_a_ledge_starts_a_fall_with_no_double_jump_then_lands_lower() {
        let w = ledge_world();
        let mut b = Body::new([0.0, 0.0, 0.0], 0.0);
        let events = level_run(&mut b, &w, 1.2, [0.0, -1.0]);
        assert!(events.iter().any(|e| e.started_falling), "no fall: {events:?}");
        assert!(events.last().unwrap().landed, "no landing: {events:?}");
        assert_eq!(b.mode, Mode::Ground);
        assert!((b.pos[1] + 30.0).abs() < 0.01, "ended at y {}", b.pos[1]);
        // the double jump was cancelled by the ledge, but landing restores it
        assert!(!b.double_used);
    }

    #[test]
    fn a_double_jump_is_refused_in_the_air_after_walking_off_a_ledge() {
        let w = ledge_world();
        let mut b = Body::new([0.0, 0.0, -95.0], 0.0);
        let (t, a) = (Tuning::default(), AirTuning::default());
        let dt = 1.0 / TICK_HZ;
        let mut fell = false;
        for _ in 0..40 {
            let ev = b.tick_world(dt, Controls { stick: StickInput { dir: [0.0, -1.0] }, jump: false, ..Default::default() }, &t, &a, &w);
            fell |= ev.started_falling;
            if fell {
                break;
            }
        }
        assert!(fell && b.double_used);
        let ev = b.tick_world(dt, Controls { stick: StickInput::default(), jump: true, ..Default::default() }, &t, &a, &w);
        assert!(!ev.double_jumped);
    }

    #[test]
    fn a_small_step_up_is_walked_up_and_a_high_wall_top_is_not() {
        let quad = |y: f32, z0: f32, z1: f32| vec![[[-200.0, y, z0], [200.0, y, z0], [200.0, y, z1]], [[-200.0, y, z0], [200.0, y, z1], [-200.0, y, z1]]];
        let mut tris = quad(0.0, 0.0, 200.0);
        tris.extend(quad(6.0, -300.0, 0.0)); // a 6 unit step: walkable
        let w = world::TriWorld::new(tris, 16.0);
        let mut b = Body::new([0.0, 0.0, 50.0], 0.0);
        level_run(&mut b, &w, 1.0, [0.0, -1.0]);
        assert!((b.pos[1] - 6.0).abs() < 0.01 && b.mode == Mode::Ground, "y {} {:?}", b.pos[1], b.mode);
        // a 30 unit rise is above the step height: the floor query finds nothing within reach, so he would fall through
        // (walls are not handled yet); here the controller must not teleport him up
        let mut tris = quad(0.0, 0.0, 200.0);
        tris.extend(quad(30.0, -300.0, 0.0));
        let w = world::TriWorld::new(tris, 16.0);
        let mut b = Body::new([0.0, 0.0, 50.0], 0.0);
        level_run(&mut b, &w, 0.8, [0.0, -1.0]);
        assert!(b.pos[1] < 30.0 - STEP_UP, "teleported up to {}", b.pos[1]);
    }

    #[test]
    fn a_jump_lands_on_a_platform_it_reaches_but_not_one_above_it() {
        let quad = |y: f32, z0: f32, z1: f32| vec![[[-200.0, y, z0], [200.0, y, z0], [200.0, y, z1]], [[-200.0, y, z0], [200.0, y, z1], [-200.0, y, z1]]];
        let mut tris = quad(0.0, -400.0, 400.0);
        tris.extend(quad(25.0, -400.0, -30.0)); // 25 high: below the 36.7 apex, on top of the ground
        let w = world::TriWorld::new(tris, 16.0);
        let (t, a) = (Tuning::default(), AirTuning::default());
        let dt = 1.0 / TICK_HZ;
        let mut b = Body::new([0.0, 0.0, 20.0], 0.0);
        // jump while running toward the platform
        let mut landed_y = None;
        for i in 0..120 {
            let ev = b.tick_world(dt, Controls { stick: StickInput { dir: [0.0, -1.0] }, jump: i == 0, ..Default::default() }, &t, &a, &w);
            if ev.landed {
                landed_y = Some(b.pos[1]);
                break;
            }
        }
        let y = landed_y.expect("never landed");
        assert!(y == 0.0 || y == 25.0, "landed at {y}");
    }

    #[test]
    fn moves_along_the_facing_direction() {
        let mut b = Body::new([0.0; 3], 0.0);
        run(&mut b, 1.0, [0.0, -1.0]);
        assert!(b.pos[2] < -100.0 && b.pos[0].abs() < 1e-3, "pos {:?}", b.pos);
    }

    type Tri = [[f32; 3]; 3];

    /// Upward-facing floor over x -300..300 from z0 to z1.
    fn up_floor(y: f32, z0: f32, z1: f32) -> Vec<Tri> {
        vec![[[-300.0, y, z0], [300.0, y, z1], [300.0, y, z0]], [[-300.0, y, z0], [-300.0, y, z1], [300.0, y, z1]]]
    }

    /// A wall in the plane z = `z` over x -300..300 from y0 to y1.
    fn z_wall(z: f32, y0: f32, y1: f32) -> Vec<Tri> {
        vec![[[-300.0, y0, z], [-300.0, y1, z], [300.0, y1, z]], [[-300.0, y0, z], [300.0, y1, z], [300.0, y0, z]]]
    }

    #[test]
    fn walking_into_a_wall_stops_at_the_body_radius() {
        let mut tris = up_floor(0.0, -200.0, 200.0);
        tris.extend(z_wall(-50.0, 0.0, 80.0));
        let w = world::CollisionWorld::new(tris, 16.0);
        let mut b = Body::new([0.0, 0.0, 0.0], 0.0);
        level_run(&mut b, &w, 1.5, [0.0, -1.0]);
        assert!((b.pos[2] + 50.0 - BODY_RADIUS).abs() < 0.5, "stopped at z {}", b.pos[2]);
        assert_eq!(b.mode, Mode::Ground);
        assert!(b.pos[1].abs() < 0.01);
    }

    #[test]
    fn a_diagonal_walk_into_a_wall_slides_along_it() {
        let mut tris = up_floor(0.0, -200.0, 200.0);
        tris.extend(z_wall(-50.0, 0.0, 80.0));
        let w = world::CollisionWorld::new(tris, 16.0);
        let mut b = Body::new([0.0, 0.0, -20.0], 0.0);
        let s = std::f32::consts::FRAC_1_SQRT_2;
        level_run(&mut b, &w, 1.0, [s, -s]);
        assert!(b.pos[0] > 60.0, "did not slide: x {}", b.pos[0]);
        assert!(b.pos[2] >= -50.0 + BODY_RADIUS - 0.5, "went into the wall: z {}", b.pos[2]);
    }

    #[test]
    fn a_low_riser_between_two_floors_is_walked_over() {
        let mut tris = up_floor(0.0, 0.0, 200.0);
        tris.extend(up_floor(6.0, -200.0, 0.0));
        tris.extend(z_wall(0.0, 0.0, 6.0));
        let w = world::CollisionWorld::new(tris, 16.0);
        let mut b = Body::new([0.0, 0.0, 50.0], 0.0);
        level_run(&mut b, &w, 1.0, [0.0, -1.0]);
        assert!(b.pos[2] < -40.0 && (b.pos[1] - 6.0).abs() < 0.01, "pos {:?}", b.pos);
    }

    #[test]
    fn a_jump_under_a_low_ceiling_bumps_the_head_and_falls() {
        let mut tris = up_floor(0.0, -200.0, 200.0);
        // a ceiling 50 units up, facing down
        tris.extend([[[-300.0, 50.0, -200.0], [300.0, 50.0, -200.0], [300.0, 50.0, 200.0]], [[-300.0, 50.0, -200.0], [300.0, 50.0, 200.0], [-300.0, 50.0, 200.0]]]);
        let w = world::CollisionWorld::new(tris, 16.0);
        let (t, a) = (Tuning::default(), AirTuning::default());
        let dt = 1.0 / TICK_HZ;
        let mut b = Body::new([0.0; 3], 0.0);
        let (mut top, mut landed) = (0.0f32, false);
        for i in 0..150 {
            let ev = b.tick_world(dt, Controls { stick: StickInput::default(), jump: i == 0, ..Default::default() }, &t, &a, &w);
            top = top.max(b.pos[1]);
            landed |= ev.landed;
        }
        assert!((top - (50.0 - BODY_HEIGHT)).abs() < 0.01, "highest feet y {top}");
        assert!(landed && b.mode == Mode::Ground);
    }
}







