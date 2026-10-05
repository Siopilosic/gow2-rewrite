//! A player for the UI movies of [`crate::flp`]: the clip instance tree, the frame timelines and the action bytecode.
//!
//! The action bytecode is the SWF action set in a compact encoding (decoded from the game's interpreter and its action lists, `docs/hud.md`):
//! one opcode byte; `0x96` Push has a type byte (0 = string-pool offset u16, 1 = f32); `0x8b` SetTarget, `0x8c` GotoLabel and `0x81` GotoFrame take a
//! u16; `0x99` Jump and `0x9d` If take an s16 byte offset; `0x9f` GotoFrame2 takes a flag byte (bit 0 = play afterwards); `0x9e` Call pops a frame.
//! Variables are one flat table: the movie reads the values the game writes (`PS2_HealthMeter_Value`, `PS2_HitCounter_Event` ...).
//!
//! The player does not draw. [`Player::draw`] flattens the tree into shapes with their placement matrices and colour transforms, which a renderer
//! turns into meshes using [`crate::flp::ShapeMesh`].

use std::collections::HashMap;

use crate::flp::{Clip, Flp, Key};

/// The movie's frame rate (the stage record stores 30).
pub const FPS: f32 = 30.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Num(f64),
    Str(String),
}

impl Val {
    fn num(&self) -> f64 {
        match self {
            Val::Num(n) => *n,
            Val::Str(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        }
    }

    pub fn text(&self) -> String {
        match self {
            Val::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
            Val::Num(n) => format!("{n}"),
            Val::Str(s) => s.clone(),
        }
    }

    fn truthy(&self) -> bool {
        self.num() != 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClipId {
    Root,
    Clip(u16),
}

#[derive(Debug)]
struct Slot {
    ch: u16,
    name: u16,
    child: Option<Box<Node>>,
}

#[derive(Debug)]
struct Node {
    clip: ClipId,
    name: String,
    frame: u16,
    playing: bool,
    alpha: f32,
    visible: bool,
    pending: Option<u16>,
    slots: Vec<Option<Slot>>,
}

/// One shape placed on the stage.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawShape {
    /// Index into [`Flp::shapes`].
    pub shape: u16,
    /// Placement: `x' = a x + c y + tx`, `y' = b x + d y + ty` as `[a, b, c, d, tx, ty]` (twips).
    pub matrix: [f32; 6],
    /// Colour multiplier r, g, b, a (1.0 = unchanged).
    pub cx: [f32; 4],
}

/// A text field placed on the stage.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawText {
    /// Index into [`Flp::texts`].
    pub field: u16,
    pub matrix: [f32; 6],
    pub cx: [f32; 4],
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Frame {
    pub shapes: Vec<DrawShape>,
    pub texts: Vec<DrawText>,
}

pub struct Player {
    pub flp: Flp,
    root: Node,
    /// Variables by the instance they live on (the path of its node; `[]` is the root, where the engine writes its `PS2_*` variables) and their own name.
    vars: HashMap<(Vec<u16>, String), Val>,
    time_ms: f64,
    carry: f32,
    depth: u32,
}

fn mul(c: [f32; 6], p: [f32; 6]) -> [f32; 6] {
    [
        c[0] * p[0] + c[1] * p[2],
        c[0] * p[1] + c[1] * p[3],
        c[2] * p[0] + c[3] * p[2],
        c[2] * p[1] + c[3] * p[3],
        c[4] * p[0] + c[5] * p[2] + p[4],
        c[4] * p[1] + c[5] * p[3] + p[5],
    ]
}

impl Player {
    pub fn new(flp: Flp) -> Player {
        let nl = flp.root.layers.len();
        let root = Node { clip: ClipId::Root, name: String::new(), frame: 0, playing: true, alpha: 1.0, visible: true, pending: None, slots: (0..nl).map(|_| None).collect() };
        let mut p = Player { flp, root, vars: HashMap::new(), time_ms: 0.0, carry: 0.0, depth: 0 };
        p.enter(&[], 0);
        p
    }

    /// Sets every `PS2_*_Event` variable the movie mentions to -1 (no event), as the engine does before the first tick.
    pub fn clear_events(&mut self) {
        let names: Vec<String> = self
            .flp
            .strings
            .split(|&c| c == 0)
            .filter_map(|s| std::str::from_utf8(s).ok())
            .map(Self::var_name)
            .filter(|n| n.starts_with("PS2_") && n.ends_with("_Event"))
            .map(str::to_string)
            .collect();
        for n in names {
            self.vars.insert((Vec::new(), n), Val::Num(-1.0));
        }
    }

    pub fn set(&mut self, name: &str, v: Val) {
        self.vars.insert((Vec::new(), Self::var_name(name).to_string()), v);
    }

    pub fn set_num(&mut self, name: &str, v: f64) {
        self.set(name, Val::Num(v));
    }

    pub fn get(&self, name: &str) -> Option<&Val> {
        self.vars.get(&(Vec::new(), Self::var_name(name).to_string()))
    }

    pub fn get_num(&self, name: &str) -> f64 {
        self.get(name).map_or(0.0, Val::num)
    }

    /// Current frame and play state of the instance at `path` (instance names from the root, e.g. `"MainMeterT/MainMeter/HealthMeter"`).
    pub fn instance_frame(&self, path: &str) -> Option<(u16, bool)> {
        let p = self.resolve(&[], &format!("/{path}"))?;
        let n = self.node(&p)?;
        Some((n.frame, n.playing))
    }

    /// Runs the action list of the root frame with this label (the engine calls `SimKeyEvent` every tick to process the `*_Event` variables).
    pub fn call_root(&mut self, label: &str) {
        if let Some(f) = self.flp.root.label_frame(label) {
            self.run_frame(&[], f);
        }
    }

    /// Advances the movie by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        self.time_ms += dt as f64 * 1000.0;
        self.carry += dt * FPS;
        let steps = self.carry.floor() as u32;
        self.carry -= steps as f32;
        for _ in 0..steps.min(8) {
            let mut paths = Vec::new();
            self.collect(&mut Vec::new(), &mut paths);
            for p in paths {
                let Some(n) = self.node(&p) else { continue };
                if !n.playing {
                    continue;
                }
                let total = self.clip_of(n.clip).frames.max(1);
                let next = if n.frame + 1 >= total { 0 } else { n.frame + 1 };
                self.enter(&p, next);
            }
        }
    }

    // ---- tree access ----

    fn clip_of(&self, id: ClipId) -> &Clip {
        match id {
            ClipId::Root => &self.flp.root,
            ClipId::Clip(i) => &self.flp.clips[i as usize],
        }
    }

    fn node(&self, path: &[u16]) -> Option<&Node> {
        let mut n = &self.root;
        for &l in path {
            n = n.slots.get(l as usize)?.as_ref()?.child.as_deref()?;
        }
        Some(n)
    }

    fn node_mut(&mut self, path: &[u16]) -> Option<&mut Node> {
        let mut n = &mut self.root;
        for &l in path {
            n = n.slots.get_mut(l as usize)?.as_mut()?.child.as_deref_mut()?;
        }
        Some(n)
    }

    fn collect(&self, path: &mut Vec<u16>, out: &mut Vec<Vec<u16>>) {
        out.push(path.clone());
        let Some(n) = self.node(path) else { return };
        for (i, s) in n.slots.iter().enumerate() {
            if s.as_ref().is_some_and(|s| s.child.is_some()) {
                path.push(i as u16);
                self.collect(path, out);
                path.pop();
            }
        }
    }

    /// Resolves an instance path (`/A/B`, relative `A/B`, `..`) to a node path. `base` is the node that relative paths start from.
    fn resolve(&self, base: &[u16], p: &str) -> Option<Vec<u16>> {
        let p = p.split(':').next().unwrap_or("");
        let mut cur: Vec<u16> = if p.starts_with('/') { Vec::new() } else { base.to_vec() };
        for seg in p.split('/').filter(|s| !s.is_empty()) {
            if seg == ".." {
                cur.pop();
                continue;
            }
            let n = self.node(&cur)?;
            let idx = n.slots.iter().position(|s| s.as_ref().is_some_and(|s| s.child.as_ref().is_some_and(|c| c.name.eq_ignore_ascii_case(seg))))?;
            cur.push(idx as u16);
        }
        Some(cur)
    }

    // ---- timeline ----

    fn make_node(&self, ch: u16, name: u16) -> Option<Node> {
        let (t, id) = *self.flp.chars.get(ch as usize)?;
        if t != 7 {
            return None;
        }
        let nl = self.flp.clips.get(id as usize)?.layers.len();
        Some(Node {
            clip: ClipId::Clip(id),
            name: self.flp.string(name).unwrap_or("").to_string(),
            frame: 0,
            playing: true,
            alpha: 1.0,
            visible: true,
            pending: None,
            slots: (0..nl).map(|_| None).collect(),
        })
    }

    /// Shows `frame` of the node at `path`: updates its display list and runs the frame's actions.
    fn enter(&mut self, path: &[u16], frame: u16) {
        if self.depth > 24 {
            return;
        }
        self.depth += 1;
        let Some(clip_id) = self.node(path).map(|n| n.clip) else {
            self.depth -= 1;
            return;
        };
        let keys: Vec<Option<Key>> = {
            let clip = self.clip_of(clip_id);
            (0..clip.layers.len()).map(|l| clip.key_at(l, frame).copied().filter(|k| k.ch != 0)).collect()
        };
        // display list
        let mut fresh: Vec<u16> = Vec::new();
        for (l, key) in keys.iter().enumerate() {
            let same = match (key, self.node(path).and_then(|n| n.slots[l].as_ref())) {
                (Some(k), Some(s)) => s.ch == k.ch && s.name == k.name,
                (None, None) => true,
                _ => false,
            };
            if same {
                continue;
            }
            let slot = key.map(|k| Slot { ch: k.ch, name: k.name, child: self.make_node(k.ch, k.name).map(Box::new) });
            let has_child = slot.as_ref().is_some_and(|s| s.child.is_some());
            if let Some(n) = self.node_mut(path) {
                n.slots[l] = slot;
            }
            if has_child {
                fresh.push(l as u16);
            }
        }
        if let Some(n) = self.node_mut(path) {
            n.frame = frame;
        }
        // new children show their first frame (and run its actions) before this frame's own actions
        for l in fresh {
            let mut cp = path.to_vec();
            cp.push(l);
            self.enter(&cp, 0);
        }
        self.run_frame(path, frame);
        // a jump the actions asked for on this very node
        if let Some(t) = self.node_mut(path).and_then(|n| n.pending.take()) {
            self.enter(path, t);
        }
        self.depth -= 1;
    }

    fn run_frame(&mut self, path: &[u16], frame: u16) {
        let Some(n) = self.node(path) else { return };
        let lists: Vec<Vec<u8>> = self.clip_of(n.clip).frame_info.iter().filter(|f| f.frame == frame).flat_map(|f| f.actions.clone()).collect();
        for code in lists {
            self.run(path, &code);
        }
    }

    fn goto(&mut self, ctx: &[u16], target: &[u16], frame: u16, play: Option<bool>) {
        if let Some(n) = self.node_mut(target) {
            if let Some(p) = play {
                n.playing = p;
            }
        } else {
            return;
        }
        if target == ctx {
            if let Some(n) = self.node_mut(target) {
                n.pending = Some(frame);
            }
        } else {
            self.enter(target, frame);
        }
    }

    fn frame_of(&self, target: &[u16], v: &Val) -> Option<u16> {
        let n = self.node(target)?;
        let clip = self.clip_of(n.clip);
        match v {
            Val::Num(x) => Some((x.floor() as i64 - 1).max(0) as u16),
            Val::Str(s) => {
                let s = s.rsplit(':').next().unwrap_or("");
                clip.label_frame(s).or_else(|| s.trim().parse::<f64>().ok().map(|x| (x.floor() as i64 - 1).max(0) as u16))
            }
        }
    }

    // ---- the interpreter ----

    fn var_name(n: &str) -> &str {
        n.rsplit(':').next().unwrap_or(n)
    }

    /// The instance a variable lives on and its own name. `/A/B:x` is `x` of the instance `A/B` (`/:x` is the root's), `:x` and a bare `x` are the current target's own.
    fn var_key(&self, target: &[u16], name: &str) -> (Vec<u16>, String) {
        match name.rsplit_once(':') {
            Some((path, var)) => {
                let scope = if path.is_empty() { target.to_vec() } else { self.resolve(target, path).unwrap_or_else(|| vec![u16::MAX]) };
                (scope, var.to_string())
            }
            // the engine's own variables (`PS2_*`) are one set for the whole movie, whichever clip names them
            None if name.starts_with("PS2_") => (Vec::new(), name.to_string()),
            None => (target.to_vec(), name.to_string()),
        }
    }

    /// Reads a variable as the target sees it. A bare name that the target does not have falls back to the root's (the engine's `PS2_*` variables are read that way by text fields deep in the tree).
    fn lookup(&self, target: &[u16], name: &str) -> Option<&Val> {
        let (scope, var) = self.var_key(target, name);
        if let Some(v) = self.vars.get(&(scope.clone(), var.clone())) {
            return Some(v);
        }
        if !name.contains(':') && !scope.is_empty() {
            return self.vars.get(&(Vec::new(), var));
        }
        None
    }

    fn run(&mut self, ctx: &[u16], code: &[u8]) {
        let mut st: Vec<Val> = Vec::new();
        let mut target: Vec<u16> = ctx.to_vec();
        let mut pc = 0usize;
        let mut budget = 20_000;
        let rd16 = |p: usize| -> Option<u16> { Some(u16::from_le_bytes([*code.get(p)?, *code.get(p + 1)?])) };
        while pc < code.len() && budget > 0 {
            budget -= 1;
            let op = code[pc];
            pc += 1;
            macro_rules! pop {
                () => {
                    st.pop().unwrap_or(Val::Num(0.0))
                };
            }
            match op {
                0x00 => break,
                0x04 | 0x05 => {
                    if let Some(n) = self.node(&target) {
                        let total = self.clip_of(n.clip).frames.max(1);
                        let f = if op == 0x04 { (n.frame + 1).min(total - 1) } else { n.frame.saturating_sub(1) };
                        let t = target.clone();
                        self.goto(ctx, &t, f, Some(false));
                    }
                }
                0x06 | 0x07 => {
                    if let Some(n) = self.node_mut(&target) {
                        n.playing = op == 0x06;
                    }
                }
                0x0a => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Num(a.num() + b.num()));
                }
                0x0b => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Num(a.num() - b.num()));
                }
                0x0c => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Num(a.num() * b.num()));
                }
                0x0d => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Num(if b.num() == 0.0 { 0.0 } else { a.num() / b.num() }));
                }
                0x0e => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Num((a.num() == b.num()) as i32 as f64));
                }
                0x0f => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Num((a.num() < b.num()) as i32 as f64));
                }
                0x10 => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Num((a.truthy() && b.truthy()) as i32 as f64));
                }
                0x11 => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Num((a.truthy() || b.truthy()) as i32 as f64));
                }
                0x12 => {
                    let a = pop!();
                    st.push(Val::Num((!a.truthy()) as i32 as f64));
                }
                0x13 => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Num((a.text() == b.text()) as i32 as f64));
                }
                0x14 => {
                    let a = pop!();
                    st.push(Val::Num(a.text().chars().count() as f64));
                }
                0x17 => {
                    pop!();
                }
                0x18 => {
                    let a = pop!();
                    st.push(Val::Num(a.num().trunc()));
                }
                0x1c => {
                    let n = pop!().text();
                    let v = self.lookup(&target, &n).cloned().unwrap_or(Val::Str(String::new()));
                    st.push(v);
                }
                0x1d => {
                    let (v, n) = (pop!(), pop!().text());
                    let key = self.var_key(&target, &n);
                    self.vars.insert(key, v);
                }
                0x21 => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Str(a.text() + &b.text()));
                }
                0x22 => {
                    let (prop, path) = (pop!().num() as i32, pop!().text());
                    let node = self.resolve(&target, &path).and_then(|p| self.node(&p));
                    let v = node.map_or(0.0, |n| match prop {
                        4 => n.frame as f64 + 1.0,
                        5 => self.clip_of(n.clip).frames as f64,
                        6 => n.alpha as f64 * 100.0,
                        7 => n.visible as i32 as f64,
                        _ => 0.0,
                    });
                    st.push(Val::Num(v));
                }
                0x23 => {
                    let (v, prop, path) = (pop!().num(), pop!().num() as i32, pop!().text());
                    if std::env::var_os("FLP_DEBUG").is_some() {
                        eprintln!("SetProp {path:?} prop {prop} = {v} (resolves: {})", self.resolve(&target, &path).is_some());
                    }
                    if let Some(p) = self.resolve(&target, &path) {
                        if let Some(n) = self.node_mut(&p) {
                            match prop {
                                6 => n.alpha = (v / 100.0).clamp(0.0, 4.0) as f32,
                                7 => n.visible = v != 0.0,
                                _ => {}
                            }
                        }
                    }
                }
                // Trace (debug output), ToggleQuality and StopSounds do nothing here. The menus' "WAIT" branches use Trace, and an unknown opcode ends the action.
                0x26 => {
                    pop!();
                }
                0x08 | 0x09 => {}
                0x20 => {
                    let s = pop!().text();
                    target = if s.is_empty() { ctx.to_vec() } else { self.resolve(&target, &s).unwrap_or_else(|| vec![u16::MAX]) };
                }
                0x29 => {
                    let (b, a) = (pop!(), pop!());
                    st.push(Val::Num((a.text() < b.text()) as i32 as f64));
                }
                0x15 => {
                    let (count, start, s) = (pop!().num() as i64, pop!().num() as i64, pop!().text());
                    let from = (start - 1).max(0) as usize;
                    st.push(Val::Str(s.chars().skip(from).take(count.max(0) as usize).collect()));
                }
                0x30 => {
                    let a = pop!().num().max(1.0);
                    let r = ((self.time_ms as u64).wrapping_mul(2654435761) >> 8) as f64 % a;
                    st.push(Val::Num(r.floor()));
                }
                0x34 => st.push(Val::Num(self.time_ms.floor())),
                0x81 => {
                    let Some(f) = rd16(pc) else { break };
                    pc += 2;
                    let t = target.clone();
                    self.goto(ctx, &t, f, None);
                }
                0x8b | 0x8c => {
                    let Some(o) = rd16(pc) else { break };
                    pc += 2;
                    let s = self.flp.string(o).unwrap_or("").to_string();
                    if op == 0x8b {
                        target = if s.is_empty() { ctx.to_vec() } else { self.resolve(&target, &s).unwrap_or_else(|| vec![u16::MAX]) };
                    } else {
                        let t = target.clone();
                        if let Some(f) = self.node(&t).and_then(|n| self.clip_of(n.clip).label_frame(&s)) {
                            self.goto(ctx, &t, f, None);
                        }
                    }
                }
                0x96 => {
                    let Some(&t) = code.get(pc) else { break };
                    pc += 1;
                    match t {
                        0 => {
                            let Some(o) = rd16(pc) else { break };
                            pc += 2;
                            st.push(Val::Str(self.flp.string(o).unwrap_or("").to_string()));
                        }
                        1 => {
                            let Some(b) = code.get(pc..pc + 4) else { break };
                            pc += 4;
                            st.push(Val::Num(f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64));
                        }
                        _ => break,
                    }
                }
                0x99 | 0x9d => {
                    let Some(o) = rd16(pc) else { break };
                    pc += 2;
                    let off = o as i16 as isize;
                    let take = if op == 0x99 { true } else { pop!().truthy() };
                    if take {
                        pc = (pc as isize + off).max(0) as usize;
                    }
                }
                0x9e => {
                    let v = pop!().text();
                    let p = if v.contains(':') { self.resolve(&target, &v) } else { Some(target.clone()) };
                    if let Some(p) = p {
                        let label = v.rsplit(':').next().unwrap_or("").to_string();
                        let f = self.node(&p).and_then(|n| self.clip_of(n.clip).label_frame(&label));
                        if let Some(f) = f {
                            if self.depth < 20 {
                                self.depth += 1;
                                self.run_frame(&p, f);
                                self.depth -= 1;
                            }
                        }
                    }
                }
                0x9f => {
                    let Some(&flags) = code.get(pc) else { break };
                    pc += 1;
                    let v = pop!();
                    let t = target.clone();
                    if let Some(f) = self.frame_of(&t, &v) {
                        self.goto(ctx, &t, f, Some(flags & 1 != 0));
                    }
                }
                _ => break,
            }
        }
    }

    /// The instance tree as text (instance name, clip, frame, play state), for debugging.
    pub fn dump(&self, max_depth: usize) -> String {
        fn go(p: &Player, n: &Node, depth: usize, max: usize, out: &mut String) {
            out.push_str(&format!("{}{} {:?} frame {}/{} {}\n", "  ".repeat(depth), if n.name.is_empty() { "-" } else { &n.name }, n.clip, n.frame, p.clip_of(n.clip).frames, if n.playing { "play" } else { "stop" }));
            if depth >= max {
                return;
            }
            for s in n.slots.iter().flatten() {
                if let Some(c) = s.child.as_deref() {
                    go(p, c, depth + 1, max, out);
                }
            }
        }
        let mut s = String::new();
        go(self, &self.root, 0, max_depth, &mut s);
        s
    }

    // ---- drawing ----

    /// Flattens the instance tree into shapes and text fields in drawing order.
    pub fn draw(&self) -> Frame {
        let mut out = Frame::default();
        self.draw_node(&self.root, &mut Vec::new(), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [1.0; 4], &mut out);
        out
    }

    /// Draws only the instance at `path` (e.g. `"MainMeterT/MainMeter/HealthMeter"`), in its parent's coordinates (its own placement is applied
    /// by the caller's matrix, so the result is relative to the instance's origin).
    pub fn draw_instance(&self, path: &str) -> Option<Frame> {
        let p = self.resolve(&[], &format!("/{path}"))?;
        let n = self.node(&p)?;
        let mut out = Frame::default();
        self.draw_node(n, &mut p.clone(), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [1.0; 4], &mut out);
        Some(out)
    }

    /// Puts one more instance of a movie clip on the root timeline (a layer the movie does not have), named `name`, at the placement `matrix` (`a b c d` unitless, `tx ty` in
    /// twips), shown from frame 0 on. The engine uses it for pictures the movie has but never places itself (the selected magic's icon). `false` when the clip is not in the movie.
    pub fn add_overlay(&mut self, clip_id: u16, name: &str, matrix: [f32; 6]) -> bool {
        let Some(ch) = self.flp.chars.iter().position(|c| *c == (7, clip_id)) else { return false };
        let name_off = self.flp.strings.len() as u16;
        self.flp.strings.extend_from_slice(name.as_bytes());
        self.flp.strings.push(0);
        self.flp.matrices.push(matrix);
        let m = (self.flp.matrices.len() - 1) as u16;
        self.flp.root.layers.push(vec![Key { frame: 0, ch: ch as u16, matrix: m, cxform: 0, name: name_off }]);
        self.root.slots.push(None);
        let f = self.root.frame;
        self.enter(&[], f);
        true
    }

    /// Shows or hides the instance at `path`.
    pub fn set_visible(&mut self, path: &str, on: bool) {
        if let Some(p) = self.resolve(&[], &format!("/{path}")) {
            if let Some(n) = self.node_mut(&p) {
                n.visible = on;
            }
        }
    }

    /// Sends the instance at `path` to the frame with this label and plays it from there (the engine's `GotoAndPlay` on a menu clip, for example `PowerUpMenu` and `PowerStart`).
    pub fn play_label(&mut self, path: &str, label: &str) -> bool {
        let Some(p) = self.resolve(&[], &format!("/{path}")) else { return false };
        let Some(f) = self.node(&p).and_then(|n| self.clip_of(n.clip).label_frame(label)) else { return false };
        if let Some(n) = self.node_mut(&p) {
            n.playing = true;
        }
        self.enter(&p, f);
        true
    }

    /// Moves the instance at `path` to `frame` and stops it (a debugging and tooling aid).
    pub fn goto_instance(&mut self, path: &str, frame: u16) -> bool {
        let Some(p) = self.resolve(&[], &format!("/{path}")) else { return false };
        if let Some(n) = self.node_mut(&p) {
            n.playing = false;
        }
        self.enter(&p, frame);
        true
    }

    fn draw_node(&self, node: &Node, path: &mut Vec<u16>, m: [f32; 6], cx: [f32; 4], out: &mut Frame) {
        let clip = self.clip_of(node.clip);
        for (l, slot) in node.slots.iter().enumerate() {
            let Some(slot) = slot else { continue };
            let Some(key) = clip.key_at(l, node.frame) else { continue };
            if key.ch != slot.ch {
                continue;
            }
            let km = if key.matrix == 0 || key.matrix as usize >= self.flp.matrices.len() { m } else { mul(self.flp.matrix(key.matrix), m) };
            let kc = if key.cxform == 0 || key.cxform as usize >= self.flp.cxforms.len() {
                cx
            } else {
                let c = self.flp.cxforms[key.cxform as usize];
                [0, 1, 2, 3].map(|i| cx[i] * c[i] as f32 / 256.0)
            };
            let Some(&(t, id)) = self.flp.chars.get(slot.ch as usize) else { continue };
            if std::env::var_os("FLP_DEBUG").is_some() {
                eprintln!("draw layer {l} frame {} char ({t},{id}) child {:?}", node.frame, slot.child.as_ref().map(|c| (c.visible, c.alpha, c.slots.len())));
            }
            match t {
                1 => out.shapes.push(DrawShape { shape: id, matrix: km, cx: kc }),
                5 => {
                    let Some(f) = self.flp.texts.get(id as usize) else { continue };
                    let text = match self.flp.string(f.var) {
                        Some(v) if !v.is_empty() => self.lookup(path, v).map(Val::text).unwrap_or_default(),
                        _ => self.flp.string(f.text).unwrap_or("").to_string(),
                    };
                    out.texts.push(DrawText { field: id, matrix: km, cx: kc, text });
                }
                7 => {
                    if let Some(c) = slot.child.as_deref() {
                        if c.visible {
                            let mut kc = kc;
                            kc[3] *= c.alpha;
                            path.push(l as u16);
                            self.draw_node(c, path, km, kc, out);
                            path.pop();
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

