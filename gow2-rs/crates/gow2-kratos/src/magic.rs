//! Magic casting: the script natives of the five magics (`docs/magic.md`), driven by the move data.
//!
//! A magic move carries *script actions* (kind `0x0d`): each names a script class by hash (`SCR_Lightning`, `SCR_ElectricCore`, `SCR_EarthStomp`, `SCR_WindThrow`,
//! `SCR_MedusaHeadFlash`, ...). The game creates a script object when the action fires and ties it to the move: it runs every frame until the move ends. This module
//! does the same with plain Rust: [`MagicSys::update`] takes the actions that fired, keeps a list of live scripts, runs the ones that work every frame (the ones that read
//! the pad and post events to the move system, the Medusa beam) and does the one-shot work of the others at the moment they fire (pay the cost, spawn a blast, throw a rock).
//!
//! What is read from the game (HIGH, `docs/magic.md`): which class does what, the stage tables (costs, ranges, bolt counts, flags), the pad bits each native tests and the event
//! codes it posts (`0x1c` to `0x20`), and `SCR_AimDownUp`'s range. What is not known and is a stand-in here (LOW): the blast records (radius, damage and impulse of every
//! blast), the damage of the Wind shots and the Medusa beam, how bolts are aimed, flight times of rocks and projectiles. Every such number is a named constant below.
//! The module has no engine types: the front end draws [`MagicSys::visuals`].

use std::collections::HashMap;

use crate::combat::{HitReport, Meters, Victim};
use crate::moves::{ClipLen, Env, Fired, MoveSys, Pad, Tick};
use crate::UNITS_PER_METRE as U;

/// The magic ids the unlock codes of the cast branches use (`BRA_*Enter`).
pub const LIGHTNING: u8 = 1;
pub const ELECTRIC: u8 = 2;
pub const WIND: u8 = 3;
pub const EARTH: u8 = 6;
pub const MEDUSA: u8 = 16;

/// Event codes the natives post to the move system (`docs/magic.md` 6 to 8): the follow-up moves branch on them.
const EV_1C: u8 = 0x1c;
const EV_1D: u8 = 0x1d;
const EV_1E: u8 = 0x1e;
const EV_1F: u8 = 0x1f;
const EV_20: u8 = 0x20;

// ---- stage tables (RAM `ingame2`, `docs/magic.md`), row = the magic's upgrade level 0 to 2 ----

/// `Scr_Lightning`: the three stages read the same.
pub const LIGHTNING_COST: f32 = 33.0;
const LIGHTNING_DAMAGE: f32 = 5.0;
const LIGHTNING_COUNT: f32 = 10.0;
const LIGHTNING_RANGE_M: f32 = 5.0;
/// Full-charge multipliers for damage, count and range; the charge decays 1 per second to at most 10 presses of Circle.
const LIGHTNING_MULT: (f32, f32, f32) = (2.0, 2.0, 4.0);
const LIGHTNING_MAX_CHARGE: f32 = 10.0;

struct ElectricRow {
    cost: f32,
    lifetime: f32,
    pool: f32,
    max_bolts: usize,
    range_m: f32,
    damage: f32,
    hits_per_s: f32,
    flags: u8,
}
const ELECTRIC_ROWS: [ElectricRow; 3] = [
    ElectricRow { cost: 10.0, lifetime: 2.0, pool: 200.0, max_bolts: 3, range_m: 5.0, damage: 1.0, hits_per_s: 4.0, flags: 0 },
    ElectricRow { cost: 12.5, lifetime: 2.0, pool: 300.0, max_bolts: 4, range_m: 7.5, damage: 1.0, hits_per_s: 8.0, flags: 2 },
    ElectricRow { cost: 16.5, lifetime: 2.0, pool: 400.0, max_bolts: 5, range_m: 10.0, damage: 1.0, hits_per_s: 12.0, flags: 3 },
];
/// Core slots (`0x0032fec0`, four of 0x60 bytes); a cast with all four busy does nothing.
const ELECTRIC_SLOTS: usize = 4;

struct EarthRow {
    cost: f32,
    flags: u8,
    rocks: u32,
}
const EARTH_ROWS: [EarthRow; 3] = [EarthRow { cost: 20.0, flags: 0, rocks: 10 }, EarthRow { cost: 25.0, flags: 1, rocks: 6 }, EarthRow { cost: 30.0, flags: 3, rocks: 20 }];
/// Rain (row 2): charge decay, maximum, and how long the phase lasts charged and not (`docs/magic.md` 6).
const EARTH_RAIN_TIME: (f32, f32) = (3.0, 0.5);

struct WindRow {
    shot_cost: f32,
    charged_count: u32,
    charged_cost: f32,
    charge_time: f32,
    tornado_count: u32,
    tornado_fan_deg: f32,
    tornado_cost: f32,
    tempest_cost: f32,
    flags: u8,
}
const WIND_ROWS: [WindRow; 3] = [
    WindRow { shot_cost: 4.0, charged_count: 0, charged_cost: 0.0, charge_time: 0.0, tornado_count: 1, tornado_fan_deg: 10.0, tornado_cost: 0.0, tempest_cost: 0.0, flags: 0 },
    WindRow { shot_cost: 4.0, charged_count: 0, charged_cost: 0.0, charge_time: 0.0, tornado_count: 1, tornado_fan_deg: 15.0, tornado_cost: 20.0, tempest_cost: 0.0, flags: 2 },
    WindRow { shot_cost: 4.0, charged_count: 6, charged_cost: 25.0, charge_time: 1.0, tornado_count: 2, tornado_fan_deg: 15.0, tornado_cost: 20.0, tempest_cost: 50.0, flags: 7 },
];

struct MedusaRow {
    beam_drain: f32,
    flash_range_m: f32,
    petrify: f32,
    flash_cost: f32,
    bomb_cost: f32,
    nuke_hold: f32,
    nuke_cost: f32,
    flags: u8,
}
const MEDUSA_ROWS: [MedusaRow; 3] = [
    MedusaRow { beam_drain: 10.0, flash_range_m: 0.0, petrify: 0.0, flash_cost: 0.0, bomb_cost: 0.0, nuke_hold: 0.0, nuke_cost: 0.0, flags: 0 },
    MedusaRow { beam_drain: 10.0, flash_range_m: 12.0, petrify: 250.0, flash_cost: 25.0, bomb_cost: 0.0, nuke_hold: 0.0, nuke_cost: 0.0, flags: 1 },
    MedusaRow { beam_drain: 10.0, flash_range_m: 12.0, petrify: 500.0, flash_cost: 25.0, bomb_cost: 25.0, nuke_hold: 0.5, nuke_cost: 50.0, flags: 7 },
];

// ---- stand-ins (LOW): the blast records and projectile data are not decoded ----

const EARTH_TINT: [f32; 3] = [0.75, 0.55, 0.35];

/// A blast: radius in metres at its end, damage, how long it takes to grow, and the push it gives (units per second, away from the centre).
struct BlastSpec {
    radius_m: f32,
    damage: f32,
    grow: f32,
    push: f32,
    /// The colour the front end tints the blast with (the effect records are not decoded: brown for earth, blue for electric, green for Medusa).
    tint: [f32; 3],
    /// Which effect the front end draws for it.
    look: BlastLook,
    /// The particle effect (`go` name in the magic WADs) that goes off with it, "" for none, and the radius in units that effect covers at its own size (0: shown at scale 1).
    effect: &'static str,
    native: f32,
}

/// The look of a blast: the earth stomp model, the electric star of bolts, the Medusa bomb-hit sphere, or a plain ring.
#[derive(Clone, Copy, PartialEq)]
enum BlastLook {
    Plain,
    Earth,
    Electric,
    Medusa,
}
const EARTH_ENTER_BLAST: BlastSpec = BlastSpec { radius_m: 3.0, damage: 10.0, grow: 0.25, push: 90.0, tint: EARTH_TINT, look: BlastLook::Earth, effect: "goearthstomp", native: 256.0 };
const EARTH_ECHO_BLAST: BlastSpec = BlastSpec { radius_m: 4.5, damage: 12.0, grow: 0.3, push: 110.0, tint: EARTH_TINT, look: BlastLook::Earth, effect: "goearthstomp", native: 256.0 };
const EARTH_STOMP_BLAST: BlastSpec = BlastSpec { radius_m: 6.5, damage: 40.0, grow: 0.35, push: 160.0, tint: EARTH_TINT, look: BlastLook::Earth, effect: "goearthstomp", native: 256.0 };
const ROCK_BLAST: BlastSpec = BlastSpec { radius_m: 2.2, damage: 15.0, grow: 0.15, push: 80.0, tint: EARTH_TINT, look: BlastLook::Earth, effect: "goearthrockhit", native: 0.0 };
const ELECTRIC_EXPLODE: BlastSpec = BlastSpec { radius_m: 4.0, damage: 30.0, grow: 0.2, push: 100.0, tint: [0.5, 0.7, 1.0], look: BlastLook::Electric, effect: "goelectricexplode", native: 0.0 };
const MEDUSA_BOMB_BLAST: BlastSpec = BlastSpec { radius_m: 5.0, damage: 60.0, grow: 0.25, push: 140.0, tint: [0.5, 1.0, 0.5], look: BlastLook::Medusa, effect: "gomedusabombhit", native: 0.0 };
const MEDUSA_NUKE_BLAST: BlastSpec = BlastSpec { radius_m: 12.0, damage: 150.0, grow: 0.6, push: 220.0, tint: [0.5, 1.0, 0.5], look: BlastLook::Medusa, effect: "gomedusanuke", native: 0.0 };
/// Wind: shot speed (m/s), life, hit radius, damage; the tornado and the tempest damage everything inside them each second.
const GUST_SPEED_M: f32 = 30.0;
const GUST_LIFE: f32 = 0.6;
const GUST_DAMAGE: f32 = 12.0;
const TORNADO_SPEED_M: f32 = 4.0;
const TORNADO_LIFE: f32 = 3.0;
const TORNADO_RADIUS_M: f32 = 2.5;
const TORNADO_DPS: f32 = 10.0;
const TEMPEST_LIFE: f32 = 4.0;
const TEMPEST_RADIUS_M: f32 = 6.0;
const TEMPEST_DPS: f32 = 15.0;
/// Medusa: the beam's reach and width (m), how fast it petrifies a target (seconds in the beam) and its damage per second.
const BEAM_RANGE_M: f32 = 14.0;
const BEAM_WIDTH_M: f32 = 1.4;
const BEAM_PETRIFY_SECS: f32 = 0.6;
const BEAM_DPS: f32 = 8.0;
const FLASH_CONE_COS: f32 = 0.7;
const BOMB_SPEED_M: f32 = 18.0;
const BOMB_LIFE: f32 = 1.4;
/// A rock's flight time (s) and the ranges the rock trigger draws its landing distance from (m).
const ROCK_FLIGHT: f32 = 0.8;
const ROCK_RANGE_M: (f32, f32) = (4.0, 10.0);

// ---- what the front end gets ----

/// Everything a front end needs to draw the magic: position, direction, age and life, and a size.
#[derive(Debug, Clone, PartialEq)]
pub struct Visual {
    pub kind: VisualKind,
    pub pos: [f32; 3],
    /// A second point for bolts and the beam (its far end), the direction for the rest.
    pub to: [f32; 3],
    pub age: f32,
    pub life: f32,
    pub radius: f32,
    /// Tint for blasts.
    pub tint: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualKind {
    /// A jagged bolt from `pos` to `to`: lightning strikes and the electric core's arcs.
    Bolt,
    /// The electric core's glow.
    ElectricCore,
    /// An expanding ring or sphere: a plain blast; `radius` is the current radius in units and `to[0]` the radius it ends at.
    Blast,
    /// The earth blasts (`earthStomp` model); radius fields as for `Blast`.
    EarthBlast,
    /// The electric explosion (a star of bolts); radius fields as for `Blast`.
    ElectricBlast,
    /// The Medusa bomb and nuke blasts (`medusaBombHit` model); radius fields as for `Blast`.
    MedusaBlast,
    Rock,
    Gust,
    Tornado,
    Tempest,
    MedusaBeam,
    MedusaBomb,
    /// The petrifying flash: a cone from `pos` along `to - pos`, `radius` long (its range).
    MedusaFlash,
}

/// A particle effect to start once: `name` is a `go` node of the magic WADs (`gow2-fx`), placed at `pos`; effects that shoot along their own axis (the Medusa flash) use `dir`,
/// the others stand upright. `scale` multiplies the effect's own size.
#[derive(Debug, Clone, PartialEq)]
pub struct FxEvent {
    pub name: &'static str,
    pub pos: [f32; 3],
    pub dir: [f32; 3],
    pub scale: f32,
}

/// A particle effect that follows a moving thing while it exists: `key` identifies it from frame to frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FxFollow {
    pub key: u64,
    pub name: &'static str,
    pub pos: [f32; 3],
    pub dir: [f32; 3],
    pub scale: f32,
}

/// What one update did.
#[derive(Debug, Default)]
pub struct Out {
    pub hits: Vec<HitReport>,
    /// A move started because a script posted an event.
    pub started: Option<usize>,
    /// Camera shake strength (0..1) and rumble requests this tick.
    pub shake: f32,
}

/// What the magic needs from the character each tick.
pub struct Ctx<'a> {
    pub dt: f32,
    pub pad: Pad,
    /// Feet position, the horizontal facing, the chest height point, and the world direction of the aim.
    pub pos: [f32; 3],
    pub facing: [f32; 2],
    pub chest: [f32; 3],
    pub aim_dir: [f32; 3],
    pub grounded: bool,
    /// Script class hash to name (`Dc::names`).
    pub names: &'a HashMap<u32, String>,
}

#[derive(Clone, Debug)]
struct Script {
    class: String,
    mv: usize,
    /// The action's first 0x20 bytes: fixed parameters at `+0x14..`.
    raw: [u8; 0x20],
    age: f32,
}

struct Core {
    id: u32,
    pos: [f32; 3],
    age: f32,
    pool: f32,
    timer: f32,
}

struct Rock {
    from: [f32; 3],
    to: [f32; 3],
    age: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum ShotKind {
    Gust,
    Tornado,
    Tempest,
    Bomb,
}

struct Shot {
    id: u32,
    kind: ShotKind,
    pos: [f32; 3],
    dir: [f32; 3],
    age: f32,
    hit: Vec<u32>,
}

struct Bolt {
    from: [f32; 3],
    to: [f32; 3],
    age: f32,
}

/// The Medusa flash: where it went off, along which direction, how far it reaches.
struct Flash {
    pos: [f32; 3],
    dir: [f32; 3],
    range: f32,
    age: f32,
}
const FLASH_LIFE: f32 = 0.5;

struct Blast {
    centre: [f32; 3],
    spec: &'static BlastSpec,
    age: f32,
    done: Vec<u32>,
    stomp_like: bool,
}

pub struct MagicSys {
    /// The upgrade level row used for every magic (0 to 2).
    pub level: usize,
    scripts: Vec<Script>,
    /// A cast is under way (its cost has been paid) until no magic move has run for a moment.
    cast: bool,
    idle_for: f32,
    charge: f32,
    hold: f32,
    wind_mode: u8,
    strike_clock: f32,
    cores: Vec<Core>,
    rocks: Vec<Rock>,
    shots: Vec<Shot>,
    bolts: Vec<Bolt>,
    blasts: Vec<Blast>,
    flashes: Vec<Flash>,
    serial: u32,
    fx_events: Vec<FxEvent>,
    rng: u32,
    // outputs the front end reads between updates
    pub head_visible: bool,
    pub bow_visible: bool,
    pub beam: Option<([f32; 3], [f32; 3])>,
    /// The aim range of the running `SCR_AimDownUp` in degrees (down, up), `None` when no aim script runs.
    pub aim_range: Option<(f32, f32)>,
    time: f32,
}

fn len(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add_scaled(a: [f32; 3], d: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] + d[0] * s, a[1] + d[1] * s, a[2] + d[2] * s]
}
fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = len(v).max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn centre(v: &Victim) -> [f32; 3] {
    [v.pos[0], v.pos[1] + v.height * 0.5, v.pos[2]]
}
fn raw_f32(raw: &[u8; 0x20], o: usize) -> f32 {
    f32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]])
}

/// Meter needed to start a cast of this magic (the check `FUN_00210728` does at the entry branch).
pub fn cast_cost(magic: u8, level: usize) -> f32 {
    let l = level.min(2);
    match magic {
        LIGHTNING => LIGHTNING_COST,
        ELECTRIC => ELECTRIC_ROWS[l].cost,
        EARTH => EARTH_ROWS[l].cost,
        WIND => WIND_ROWS[l].shot_cost,
        MEDUSA => 0.01,
        _ => 0.0,
    }
}

impl MagicSys {
    pub fn new(level: usize) -> Self {
        MagicSys {
            level: level.min(2),
            scripts: Vec::new(),
            cast: false,
            idle_for: 9.0,
            charge: 0.0,
            hold: 0.0,
            wind_mode: 0,
            strike_clock: 0.0,
            cores: Vec::new(),
            rocks: Vec::new(),
            shots: Vec::new(),
            bolts: Vec::new(),
            blasts: Vec::new(),
            flashes: Vec::new(),
            serial: 0,
            fx_events: Vec::new(),
            rng: 0x2545_f491,
            head_visible: false,
            bow_visible: false,
            beam: None,
            aim_range: None,
            time: 0.0,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    fn pay(meters: &mut Meters, cost: f32) -> bool {
        if meters.magic + 1e-3 >= cost {
            meters.adjust(1, -cost, false);
            true
        } else {
            false
        }
    }

    /// Whether a script of this class runs now.
    fn running(&self, class: &str) -> bool {
        self.scripts.iter().any(|s| s.class == class)
    }

    /// Stops everything that a cast leaves in the world only while the player is casting (the beam, the aim). Live projectiles, cores and blasts carry on.
    pub fn reset_cast(&mut self) {
        self.scripts.clear();
        self.beam = None;
        self.head_visible = false;
        self.bow_visible = false;
        self.aim_range = None;
    }

    /// One tick. `tick` is what the move system returned this tick (its fired actions start scripts); the current move decides which scripts live on.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, ctx: &Ctx, sys: &mut MoveSys, env: &Env, clip_len: ClipLen, meters: &mut Meters, victims: &mut [Victim], tick: &Tick) -> Out {
        let mut out = Out::default();
        self.time += ctx.dt;
        let cur = sys.cur.as_ref().map(|i| i.mv);
        // scripts end with their move
        self.scripts.retain(|s| Some(s.mv) == cur);
        // new scripts from the actions that fired
        for f in &tick.fired {
            self.start_script(f, ctx, sys, env, clip_len, meters, victims, &mut out);
        }
        // the cast ends a moment after the last magic move
        let magic_now = cur.is_some_and(|m| sys.set.moves[m].anim.starts_with("mag"));
        self.idle_for = if magic_now { 0.0 } else { self.idle_for + ctx.dt };
        if self.idle_for > 0.4 {
            self.cast = false;
            self.charge = 0.0;
            self.hold = 0.0;
        }
        // per-frame scripts
        self.head_visible = false;
        self.bow_visible = false;
        self.beam = None;
        self.aim_range = None;
        let scripts = self.scripts.clone();
        for s in &scripts {
            self.run_script(s, ctx, sys, env, clip_len, meters, victims, &mut out);
        }
        for s in &mut self.scripts {
            s.age += ctx.dt;
        }
        // the Exit moves hide the head and the bow after their window
        if let Some((m, i)) = sys.current() {
            if m.name.ends_with("Exit") && i.t() > 0.45 {
                self.head_visible = false;
            }
            if m.name.ends_with("Exit") && i.t() > 0.15 {
                self.bow_visible = false;
            }
        }
        self.step_world(ctx, victims, &mut out);
        out
    }

    fn name_of(&self, ctx: &Ctx, sys: &MoveSys, f: &Fired) -> Option<(String, [u8; 0x20])> {
        let a = &sys.set.moves[f.mv].actions[f.action];
        if a.kind != 0x0d {
            return None;
        }
        ctx.names.get(&a.u32_at(8)).map(|n| (n.clone(), a.raw))
    }

    #[allow(clippy::too_many_arguments)]
    fn start_script(&mut self, f: &Fired, ctx: &Ctx, sys: &mut MoveSys, env: &Env, clip_len: ClipLen, meters: &mut Meters, victims: &mut [Victim], out: &mut Out) {
        let Some((class, raw)) = self.name_of(ctx, sys, f) else { return };
        if std::env::var_os("GOW_LOGMAGIC").is_some() {
            println!("magic: script {class} started by {}", sys.set.moves[f.mv].name);
        }
        if sys.cur.as_ref().map(|i| i.mv) != Some(f.mv) {
            return;
        }
        let l = self.level;
        let (facing3, aim) = ([ctx.facing[0], 0.0, ctx.facing[1]], ctx.aim_dir);
        match class.as_str() {
            // ---- one-shot natives ----
            "SCR_Lightning" => {
                if !self.cast {
                    self.cast = true;
                    self.charge = 0.0;
                    Self::pay(meters, LIGHTNING_COST);
                }
            }
            "SCR_ElectricCore" => {
                let row = &ELECTRIC_ROWS[l];
                if self.cores.len() < ELECTRIC_SLOTS && Self::pay(meters, row.cost) {
                    let p = add_scaled(ctx.chest, facing3, 14.0);
                    self.serial += 1;
                    self.cores.push(Core { id: self.serial, pos: p, age: 0.0, pool: row.pool, timer: 0.0 });
                }
            }
            "SCR_EarthTrigger" => {
                if !self.cast {
                    self.cast = true;
                    let row = &EARTH_ROWS[l];
                    Self::pay(meters, row.cost);
                    if row.flags & 3 == 3 {
                        out.started = out.started.or(sys.post_event(EV_1E, env, clip_len).started);
                    } else if row.flags & 1 != 0 {
                        out.started = out.started.or(sys.post_event(EV_1C, env, clip_len).started);
                    }
                }
            }
            "SCR_EarthEnter" => self.blast(ctx.pos, &EARTH_ENTER_BLAST, false),
            "SCR_EarthEcho" => self.blast(ctx.pos, &EARTH_ECHO_BLAST, false),
            "SCR_EarthStomp" => {
                self.blast(ctx.pos, &EARTH_STOMP_BLAST, true);
                out.shake = out.shake.max(0.8);
            }
            "SCR_EarthRockTrigger" => {
                let d = ROCK_RANGE_M.0 + (ROCK_RANGE_M.1 - ROCK_RANGE_M.0) * self.rand();
                let ang = (self.rand() - 0.5) * 100.0f32.to_radians();
                let (s, c) = ang.sin_cos();
                let dir = [ctx.facing[0] * c - ctx.facing[1] * s, ctx.facing[0] * s + ctx.facing[1] * c];
                let to = [ctx.pos[0] + dir[0] * d * U, ctx.pos[1], ctx.pos[2] + dir[1] * d * U];
                let from = [ctx.pos[0], ctx.pos[1] + 40.0, ctx.pos[2]];
                self.rocks.push(Rock { from, to, age: 0.0 });
            }
            "SCR_WindThrow" => self.wind_throw(ctx, meters),
            "SCR_WindBlowHit" => {
                // the blow of the tornado and tempest moves: a push in front of Kratos
                let at = add_scaled(ctx.pos, facing3, 3.0 * U);
                self.blast(at, &BlastSpec { radius_m: 3.0, damage: 6.0, grow: 0.15, push: 120.0, tint: [0.9, 0.95, 1.0], look: BlastLook::Plain, effect: "gowindblowhit", native: 0.0 }, false);
            }
            "SCR_MedusaHeadFlash" => {
                let row = &MEDUSA_ROWS[l];
                if row.flags & 1 != 0 && Self::pay(meters, row.flash_cost) {
                    let range = row.flash_range_m * U;
                    for v in victims.iter_mut().filter(|v| v.alive()) {
                        let d = sub(centre(v), ctx.chest);
                        let dist = len(d);
                        if dist <= range && dot(norm(d), norm(aim)) >= FLASH_CONE_COS {
                            v.petrify = row.petrify.max(1.0);
                            v.vel = [0.0; 2];
                            out.hits.push(HitReport { victim: v.id, damage: 0.0, lethal: false, knock: [0.0; 3], flags: 0, window: "MedusaFlash".into() });
                        }
                    }
                    self.fx_events.push(FxEvent { name: "gomedusaflash", pos: ctx.chest, dir: norm(aim), scale: 1.0 });
                    self.flashes.push(Flash { pos: ctx.chest, dir: norm(aim), range, age: 0.0 });
                    out.shake = out.shake.max(0.5);
                }
            }
            "SCR_MedusaHeadBomb" => {
                if Self::pay(meters, MEDUSA_ROWS[l].bomb_cost) {
                    self.serial += 1;
                    self.shots.push(Shot { id: self.serial, kind: ShotKind::Bomb, pos: ctx.chest, dir: norm(aim), age: 0.0, hit: Vec::new() });
                }
            }
            "SCR_MedusaHeadNuke" => {
                if Self::pay(meters, MEDUSA_ROWS[l].nuke_cost) {
                    self.blast(ctx.pos, &MEDUSA_NUKE_BLAST, true);
                    out.shake = out.shake.max(1.0);
                }
            }
            _ => {}
        }
        // every script object lives until its move ends, even the one-shot ones (they do nothing more)
        self.scripts.push(Script { class, mv: f.mv, raw, age: 0.0 });
        let _ = victims;
    }

    fn blast(&mut self, centre: [f32; 3], spec: &'static BlastSpec, stomp_like: bool) {
        if !spec.effect.is_empty() {
            let scale = if spec.native > 0.0 { spec.radius_m * U / spec.native } else { 1.0 };
            self.fx_events.push(FxEvent { name: spec.effect, pos: centre, dir: [0.0, 1.0, 0.0], scale });
        }
        self.blasts.push(Blast { centre, spec, age: 0.0, done: Vec::new(), stomp_like });
    }

    fn wind_throw(&mut self, ctx: &Ctx, meters: &mut Meters) {
        let row = &WIND_ROWS[self.level];
        let aim = norm(ctx.aim_dir);
        let origin = add_scaled(ctx.chest, aim, 10.0);
        match self.wind_mode {
            1 => {
                let (count, cost) = if self.hold >= row.charge_time && row.charged_count > 0 { (row.charged_count, row.charged_cost) } else { (1, row.shot_cost) };
                if Self::pay(meters, cost) {
                    for k in 0..count {
                        let off = if count == 1 { 0.0 } else { (k as f32 / (count - 1) as f32 - 0.5) * 30.0f32.to_radians() };
                        self.serial += 1;
                        self.shots.push(Shot { id: self.serial, kind: ShotKind::Gust, pos: origin, dir: rotate_y(aim, off), age: 0.0, hit: Vec::new() });
                    }
                }
            }
            2 => {
                if row.flags & 2 != 0 && Self::pay(meters, row.tornado_cost) {
                    let n = row.tornado_count;
                    for k in 0..n {
                        let off = if n == 1 { 0.0 } else { (k as f32 / (n - 1) as f32 - 0.5) * row.tornado_fan_deg.to_radians() };
                        let mut d = rotate_y(aim, off);
                        d[1] = 0.0;
                        self.serial += 1;
                        self.shots.push(Shot { id: self.serial, kind: ShotKind::Tornado, pos: [ctx.pos[0], ctx.pos[1], ctx.pos[2]], dir: norm(d), age: 0.0, hit: Vec::new() });
                    }
                }
            }
            3 => {
                if row.flags & 4 != 0 && Self::pay(meters, row.tempest_cost) {
                    let at = [ctx.pos[0] + aim[0] * 6.0 * U, ctx.pos[1], ctx.pos[2] + aim[2] * 6.0 * U];
                    self.serial += 1;
                    self.shots.push(Shot { id: self.serial, kind: ShotKind::Tempest, pos: at, dir: [0.0, 0.0, 0.0], age: 0.0, hit: Vec::new() });
                }
            }
            _ => {}
        }
        self.hold = 0.0;
    }

    #[allow(clippy::too_many_arguments)]
    fn run_script(&mut self, s: &Script, ctx: &Ctx, sys: &mut MoveSys, env: &Env, clip_len: ClipLen, meters: &mut Meters, victims: &mut [Victim], out: &mut Out) {
        let l = self.level;
        let dt = ctx.dt;
        let post = |sys: &mut MoveSys, code: u8, out: &mut Out| {
            let t = sys.post_event(code, env, clip_len);
            if t.started.is_some() {
                out.started = t.started;
            }
        };
        match s.class.as_str() {
            "SCR_AimDownUp" => self.aim_range = Some((raw_f32(&s.raw, 0x18), raw_f32(&s.raw, 0x1c))),
            "SCR_HideSubWeapon" | "SCR_ClearRadius" | "SCR_FirstTimeMsg" => {}
            // ---- Lightning ----
            "SCR_Lightning" => {
                if ctx.pad.pressed(5) {
                    self.charge = (self.charge + 1.0).min(LIGHTNING_MAX_CHARGE);
                }
                self.charge = (self.charge - dt).max(0.0);
                let c = self.charge / LIGHTNING_MAX_CHARGE;
                let dmg = (c * (LIGHTNING_MULT.0 - 1.0) + 1.0) * LIGHTNING_DAMAGE;
                let rate = (c * (LIGHTNING_MULT.1 - 1.0) + 1.0) * LIGHTNING_COUNT;
                let range = (c * (LIGHTNING_MULT.2 - 1.0) + 1.0) * LIGHTNING_RANGE_M * U;
                self.strike_clock += dt * rate;
                while self.strike_clock >= 1.0 {
                    self.strike_clock -= 1.0;
                    let near: Vec<usize> = (0..victims.len()).filter(|&i| victims[i].alive() && len(sub(centre(&victims[i]), ctx.pos)) <= range).collect();
                    if near.is_empty() {
                        let a = self.rand() * std::f32::consts::TAU;
                        let r = (0.3 + 0.7 * self.rand()) * range;
                        let to = [ctx.pos[0] + a.cos() * r, ctx.pos[1], ctx.pos[2] + a.sin() * r];
                        self.bolts.push(Bolt { from: [to[0], to[1] + 14.0 * U, to[2]], to, age: 0.0 });
                    } else {
                        let k = near[(self.rand() * near.len() as f32) as usize % near.len()];
                        let v = &mut victims[k];
                        let to = [v.pos[0], v.pos[1], v.pos[2]];
                        v.health = (v.health - dmg).max(0.0);
                        v.since_hit = 0.0;
                        let lethal = v.health <= 0.0;
                        if v.petrify > 0.0 {
                            v.health = 0.0;
                        }
                        out.hits.push(HitReport { victim: v.id, damage: dmg, lethal, knock: [0.0; 3], flags: 0, window: "Lightning".into() });
                        self.bolts.push(Bolt { from: [to[0], to[1] + 14.0 * U, to[2]], to, age: 0.0 });
                    }
                }
            }
            // ---- Wind ----
            "SCR_WindShowBow" => self.bow_visible = true,
            "SCR_WindTrigger" => {
                let row = &WIND_ROWS[l];
                let (sq, tri, cir) = (ctx.pad.held(7), ctx.pad.pressed(4), ctx.pad.pressed(5));
                let in_charge = sys.current().is_some_and(|(m, _)| m.name.contains("Charge"));
                if sq && row.flags & 1 != 0 {
                    self.hold += dt;
                }
                if in_charge && !sq {
                    // let go of Square: the charged shot
                    self.wind_mode = 1;
                    post(sys, EV_1D, out);
                } else if cir && row.flags & 4 != 0 && meters.magic >= row.tempest_cost {
                    self.wind_mode = 3;
                    post(sys, EV_1F, out);
                } else if tri && row.flags & 2 != 0 && meters.magic >= row.tornado_cost {
                    self.wind_mode = 2;
                    post(sys, EV_1E, out);
                } else if sq && row.flags & 1 != 0 && self.hold > 0.25 && !in_charge {
                    post(sys, EV_20, out);
                } else if ctx.pad.pressed(7) {
                    self.wind_mode = 1;
                    post(sys, EV_1C, out);
                }
            }
            "SCR_WindCharge" => self.hold += dt * 0.0,
            // ---- Earth ----
            "SCR_EarthRain" => {
                if ctx.pad.pressed(5) {
                    self.charge = (self.charge + 1.0).min(20.0);
                }
                self.charge = (self.charge - 5.0 * dt).max(0.0);
                let limit = if self.charge >= 1.0 { EARTH_RAIN_TIME.0 } else { EARTH_RAIN_TIME.1 + 2.0 };
                if s.age >= limit {
                    post(sys, EV_1D, out);
                }
            }
            // ---- Medusa ----
            "SCR_MedusaHeadShow" => self.head_visible = true,
            "SCR_MedusaHeadUpdate" => self.head_visible = true,
            "SCR_MedusaHeadEvts" => {
                let row = &MEDUSA_ROWS[l];
                let mut code = EV_1F;
                if ctx.pad.held(7) && meters.magic >= 0.01 {
                    code = EV_1C;
                }
                if row.flags & 1 != 0 && ctx.pad.pressed(4) && meters.magic >= row.flash_cost {
                    code = EV_1D;
                }
                if std::env::var_os("GOW_LOGMAGIC").is_some() && (ctx.pad.pressed(4) || ctx.pad.pressed(5)) {
                    println!("magic: medusa events: pressed 4 {} 5 {}, meter {:.1}, flags {}, code {:#x}", ctx.pad.pressed(4), ctx.pad.pressed(5), meters.magic, row.flags, code);
                }
                if row.flags & 4 != 0 && ctx.pad.pressed(5) && meters.magic >= row.bomb_cost {
                    code = EV_1E;
                }
                post(sys, code, out);
            }
            "SCR_MedusaHeadChrgEvts" => {
                let row = &MEDUSA_ROWS[l];
                self.hold += dt;
                if !ctx.pad.held(5) {
                    let nuke = row.flags & 2 != 0 && meters.magic >= row.nuke_cost && self.hold >= row.nuke_hold;
                    post(sys, if nuke { EV_1D } else { EV_1C }, out);
                    self.hold = 0.0;
                }
            }
            "SCR_MedusaHeadBeam" => {
                let row = &MEDUSA_ROWS[l];
                meters.adjust(1, -row.beam_drain * dt, false);
                let aim = norm(ctx.aim_dir);
                let end = add_scaled(ctx.chest, aim, BEAM_RANGE_M * U);
                self.beam = Some((ctx.chest, end));
                let half = BEAM_WIDTH_M * U * 0.5;
                for v in victims.iter_mut().filter(|v| v.alive() && v.petrify <= 0.0) {
                    // distance from the victim's centre to the ray
                    let d = sub(centre(v), ctx.chest);
                    let along = dot(d, aim);
                    if along < 0.0 || along > BEAM_RANGE_M * U {
                        continue;
                    }
                    let perp = len(sub(d, [aim[0] * along, aim[1] * along, aim[2] * along]));
                    if perp > half + v.radius {
                        continue;
                    }
                    v.health = (v.health - BEAM_DPS * dt).max(1.0);
                    v.beam_time += dt;
                    v.since_hit = 0.0;
                    if v.beam_time >= BEAM_PETRIFY_SECS {
                        v.petrify = MEDUSA_ROWS[l].petrify.max(250.0);
                        v.vel = [0.0; 2];
                    }
                }
                if meters.magic <= 0.0 {
                    post(sys, EV_1F, out);
                }
            }
            _ => {}
        }
        let _ = dt;
    }

    /// Moves everything that is in the world and applies its damage.
    fn step_world(&mut self, ctx: &Ctx, victims: &mut [Victim], out: &mut Out) {
        let dt = ctx.dt;
        let l = self.level;
        // electric cores
        let mut spawn_bolts: Vec<Bolt> = Vec::new();
        let mut explode: Vec<[f32; 3]> = Vec::new();
        let row = &ELECTRIC_ROWS[l];
        for c in &mut self.cores {
            c.age += dt;
            c.timer += dt;
            if c.timer >= 1.0 / row.hits_per_s {
                c.timer = 0.0;
                let mut near: Vec<(f32, usize)> = victims.iter().enumerate().filter(|(_, v)| v.alive()).map(|(i, v)| (len(sub(centre(v), c.pos)), i)).filter(|(d, _)| *d <= row.range_m * U).collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
                for (_, i) in near.into_iter().take(row.max_bolts) {
                    let v = &mut victims[i];
                    v.health = (v.health - row.damage).max(0.0);
                    v.since_hit = 0.0;
                    c.pool -= row.damage;
                    if v.petrify > 0.0 {
                        v.health = 0.0;
                    }
                    out.hits.push(HitReport { victim: v.id, damage: row.damage, lethal: v.health <= 0.0, knock: [0.0; 3], flags: 0, window: "Electric".into() });
                    spawn_bolts.push(Bolt { from: c.pos, to: centre(v), age: 0.0 });
                }
            }
            if (c.age >= row.lifetime || c.pool <= 0.0) && row.flags & 1 != 0 {
                explode.push(c.pos);
            }
        }
        self.cores.retain(|c| c.age < row.lifetime && c.pool > 0.0);
        self.bolts.extend(spawn_bolts);
        for p in explode {
            self.blast(p, &ELECTRIC_EXPLODE, false);
        }
        // rocks
        let mut landed: Vec<[f32; 3]> = Vec::new();
        for r in &mut self.rocks {
            r.age += dt;
            if r.age >= ROCK_FLIGHT {
                landed.push(r.to);
            }
        }
        self.rocks.retain(|r| r.age < ROCK_FLIGHT);
        for p in landed {
            self.blast(p, &ROCK_BLAST, false);
        }
        // projectiles
        let mut bomb_hit: Vec<[f32; 3]> = Vec::new();
        for s in &mut self.shots {
            s.age += dt;
            match s.kind {
                ShotKind::Gust => {
                    s.pos = add_scaled(s.pos, s.dir, GUST_SPEED_M * U * dt);
                    for v in victims.iter_mut().filter(|v| v.alive()) {
                        if !s.hit.contains(&v.id) && len(sub(centre(v), s.pos)) <= v.radius + 1.2 * U {
                            let dmg = GUST_DAMAGE;
                            v.health = (v.health - dmg).max(0.0);
                            v.vel = [s.dir[0] * 140.0, s.dir[2] * 140.0];
                            v.since_hit = 0.0;
                            if v.petrify > 0.0 {
                                v.health = 0.0;
                            }
                            s.hit.push(v.id);
                            // the gust's hit effect (smoke puffs and a flash) at the victim
                            self.fx_events.push(FxEvent { name: "gowindgusthit", pos: centre(v), dir: s.dir, scale: 1.0 });
                            out.hits.push(HitReport { victim: v.id, damage: dmg, lethal: v.health <= 0.0, knock: [s.dir[0] * 140.0, 0.0, s.dir[2] * 140.0], flags: 0, window: "WindGust".into() });
                        }
                    }
                }
                ShotKind::Tornado => {
                    s.pos = add_scaled(s.pos, s.dir, TORNADO_SPEED_M * U * dt);
                    wind_field(victims, s.pos, TORNADO_RADIUS_M * U, TORNADO_DPS * dt, dt, (s.age % 0.25) < dt, out, "WindTornado");
                }
                ShotKind::Tempest => {
                    wind_field(victims, s.pos, TEMPEST_RADIUS_M * U, TEMPEST_DPS * dt, dt, (s.age % 0.25) < dt, out, "WindTempest");
                }
                ShotKind::Bomb => {
                    s.pos = add_scaled(s.pos, s.dir, BOMB_SPEED_M * U * dt);
                    let hit = victims.iter().any(|v| v.alive() && len(sub(centre(v), s.pos)) <= v.radius + 0.8 * U);
                    if hit || s.age >= BOMB_LIFE {
                        bomb_hit.push(s.pos);
                        s.age = 99.0;
                    }
                }
            }
        }
        self.shots.retain(|s| match s.kind {
            ShotKind::Gust => s.age < GUST_LIFE,
            ShotKind::Tornado => s.age < TORNADO_LIFE,
            ShotKind::Tempest => s.age < TEMPEST_LIFE,
            ShotKind::Bomb => s.age < 90.0,
        });
        for p in bomb_hit {
            self.blast(p, &MEDUSA_BOMB_BLAST, false);
            out.shake = out.shake.max(0.6);
        }
        // blasts
        for b in &mut self.blasts {
            b.age += dt;
            let r = (b.age / b.spec.grow).clamp(0.0, 1.0) * b.spec.radius_m * U;
            for v in victims.iter_mut().filter(|v| v.alive()) {
                let d = sub(centre(v), b.centre);
                if !b.done.contains(&v.id) && (d[0] * d[0] + d[2] * d[2]).sqrt() <= r + v.radius && d[1].abs() <= b.spec.radius_m * U {
                    let dmg = if v.petrify > 0.0 { v.health } else { b.spec.damage };
                    v.health = (v.health - dmg).max(0.0);
                    let h = (d[0] * d[0] + d[2] * d[2]).sqrt().max(1e-3);
                    v.vel = [d[0] / h * b.spec.push, d[2] / h * b.spec.push];
                    if b.stomp_like {
                        v.vy = 90.0;
                        v.airborne = true;
                    }
                    v.since_hit = 0.0;
                    b.done.push(v.id);
                    out.hits.push(HitReport { victim: v.id, damage: dmg, lethal: v.health <= 0.0, knock: [v.vel[0], v.vy, v.vel[1]], flags: 0, window: "Blast".into() });
                }
            }
        }
        self.blasts.retain(|b| b.age < b.spec.grow + 0.45);
        // bolts are short flashes
        for b in &mut self.bolts {
            b.age += dt;
        }
        self.bolts.retain(|b| b.age < 0.25);
        for f in &mut self.flashes {
            f.age += dt;
        }
        self.flashes.retain(|f| f.age < FLASH_LIFE);
    }

    /// Lightning is cast: its effects `golghtnshoulder`, `golghtnelbow` and `golghtnwrist` run on Kratos's arms (`SCR_Lightning` attaches six of them to joints found by name).
    pub fn body_fx_on(&self) -> bool {
        self.scripts.iter().any(|s| s.class == "SCR_Lightning")
    }

    /// The particle effects that started since the last call.
    pub fn take_fx(&mut self) -> Vec<FxEvent> {
        std::mem::take(&mut self.fx_events)
    }

    /// The particle effects that follow the moving magic this frame (the electric core, the Medusa bomb, the wind shots).
    pub fn following(&self) -> Vec<FxFollow> {
        let mut v = Vec::new();
        for c in &self.cores {
            v.push(FxFollow { key: 1 << 32 | c.id as u64, name: "goelectriccore", pos: c.pos, dir: [0.0, 1.0, 0.0], scale: 1.0 });
        }
        for s in &self.shots {
            let (name, scale) = match s.kind {
                ShotKind::Gust => ("gowindgust", 1.0),
                ShotKind::Tornado => ("gowindtornado", 1.0),
                ShotKind::Tempest => ("gowindtempest", TEMPEST_RADIUS_M * U / 338.0),
                ShotKind::Bomb => ("gomedusabomb", 1.0),
            };
            v.push(FxFollow { key: 2 << 32 | s.id as u64, name, pos: s.pos, dir: s.dir, scale });
        }
        v
    }

    /// Everything to draw this frame.
    pub fn visuals(&self) -> Vec<Visual> {
        let mut v = Vec::new();
        for b in &self.bolts {
            v.push(Visual { kind: VisualKind::Bolt, pos: b.from, to: b.to, age: b.age.max(0.0), life: 0.25, radius: 1.0, tint: [1.0; 3] });
        }
        for c in &self.cores {
            v.push(Visual { kind: VisualKind::ElectricCore, pos: c.pos, to: c.pos, age: c.age, life: ELECTRIC_ROWS[self.level].lifetime, radius: 5.0, tint: [1.0; 3] });
        }
        for r in &self.rocks {
            let t = (r.age / ROCK_FLIGHT).clamp(0.0, 1.0);
            let mut p = [r.from[0] + (r.to[0] - r.from[0]) * t, r.from[1] + (r.to[1] - r.from[1]) * t, r.from[2] + (r.to[2] - r.from[2]) * t];
            p[1] += 4.0 * 3.0 * U * t * (1.0 - t);
            v.push(Visual { kind: VisualKind::Rock, pos: p, to: p, age: r.age, life: ROCK_FLIGHT, radius: 1.0, tint: [1.0; 3] });
        }
        for s in &self.shots {
            let (kind, life, radius) = match s.kind {
                ShotKind::Gust => (VisualKind::Gust, GUST_LIFE, 1.2 * U),
                ShotKind::Tornado => (VisualKind::Tornado, TORNADO_LIFE, TORNADO_RADIUS_M * U),
                ShotKind::Tempest => (VisualKind::Tempest, TEMPEST_LIFE, TEMPEST_RADIUS_M * U),
                ShotKind::Bomb => (VisualKind::MedusaBomb, BOMB_LIFE, 0.8 * U),
            };
            v.push(Visual { kind, pos: s.pos, to: add_scaled(s.pos, s.dir, 1.0), age: s.age, life, radius, tint: [1.0; 3] });
        }
        for b in &self.blasts {
            let r = (b.age / b.spec.grow).clamp(0.0, 1.0) * b.spec.radius_m * U;
            let kind = match b.spec.look {
                BlastLook::Plain => VisualKind::Blast,
                BlastLook::Earth => VisualKind::EarthBlast,
                BlastLook::Electric => VisualKind::ElectricBlast,
                BlastLook::Medusa => VisualKind::MedusaBlast,
            };
            // `to[0]` carries the radius the blast ends at, so the front end can scale a model to it
            v.push(Visual { kind, pos: b.centre, to: [b.spec.radius_m * U, 0.0, b.spec.grow], age: b.age, life: b.spec.grow + 0.45, radius: r, tint: b.spec.tint });
        }
        for f in &self.flashes {
            v.push(Visual { kind: VisualKind::MedusaFlash, pos: f.pos, to: add_scaled(f.pos, f.dir, 1.0), age: f.age, life: FLASH_LIFE, radius: f.range, tint: [1.0; 3] });
        }
        if let Some((a, b)) = self.beam {
            v.push(Visual { kind: VisualKind::MedusaBeam, pos: a, to: b, age: self.time, life: 1.0, radius: BEAM_WIDTH_M * U * 0.5, tint: [1.0; 3] });
        }
        v
    }
}

fn rotate_y(v: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c]
}

/// A wind field (tornado, tempest): every victim inside takes damage and is pulled toward the centre while it lasts.
/// `report` is true once every quarter second: the hit reports (sounds, the hit counter) are not made every tick.
fn wind_field(victims: &mut [Victim], at: [f32; 3], radius: f32, damage: f32, dt: f32, report: bool, out: &mut Out, name: &str) {
    for v in victims.iter_mut().filter(|v| v.alive()) {
        let d = sub(centre(v), at);
        let h = (d[0] * d[0] + d[2] * d[2]).sqrt();
        if h <= radius + v.radius {
            v.health = (v.health - damage).max(0.0);
            if report {
                v.since_hit = 0.0;
            }
            if v.petrify > 0.0 {
                v.health = 0.0;
            }
            // a swirl: pulled in and lifted
            let k = 60.0 * dt;
            v.vel[0] += (-d[0] / h.max(1.0)) * 40.0 * dt + (-d[2] / h.max(1.0)) * k;
            v.vel[1] += (-d[2] / h.max(1.0)) * 40.0 * dt + (d[0] / h.max(1.0)) * k;
            v.vy = v.vy.max(30.0);
            v.airborne = true;
            if report && damage > 0.0 {
                out.hits.push(HitReport { victim: v.id, damage, lethal: v.health <= 0.0, knock: [0.0; 3], flags: 0, window: name.into() });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stage_tables_have_the_costs_of_the_ram_capture() {
        assert_eq!(cast_cost(LIGHTNING, 0), 33.0);
        assert_eq!(cast_cost(ELECTRIC, 2), 16.5);
        assert_eq!(cast_cost(EARTH, 1), 25.0);
        assert_eq!(cast_cost(WIND, 0), 4.0);
    }

    #[test]
    fn a_blast_hits_a_victim_inside_its_radius_once() {
        let mut m = MagicSys::new(2);
        m.blast([0.0; 3], &EARTH_STOMP_BLAST, true);
        let mut victims = vec![Victim::new(1, [60.0, 0.0, 0.0], 100.0, 100.0), Victim::new(2, [900.0, 0.0, 0.0], 100.0, 100.0)];
        let names = HashMap::new();
        let ctx = Ctx { dt: 0.1, pad: Pad::default(), pos: [0.0; 3], facing: [0.0, -1.0], chest: [0.0, 22.0, 0.0], aim_dir: [0.0, 0.0, -1.0], grounded: true, names: &names };
        let mut out = Out::default();
        for _ in 0..6 {
            m.step_world(&ctx, &mut victims, &mut out);
        }
        assert_eq!(out.hits.iter().filter(|h| h.victim == 1).count(), 1);
        assert!(victims[0].health < 100.0);
        assert_eq!(victims[1].health, 100.0);
    }

    #[test]
    fn each_blast_asks_for_its_own_look_and_carries_its_end_radius() {
        let mut m = MagicSys::new(2);
        m.blast([0.0; 3], &EARTH_STOMP_BLAST, true);
        m.blast([0.0; 3], &ELECTRIC_EXPLODE, false);
        m.blast([0.0; 3], &MEDUSA_NUKE_BLAST, true);
        let v = m.visuals();
        let kinds: Vec<VisualKind> = v.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, vec![VisualKind::EarthBlast, VisualKind::ElectricBlast, VisualKind::MedusaBlast]);
        assert_eq!(v[0].to[0], EARTH_STOMP_BLAST.radius_m * U);
        assert_eq!(v[2].to[0], MEDUSA_NUKE_BLAST.radius_m * U);
    }

    #[test]
    fn a_flash_is_drawn_for_half_a_second() {
        let mut m = MagicSys::new(2);
        m.flashes.push(Flash { pos: [0.0; 3], dir: [0.0, 0.0, -1.0], range: 240.0, age: 0.0 });
        assert_eq!(m.visuals().iter().filter(|x| x.kind == VisualKind::MedusaFlash).count(), 1);
        let mut victims: Vec<Victim> = Vec::new();
        let names = HashMap::new();
        let ctx = Ctx { dt: 0.1, pad: Pad::default(), pos: [0.0; 3], facing: [0.0, -1.0], chest: [0.0, 22.0, 0.0], aim_dir: [0.0, 0.0, -1.0], grounded: true, names: &names };
        let mut out = Out::default();
        for _ in 0..6 {
            m.step_world(&ctx, &mut victims, &mut out);
        }
        assert!(m.visuals().is_empty());
    }
}
