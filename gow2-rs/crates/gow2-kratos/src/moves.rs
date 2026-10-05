//! The move system: Kratos's data-driven move graph (`docs/kratos-data.md` 6.3 to 6.5, `docs/combat.md`).
//!
//! A *move* plays one animation clip and carries branches (which move comes next, on which input, in which window of the clip),
//! hit windows and actions. This module ports the game's evaluation:
//!
//! * `FUN_0024f250` per-frame evaluation of the current move's branches: window, input (`FUN_0024e210`), state (`FUN_0024eb00`),
//!   score (`FUN_0024f1e8`); the best score wins (ties at random) and is queued; unless it is immediate (`flagsA & 0x400`) it
//!   waits for the end of its window, and taps are buffered that way while holds and the `0x0b` "move ended" branch wait for
//!   the end of the window before they queue at all.
//! * `FUN_0024cb78` the dequeue: a due branch ends the move or starts its target (`FUN_002501d0`, `FUN_002470b8`); with no move
//!   active the entry branches of `CRT_Hero + 0x18` are evaluated (`FUN_0024f6c0`), and the lowest specificity value wins.
//! * `FUN_00247820` / `FUN_00247968` hit windows and the per-move list of hits already dealt.
//!
//! Locomotion (stand, walk, jump, fall) is not run through the graph here: "no move active" stands for those moves, which the game
//! also treats as layered under everything else (`MOV + 4 & 8`).

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

use gow2_formats::dc::{Branch, HitWindow, Move, MoveSet, BUTTON_BIT};

/// Pad word bits as the move data uses them (button codes 1 to 10 map through table `0x002f6da7`).
///
/// The face buttons are HIGH: Square is bit 7 (code 2, the `*Square01` moves), Triangle bit 4 (code 3), Circle bit 5 (code 4,
/// grabs and the `PB_CircleBtnSmash` prompt) and Cross bit 6 (code 1, jumps and evades). The shoulder bits are named by role from
/// the moves they start: bit 2 (code 5) blocks and parries, bit 0 (code 6) casts magic; bits 1 and 3 are not identified. Which
/// physical shoulder button each is has not been confirmed (LOW; the real game blocks with R1 and casts with L1).
pub mod pad {
    pub const MAGIC: u32 = 1 << 0;
    pub const BIT1: u32 = 1 << 1;
    pub const BLOCK: u32 = 1 << 2;
    pub const BIT3: u32 = 1 << 3;
    pub const TRIANGLE: u32 = 1 << 4;
    pub const CIRCLE: u32 = 1 << 5;
    pub const CROSS: u32 = 1 << 6;
    pub const SQUARE: u32 = 1 << 7;
}

/// A clip name that lives for the whole program, so the animation layers can keep `&'static str` names.
pub fn intern(s: &str) -> &'static str {
    static POOL: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut p = POOL.get_or_init(|| Mutex::new(HashSet::new())).lock().unwrap();
    if let Some(x) = p.get(s) {
        return x;
    }
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    p.insert(leaked);
    leaked
}

/// The buttons this frame and the frame before (input struct `+0x28` and `+0x2c`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pad {
    pub cur: u32,
    pub prev: u32,
}

impl Pad {
    pub fn pressed(&self, bit: u32) -> bool {
        self.cur & (self.cur ^ self.prev) & (1 << bit) != 0
    }
    pub fn released(&self, bit: u32) -> bool {
        self.prev & !self.cur & (1 << bit) != 0
    }
    pub fn held(&self, bit: u32) -> bool {
        self.cur & (1 << bit) != 0
    }
}

/// Sticks in the character's frame: `[right, forward]`, length up to 1.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sticks {
    pub left: [f32; 2],
    pub right: [f32; 2],
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Input {
    pub pad: Pad,
    pub sticks: Sticks,
}

/// The player's progress block (`0x00335834` and neighbours, `docs/kratos-data.md` 6.7).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    /// Unlock mask `0x00335834`.
    pub unlock_mask: u32,
    /// Level of the default blades `0x00335869` (5 in the RAM captures).
    pub base_level: i8,
    /// Selected magic `0x00335873` (0 = none).
    pub selected_magic: u8,
    /// The meter check for the selected magic passes.
    pub magic_ok: bool,
    /// Active sub-weapon: 0 Bone, 1 Hammer, 2 Olympus.
    pub sub_weapon: Option<u8>,
    pub sub_weapon_level: i8,
}

impl Default for Progress {
    fn default() -> Self {
        Progress { unlock_mask: 0, base_level: 5, selected_magic: 0, magic_ok: true, sub_weapon: None, sub_weapon_level: 1 }
    }
}

/// What the branch tests need to know about the character and its target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Env {
    /// `Character_StateMask`: 1 ground, 2 air, 4 rope, 8 wall, 0x10 water, 0x40 Pegasus, 0x80 ceiling, 0x100.
    pub state_mask: u32,
    pub health: f32,
    pub has_target: bool,
    pub target_health: Option<i32>,
    pub target_class: i8,
    pub god_mode: bool,
    pub progress: Progress,
}

impl Default for Env {
    fn default() -> Self {
        Env { state_mask: 1, health: 200.0, has_target: false, target_health: None, target_class: -1, god_mode: false, progress: Progress::default() }
    }
}

pub const STATE_GROUND: u32 = 1;
pub const STATE_AIR: u32 = 2;

/// Unlock or magic requirement of a branch (`FUN_0024e8d0`).
fn unlock_ok(code: u8, p: &Progress) -> bool {
    match code {
        0 => true,
        5 | 10 | 0x0b => p.unlock_mask & (1 << (code - 1)) != 0,
        7 => p.unlock_mask & 0x40 != 0,
        c => p.selected_magic == c && p.magic_ok,
    }
}

/// Sub-weapon gating of `flagsB & 0x3f00` (`FUN_0024ea30`, read from the decompilation, HIGH). The game compares the selected magic id
/// (`DAT_00335874`: `0xe`, `0xf` or `0x11`) and whether a sub-weapon is active. Each of the three ids has a pair of bits: the upper
/// one (`0x200`, `0x800`, `0x2000`) means the branch does not care, the lower one (`0x100`, `0x400`, `0x1000`) requires that id to be
/// the active one, and with neither the branch is refused while that id is active. A branch with no bits set is therefore one of the default
/// blades' moves (no sub-weapon active), and `0x2aaaa` (all three "do not care") one that works with anything. `active` is the
/// index 0, 1 or 2 of the active id.
fn sub_weapon_ok(flags_b: u32, active: Option<u8>) -> bool {
    (0..3u8).all(|i| {
        let (require, dont_care) = (flags_b & (0x100 << (2 * i)) != 0, flags_b & (0x200 << (2 * i)) != 0);
        let is_active = active == Some(i);
        dont_care || (if require { is_active } else { !is_active })
    })
}
/// `FUN_0024e210`: does the input satisfy the branch?
pub fn input_ok(b: &Branch, inp: &Input) -> bool {
    let btn = b.button;
    let bit = if (1..=10).contains(&btn) { BUTTON_BIT[btn as usize] } else { -1 };
    if bit < 0 {
        match btn {
            0 | 0x0d | 0x10 | 0x12 | 0x13 | 0x14 | 0x15 | 0x16 | 0x1c..=0x20 => return false,
            // events posted by the combat code, not input
            0x0e | 0x0f => return false,
            // an attack button was pressed (the stance ends when you attack)
            0x17 => {
                let p = inp.pad;
                if !(p.pressed(7) || p.pressed(4) || p.pressed(5)) {
                    return false;
                }
            }
            // the left stick is being used (a move can be walked out of)
            0x18 => {
                let l = inp.sticks.left;
                if l[0] * l[0] + l[1] * l[1] < 0.25 {
                    return false;
                }
            }
            0x19 | 0x1a | 0x1b => return false,
            _ => {}
        }
    } else {
        let bit = bit as u32;
        let ok = match b.press {
            1 => inp.pad.pressed(bit),
            2 => inp.pad.released(bit),
            3 | 4 => inp.pad.held(bit),
            5 => !inp.pad.held(bit),
            _ => true,
        };
        if !ok {
            return false;
        }
    }
    stick_ok(b.stick, &inp.sticks)
}

/// The stick condition `+0x22`: 1 and 2 magnitude, 7 to 10 left, right, forward, back with the stick at least 0.8 out. Codes
/// 0x0b to 0x14 are the same tests on the right stick. Quadrants (3 to 6) and the speed thresholds (0x15 to 0x17) are not ported.
fn stick_ok(code: u8, s: &Sticks) -> bool {
    if code == 0 {
        return true;
    }
    let (v, c) = if (0x0b..0x15).contains(&code) { (s.right, code - 10) } else { (s.left, code) };
    let mag = (v[0] * v[0] + v[1] * v[1]).sqrt();
    let n = if mag > 1e-6 { [v[0] / mag, v[1] / mag] } else { [0.0, 0.0] };
    match c {
        1 => mag < 0.8,
        2 => mag >= 0.8,
        7 => mag >= 0.8 && n[0] <= -0.707,
        8 => mag >= 0.8 && n[0] >= 0.707,
        9 => mag >= 0.8 && n[1] >= 0.707,
        10 => mag >= 0.8 && n[1] <= -0.707,
        _ => false,
    }
}

/// Where the branch is evaluated: the current move's normalised time and its internal flags.
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub t: f32,
    /// `moveSys + 0x2b4 & 2`, needed by `flagsA & 0x200` (not set anywhere in this port).
    pub flag2: bool,
}

/// `FUN_0024eb00`: the state test. Returns the specificity bits when the branch applies (bit 1 no unlock requirement, bit 2 and 3
/// target class wildcards, bit 4 target subclass wildcard).
pub fn state_ok(b: &Branch, event: Option<u8>, cur: Option<Cursor>, env: &Env) -> Option<u32> {
    if let Some(code) = event {
        if code != b.button || code == 0x0e || code == 0x0f {
            return None;
        }
    }
    if b.flags_a & env.state_mask == 0 {
        return None;
    }
    let unlocked = unlock_ok(b.unlock, &env.progress);
    if b.flags_a & 0x2000 == 0 {
        if !unlocked {
            return None;
        }
    } else if unlocked {
        return None;
    }
    if b.flags_a & 0x4000 != 0 && !env.has_target {
        return None;
    }
    if b.flags_a & 0x200 != 0 && !cur.map_or(false, |c| c.flag2) {
        return None;
    }
    let p = &env.progress;
    let level = if p.sub_weapon.is_some() { p.sub_weapon_level } else { p.base_level };
    if level < b.min_level || !sub_weapon_ok(b.flags_b, p.sub_weapon) {
        return None;
    }
    let fb = b.flags_b;
    // god mode (global 0x200): 0x20 either, 0x10 required, neither forbidden
    if fb & 0x20 == 0 && (fb & 0x10 != 0) != env.god_mode {
        return None;
    }
    // own flag 0x400000 (never set here): 0x80 either, 0x40 required, neither forbidden
    if fb & 0x80 == 0 && fb & 0x40 != 0 {
        return None;
    }
    // target object present: 0x2 either, 0x1 required, neither forbidden
    if fb & 2 == 0 && fb & 1 != 0 {
        return None;
    }
    // two camera bits and a target flag, never set here: a bit that requires them fails
    if fb & 0x20000 == 0 && fb & 0x10000 != 0 {
        return None;
    }
    if fb & 0x8000 == 0 && fb & 0x4000 != 0 {
        return None;
    }
    if fb & 8 == 0 && fb & 4 != 0 {
        return None;
    }
    if let Some(c) = cur {
        if c.t < b.win.0 || c.t > b.win.1 {
            return None;
        }
    }
    if let Some(h) = env.target_health {
        if (h as i32) < b.tgt_health.0 as i32 || (h as i32) > b.tgt_health.1 as i32 {
            return None;
        }
    }
    let own = env.health as i32;
    if own < b.own_health.0 as i32 || own > b.own_health.1 as i32 {
        return None;
    }
    if b.has_context {
        return None;
    }
    let mut score = 0;
    if b.unlock == 0 {
        score |= 2;
    }
    let (tc, bc) = (env.target_class, b.b1f);
    if tc == bc {
        if bc != -1 && b.flags_a & 0x1000 != 0 {
            return None;
        }
    } else if tc == -1 || bc == -1 {
        score |= 4;
    } else if b.flags_a & 0x1000 == 0 {
        return None;
    }
    if env.target_class != b.b1e {
        // the target subclass is unknown here (-1)
        if b.b1e != -1 {
            return None;
        }
        score |= 8;
    }
    Some(score)
}

/// `FUN_0024f1e8`: the priority of a branch when several apply (higher wins).
pub fn branch_score(b: &Branch, buffered: bool) -> i32 {
    let mut s = if b.button == 0x0b {
        1
    } else if b.press == 3 {
        if buffered { 2 } else { -1 }
    } else {
        3
    };
    if s != -1 {
        if b.flags_a & 0x800 != 0 {
            s = 5;
        } else if b.flags_a & 0x400 != 0 {
            s = 4;
        }
    }
    s
}

/// `FUN_00247820`: the sub-hit index (1 based) of a window at normalised time `t`, or 0 when `t` is outside it.
pub fn sub_hit_index(w: &HitWindow, t: f32) -> u32 {
    if t < w.win.0 || t > w.win.1 {
        return 0;
    }
    let n = (w.sub_hits.max(1)) as f32;
    let span = (w.win.1 - w.win.0).max(1e-6);
    (((n * (t - w.win.0) / span).floor() as u32) + 1).min(n as u32)
}

/// One running move.
#[derive(Debug, Clone)]
pub struct Instance {
    pub mv: usize,
    /// Seconds into the clip.
    pub clip_time: f32,
    pub duration: f32,
    /// Seconds the move has been running (for the blend-in).
    pub age: f32,
    fired: Vec<bool>,
    /// Hits dealt: (victim, window, sub-hit); at most 48 (`FUN_002477c8`).
    hits: Vec<(u32, u8, u8)>,
    queued: Option<(Branch, f32)>,
    hit_frame: Option<u32>,
    kill_frame: Option<u32>,
    block_frame: Option<u32>,
}

impl Instance {
    pub fn t(&self) -> f32 {
        (self.clip_time / self.duration.max(1e-6)).clamp(0.0, 1.0)
    }
}

/// An action that fired this tick.
#[derive(Debug, Clone, PartialEq)]
pub struct Fired {
    pub mv: usize,
    pub action: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Tick {
    /// A move started this tick.
    pub started: Option<usize>,
    /// The move ended (with no successor) this tick.
    pub ended: bool,
    pub fired: Vec<Fired>,
}

/// What the move system needs to know about the clips: the length in seconds, or `None` when the clip is missing.
pub type ClipLen<'a> = &'a dyn Fn(&str) -> Option<f32>;

pub struct MoveSys {
    pub set: Arc<MoveSet>,
    pub cur: Option<Instance>,
    frame: u32,
    rng: u32,
    /// Seconds of the move system's own time inside moves (`+0x54`).
    pub time_in_move: f32,
    /// The instance flag +0x2b4 & 2 that branches with flagsA 0x200 require. In the game the move's scripts and timer actions set it; the port has no scripts, so the
    /// caller sets it for the moves it knows how to end (the magic loops: released button or empty meter).
    pub flag2: bool,
}

impl MoveSys {
    pub fn new(set: Arc<MoveSet>) -> Self {
        MoveSys { set, cur: None, frame: 1, rng: 0x1234_5678, time_in_move: 0.0, flag2: false }
    }

    pub fn current(&self) -> Option<(&Move, &Instance)> {
        self.cur.as_ref().map(|i| (&self.set.moves[i.mv], i))
    }

    pub fn is_busy(&self) -> bool {
        self.cur.is_some()
    }

    fn rand(&mut self, n: usize) -> usize {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as usize) % n.max(1)
    }

    /// Starts a move (`FUN_002470b8`) at the given normalised time (the branch's start time).
    pub fn start(&mut self, mv: usize, start_t: f32, clip_len: ClipLen) -> Option<usize> {
        let m = &self.set.moves[mv];
        let dur = clip_len(&m.anim)?;
        self.cur = Some(Instance {
            mv,
            clip_time: start_t * dur,
            duration: dur,
            age: 0.0,
            fired: vec![false; m.actions.len()],
            hits: Vec::new(),
            queued: None,
            hit_frame: None,
            kill_frame: None,
            block_frame: None,
        });
        Some(mv)
    }

    /// Takes a branch (`FUN_002501d0`): flagsA `0x8000` only ends the move, `0x10000` does nothing, otherwise the target starts.
    /// A target whose clip is missing does nothing.
    fn take(&mut self, b: &Branch, clip_len: ClipLen, tick: &mut Tick) {
        if b.flags_a & 0x10000 != 0 {
            return;
        }
        if b.flags_a & 0x8000 != 0 {
            self.cur = None;
            tick.ended = true;
            return;
        }
        if b.flags_a & 0x20000 != 0 {
            // substitute decision branches are not ported
            return;
        }
        let Some(t) = b.target else { return };
        if clip_len(&self.set.moves[t].anim).is_none() {
            return;
        }
        if let Some(m) = self.start(t, b.start_time, clip_len) {
            tick.started = Some(m);
            tick.ended = false;
        }
    }

    /// Ends the current move without starting another (`FUN_002504f8`).
    pub fn cancel(&mut self) {
        self.cur = None;
    }

    /// Whether the current move is a combat stance (`MOV_CombatIdle`, `MOV_CombatIdleStep`, the Titan variant): a standing pose that
    /// loops until something else happens.
    pub fn in_stance(&self) -> bool {
        self.current().map_or(false, |(m, _)| m.name.contains("CombatIdle"))
    }

    /// A stand-in for the stance's exit. The data lets a stance end only on an attack button (branch code `0x17`); how the game gets
    /// from the stance back to walking and standing is not found in the move data, so walking out with the stick and a time limit
    /// end the stance here (LOW; the 4 s limit is a guess).
    pub fn leave_stance_when(&mut self, walking: bool, limit_secs: f32) -> bool {
        if self.in_stance() && (walking || self.time_in_move > limit_secs) {
            self.cur = None;
            return true;
        }
        false
    }

    /// Entry branches (`FUN_0024f6c0`): the ones whose input and state apply, lowest specificity value first, ties at random.
    fn pick_entry(&mut self, inp: &Input, env: &Env) -> Option<Branch> {
        let mut best = i32::MAX;
        let mut pool: Vec<usize> = Vec::new();
        for (i, b) in self.set.entry.iter().enumerate() {
            if b.target.is_none() || !input_ok(b, inp) {
                continue;
            }
            // entry branches have no window of their own to test against a move
            if let Some(s) = state_ok(b, None, None, env) {
                let s = s as i32;
                if s < best {
                    best = s;
                    pool = vec![i];
                } else if s == best {
                    pool.push(i);
                }
            }
        }
        if pool.is_empty() {
            return None;
        }
        let k = self.rand(pool.len());
        Some(self.set.entry[pool[k]].clone())
    }

    /// Evaluates the current move's branches (`FUN_0024f250`) and queues the winner.
    fn evaluate(&mut self, inp: &Input, env: &Env) {
        let Some(inst) = &self.cur else { return };
        let mv = &self.set.moves[inst.mv];
        let cursor = Cursor { t: inst.t(), flag2: self.flag2 };
        let (mut best, mut pool): (i32, Vec<usize>) = (0, Vec::new());
        for (i, b) in mv.branches.iter().enumerate() {
            if cursor.t < b.win.0 || cursor.t > b.win.1 {
                continue;
            }
            if input_ok(b, inp) && state_ok(b, None, Some(cursor), env).is_some() {
                let s = branch_score(b, false);
                if s > best {
                    best = s;
                    pool = vec![i];
                } else if s == best && s > 0 {
                    pool.push(i);
                }
            }
        }
        // a branch already queued competes with the winners
        if let Some((q, _)) = &inst.queued {
            if best < branch_score(q, true) {
                pool.clear();
            }
        }
        if pool.is_empty() {
            return;
        }
        let (mv_idx, t) = (inst.mv, cursor.t);
        let k = self.rand(pool.len());
        let b = self.set.moves[mv_idx].branches[pool[k]].clone();
        let cursor = Cursor { t, flag2: self.flag2 };
        let immediate = b.flags_a & 0x400 != 0;
        let mut when = 0.0;
        if !immediate {
            when = b.win.1;
            if cursor.t < when && (b.button == 0x0b || b.press == 3) {
                // holds and the "move ended" branch only queue once their window has run out
                return;
            }
        }
        if let Some(i) = &mut self.cur {
            i.queued = Some((b, when));
        }
    }

    /// One tick of `MoveSys_Update`. `dt` is in seconds, `clip_len` gives clip lengths.
    pub fn update(&mut self, dt: f32, inp: &Input, env: &Env, clip_len: ClipLen) -> Tick {
        let mut tick = Tick::default();
        self.frame += 1;
        if let Some(inst) = &mut self.cur {
            let rate = self.set.moves[inst.mv].rate;
            inst.clip_time += dt * if rate > 0.0 { rate } else { 1.0 };
            inst.age += dt;
            self.time_in_move += dt;
        } else {
            self.time_in_move = 0.0;
        }
        // actions
        if let Some(inst) = &mut self.cur {
            let t = inst.t();
            let frame = self.frame;
            let mv = &self.set.moves[inst.mv];
            for (k, a) in mv.actions.iter().enumerate() {
                if inst.fired[k] || t < a.win.0 || t > a.win.1 {
                    continue;
                }
                let go = match a.trigger {
                    0 => true,
                    1 => inst.hit_frame.map_or(false, |f| f + 1 == frame),
                    3 => inst.kill_frame.map_or(false, |f| f + 1 == frame),
                    5 => inst.block_frame.map_or(false, |f| f + 1 == frame),
                    _ => false,
                };
                if go {
                    inst.fired[k] = true;
                    tick.fired.push(Fired { mv: inst.mv, action: k });
                }
            }
        }
        if self.cur.is_some() {
            self.evaluate(inp, env);
            // the dequeue: a queued branch that is due, or the end of the clip
            let (due, ended) = {
                let i = self.cur.as_ref().unwrap();
                (i.queued.as_ref().map_or(false, |(_, w)| i.t() >= *w), i.clip_time >= i.duration)
            };
            if due {
                let (b, _) = self.cur.as_mut().unwrap().queued.take().unwrap();
                self.take(&b, clip_len, &mut tick);
            } else if ended {
                self.cur = None;
                tick.ended = true;
            }
        }
        // no move active: the entry branches
        if self.cur.is_none() {
            if let Some(b) = self.pick_entry(inp, env) {
                self.take(&b, clip_len, &mut tick);
            }
        }
        tick
    }

    /// The hit windows of the current move that are open now, with their sub-hit index.
    pub fn open_windows(&self) -> Vec<(usize, u32, &HitWindow)> {
        let Some((m, i)) = self.current() else { return Vec::new() };
        let t = i.t();
        m.hits.iter().enumerate().filter_map(|(k, w)| {
            let s = sub_hit_index(w, t);
            (s > 0).then_some((k, s, w))
        }).collect()
    }

    /// Whether this (victim, window, sub-hit) was already dealt by the current move.
    pub fn already_hit(&self, victim: u32, window: usize, sub: u32) -> bool {
        self.cur.as_ref().map_or(true, |i| i.hits.contains(&(victim, window as u8, sub as u8)))
    }

    /// Records a hit (`FUN_002477c8`) and stamps the frame for the "on hit" actions.
    pub fn register_hit(&mut self, victim: u32, window: usize, sub: u32, lethal: bool) {
        let frame = self.frame;
        if let Some(i) = &mut self.cur {
            if i.hits.len() < 48 {
                i.hits.push((victim, window as u8, sub as u8));
            }
            i.hit_frame = Some(frame);
            if lethal {
                i.kill_frame = Some(frame);
            }
        }
    }

    /// The attack was blocked (`FUN_00247b78`).
    pub fn register_blocked(&mut self) {
        let frame = self.frame;
        if let Some(i) = &mut self.cur {
            i.block_frame = Some(frame);
        }
    }

    /// Posts a combat event code as a pseudo-button (`MoveSys_PostEvent`, `docs/combat.md` section 7): the current move's branches and
    /// the entry branches are tried with that code; the branch with the lowest specificity value starts.
    pub fn post_event(&mut self, code: u8, env: &Env, clip_len: ClipLen) -> Tick {
        let mut tick = Tick::default();
        let cursor = self.cur.as_ref().map(|i| Cursor { t: i.t(), flag2: self.flag2 });
        let mut best = i32::MAX;
        let mut pool: Vec<Branch> = Vec::new();
        let mut consider = |b: &Branch, cur: Option<Cursor>| {
            if let Some(s) = state_ok(b, Some(code), cur, env) {
                let s = s as i32;
                if s < best {
                    best = s;
                    pool = vec![b.clone()];
                } else if s == best {
                    pool.push(b.clone());
                }
            }
        };
        if let Some(i) = &self.cur {
            for b in &self.set.moves[i.mv].branches {
                consider(b, cursor);
            }
        }
        for b in &self.set.entry {
            consider(b, None);
        }
        if !pool.is_empty() {
            let k = self.rand(pool.len());
            let b = pool[k].clone();
            self.take(&b, clip_len, &mut tick);
        }
        tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(button: u8, press: u8, stick: u8) -> Branch {
        Branch {
            name: "t".into(),
            target: None,
            flags_a: 1,
            flags_b: 0x2808a,
            win: (0.0, 1.0),
            start_time: 0.0,
            tgt_health: (0, 32767),
            own_health: (0, 32767),
            b1e: -1,
            b1f: -1,
            button,
            press,
            stick,
            unlock: 0,
            min_level: 0,
            has_context: false,
        }
    }

    #[test]
    fn a_just_pressed_button_needs_an_edge() {
        let p = Pad { cur: pad::SQUARE, prev: 0 };
        assert!(input_ok(&b(2, 1, 0), &Input { pad: p, ..Default::default() }));
        let held = Pad { cur: pad::SQUARE, prev: pad::SQUARE };
        assert!(!input_ok(&b(2, 1, 0), &Input { pad: held, ..Default::default() }));
        assert!(input_ok(&b(2, 4, 0), &Input { pad: held, ..Default::default() }));
        assert!(input_ok(&b(2, 5, 0), &Input::default()));
        assert!(!input_ok(&b(2, 5, 0), &Input { pad: held, ..Default::default() }));
    }

    #[test]
    fn the_unconditional_branch_always_passes_input() {
        assert!(input_ok(&b(0x0b, 1, 0), &Input::default()));
        // with a stick condition: the right stick toward the character's right
        let mut i = Input::default();
        i.sticks.right = [0.0, 1.0];
        assert!(input_ok(&b(0x0b, 1, 0x13), &i), "stick forward on the right stick is code 9 + 10");
        assert!(!input_ok(&b(0x0b, 1, 0x14), &i));
    }

    #[test]
    fn released_is_an_edge_downward() {
        let p = Pad { cur: 0, prev: pad::SQUARE };
        assert!(p.released(7));
        assert!(!Pad { cur: pad::SQUARE, prev: pad::SQUARE }.released(7));
        assert!(!Pad { cur: pad::SQUARE, prev: 0 }.released(7));
    }

    #[test]
    fn sub_weapon_gating_follows_the_data_pattern() {
        // default blades: no bits, only when nothing is equipped
        assert!(sub_weapon_ok(0x2808a, None));
        assert!(!sub_weapon_ok(0x2808a, Some(0)));
        // Bone moveset needs Bone active
        assert!(sub_weapon_ok(0x2a9aa, Some(0)));
        assert!(!sub_weapon_ok(0x2a9aa, None));
        assert!(!sub_weapon_ok(0x2a9aa, Some(1)));
        // an unconditional branch passes with nothing equipped
        assert!(sub_weapon_ok(0x2aaaa, None));
    }

    #[test]
    fn score_prefers_immediate_and_priority_branches_and_ranks_the_end_branch_last() {
        assert_eq!(branch_score(&b(2, 1, 0), false), 3);
        assert_eq!(branch_score(&b(0x0b, 1, 0), false), 1);
        let mut imm = b(2, 1, 0);
        imm.flags_a |= 0x400;
        assert_eq!(branch_score(&imm, false), 4);
        imm.flags_a |= 0x800;
        assert_eq!(branch_score(&imm, false), 5);
        assert_eq!(branch_score(&b(2, 3, 0), false), -1);
    }

    #[test]
    fn sub_hits_are_spread_over_the_window() {
        let w = HitWindow { name: "w".into(), win: (0.2, 0.4), ground: [0.0; 3], air: [0.0; 3], block: [0.0; 3], damage: 1.0, volume: 0x20, flags: 0, sub_hits: 4 };
        assert_eq!(sub_hit_index(&w, 0.1), 0);
        assert_eq!(sub_hit_index(&w, 0.2), 1);
        assert_eq!(sub_hit_index(&w, 0.31), 3);
        assert_eq!(sub_hit_index(&w, 0.4), 4);
        assert_eq!(sub_hit_index(&w, 0.41), 0);
    }
}


