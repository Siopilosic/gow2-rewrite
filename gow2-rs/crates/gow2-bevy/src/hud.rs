//! The in-game HUD: the game's own UI movie `FLP_HUDA` (`R_PERMA.WAD`), played by `gow2_formats::flp_play` and drawn with Bevy 2D meshes.
//!
//! What is read from the game: the movie (clip tree, timelines, placement, colour transforms, the action scripts that move the meter bars),
//! its shape model `MDL_HUDA_0` and the bitmaps `TXR_HUDA*`. What the port does is what the engine does (`docs/hud.md`): it writes the UI
//! variables (`PS2_HealthMeter_Value`, `PS2_MagicMeter_Value`, `PS2_PowerOrb_Count`, `PS2_HitCounter_Value`, the `*_Event` codes) and calls the
//! movie's `SimKeyEvent` frame action every tick, which moves the fills with `gotoAndStop(1 + value / 2)` and shows or hides the counters.
//!
//! The movie's stage is 640 x 480 px (12,800 x 9,600 twips); the HUD camera shows it scaled to the window height, anchored at the left edge.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    camera::{visibility::RenderLayers, ClearColorConfig, ScalingMode},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    window::PrimaryWindow,
};
use gow2_formats::{
    flp::{self, ShapeMesh},
    flp_play::{DrawShape, DrawText, Player},
    texture::TextureStore,
    wad,
};

/// The clip of `FLP_HUDA` that holds the item pictures (frames 2 Lightning, 3 Wind, 4 Electric, 5 Medusa, 6 Earth, 7 Hammer, 9 Olympus: its labels), and where the HUD puts
/// the selected magic's (stage pixels x, y of the circle's centre, and the scale).
pub const MAGIC_ICON_CLIP: u16 = 343;
pub const MAGIC_ICON_PLACE: (f32, f32, f32) = (50.0, 80.0, 1.0);

/// Virtual canvas height; widths follow the window's aspect ratio.
pub const CANVAS_HEIGHT: f32 = 480.0;
const HUD_LAYER: usize = 1;

/// The values the HUD shows.
#[derive(Debug, Clone, PartialEq)]
pub struct HudValues {
    pub health: f32,
    pub health_max: f32,
    pub magic: f32,
    pub magic_max: f32,
    pub god: f32,
    pub god_max: f32,
    pub orbs: u32,
    /// The combo counter and how long ago the last hit was (seconds).
    pub hits: u32,
    pub hit_age: f32,
    /// The last enemy hit: name, health, maximum.
    pub enemy: Option<(String, f32, f32)>,
}

impl Default for HudValues {
    fn default() -> Self {
        HudValues { health: 200.0, health_max: 200.0, magic: 200.0, magic_max: 200.0, god: 0.0, god_max: 100.0, orbs: 0, hits: 0, hit_age: 99.0, enemy: None }
    }
}

#[derive(Component)]
pub struct HudCamera;

#[derive(Component)]
pub struct HudMesh;

struct Pooled {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: Handle<ColorMaterial>,
}

#[derive(Resource)]
pub struct Hud {
    pub values: HudValues,
    pub visible: bool,
    player: Player,
    shapes: Vec<ShapeMesh>,
    textures: Vec<Option<Handle<Image>>>,
    pool: Vec<Pooled>,
    last: Sent,
    /// Writes and frame calls for the next tick (menus: event codes, key presses).
    pending: Vec<Pending>,
    /// Half of the extra width a window wider than 4:3 has, in stage pixels (see `widen`).
    wide: f32,
    /// A menu is open: lay the whole movie out across the window (see `widen`).
    pub menu: bool,
    /// The frame of the magic icon clip to show in the HUD's big circle (`None` hides it); see `MAGIC_ICON_CLIP`.
    pub magic_icon: Option<u16>,
}

enum Pending {
    Set(String, f64),
    Call(String),
    /// Play the clip at a path from a label (`GotoAndPlay`).
    Play(String, String),
    /// Put the clip at a path on a frame and stop it.
    Stop(String, u16),
}

/// What was last written to the movie, to raise the `*_Event` codes only on change.
#[derive(Default)]
struct Sent {
    started: bool,
    health: i32,
    health_level: i32,
    magic: i32,
    magic_level: i32,
    god: i32,
    hits: u32,
    shown: bool,
}

/// Bar length level from the meter maximum. The well clip has five lengths (`BarLevel0` to `BarLevel4`, frames 51, 102, 152, 202, 252) and the
/// fill clip shows `value` at frame `1 + value / 2`. Comparing the right edge of each well with the fill's edge (`tests/flp_font_probe.rs`) puts
/// the wells at 100, 125, 150, 175 and 200 (MEDIUM: the geometry fits to within a frame, the game's own table is not read).
fn bar_level(max: f32) -> i32 {
    ((max - 100.0) / 25.0).round().clamp(0.0, 4.0) as i32
}

impl Hud {
    /// Sets a movie variable at the start of the next tick (for example `PS2_DeadMenu_Event`).
    pub fn set(&mut self, name: &str, v: f64) {
        self.pending.push(Pending::Set(name.to_string(), v));
    }

    /// Calls a root frame by label at the start of the next tick (`PressUp`, `OpenPause` ...).
    pub fn call(&mut self, label: &str) {
        self.pending.push(Pending::Call(label.to_string()));
    }

    /// Sends the clip at `path` (for example `PowerUpMenu`) to a label and plays it, at the start of the next tick.
    pub fn play(&mut self, path: &str, label: &str) {
        self.pending.push(Pending::Play(path.to_string(), label.to_string()));
    }

    /// Puts the clip at `path` on `frame` and stops it, at the start of the next tick (a menu clip back to its empty first frame).
    pub fn stop_at(&mut self, path: &str, frame: u16) {
        self.pending.push(Pending::Stop(path.to_string(), frame));
    }

    /// A movie variable as text (a message of the table is `PS2_<id>`), empty when unset.
    pub fn text(&self, name: &str) -> String {
        self.player.get(name).map(|v| v.text()).unwrap_or_default()
    }

    /// The sounds the movie asked for since the last call: it writes a sound name into `PS2_Sound1`, `PS2_Sound2` or `PS2_Sound3` and the engine plays it.
    pub fn take_sounds(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        for n in ["PS2_Sound1", "PS2_Sound2", "PS2_Sound3"] {
            let v = self.player.get(n).map(|v| v.text()).unwrap_or_default();
            if !v.is_empty() {
                out.push(v);
                self.player.set(n, gow2_formats::flp_play::Val::Str(String::new()));
            }
        }
        out
    }

    /// The instance tree as text (debugging).
    pub fn dump(&self, depth: usize) -> String {
        self.player.dump(depth)
    }

    /// A movie variable as a number (0 when unset), for example `DeadMenu_State`.
    pub fn num(&self, name: &str) -> f64 {
        self.player.get_num(name)
    }
}

/// Builds the HUD from `R_PERMA.WAD` (next to the hero WAD). `None` when the WAD or the movie is missing.
pub fn spawn_hud(commands: &mut Commands, images: &mut Assets<Image>, perma_wad: &str) -> Option<Hud> {
    let data = std::fs::read(perma_wad).ok()?;
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let movie = recs.iter().find(|r| r.name == "FLP_HUDA" && !r.data.is_empty())?;
    let flp = flp::Flp::parse(movie.data)?;
    let model = recs.iter().find(|r| r.name == "MDL_HUDA_0" && !r.data.is_empty())?;
    let shapes = flp::parse_shapes(model.data);
    let tags: Vec<(wad::Tag, String)> = recs.iter().map(|r| (r.tag, r.name.clone())).collect();
    let group = flp::texture_group(&tags, "FLP_HUDA");
    let store = TextureStore::new(recs.iter().filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)));
    let textures = group.iter().map(|n| store.txr_texture(n).map(|t| images.add(crate::gpu::texture_image(&crate::gpu::pad_to_pow2(&t), false)))).collect();
    let mut player = Player::new(flp);
    player.clear_events();
    // the picture of the selected magic goes into the big circle at the left end of the meter blade; the movie holds the icons (clip 343, one frame per magic: the pictures of the
    // upgrade menu's medallions) but has no instance of them in the HUD, so the engine places one (stage position of the circle read from a screenshot, MEDIUM)
    let (cx, cy, s) = MAGIC_ICON_PLACE;
    player.add_overlay(MAGIC_ICON_CLIP, "MagicIcon", [s, 0.0, 0.0, s, cx * 20.0, cy * 20.0]);
    player.set_visible("MagicIcon", false);
    // the message table (`MSGS_TXT`): the engine puts each message into the variable `PS2_<id>` the movie reads
    if let Some(table) = recs.iter().find(|r| r.name == "MSGS_TXT" && r.data.len() > 4) {
        // the record is a length-prefixed text: 4 bytes of length, then the text
        let text: String = table.data.iter().map(|&c| c as char).collect();
        let start = text.find("*1*").unwrap_or(0);
        for (name, v) in flp::message_vars(&text[start..]) {
            player.set(&name, gow2_formats::flp_play::Val::Str(v));
        }
    }
    commands.spawn((
        Camera2d,
        Camera { order: 1, clear_color: ClearColorConfig::None, ..default() },
        Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::FixedVertical { viewport_height: CANVAS_HEIGHT }, ..OrthographicProjection::default_2d() }),
        Transform::from_xyz(CANVAS_HEIGHT * 4.0 / 3.0 / 2.0, -CANVAS_HEIGHT / 2.0, 100.0),
        RenderLayers::layer(HUD_LAYER),
        HudCamera,
    ));
    Some(Hud { values: HudValues::default(), visible: true, player, shapes, textures, pool: Vec::new(), last: Sent::default(), pending: Vec::new(), wide: 0.0, menu: false, magic_icon: None })
}

/// Keeps the HUD camera centred on the 4:3 stage and records how much wider than the stage the window is. The movie was made for a 4:3 frame; on a wider window its
/// pieces are laid out across the full width instead (see `Hud::widen`), so menus and backdrops fill a 16:9 screen.
pub fn scale_hud(windows: Query<&Window, With<PrimaryWindow>>, mut cams: Query<&mut Transform, With<HudCamera>>, hud: Option<ResMut<Hud>>) {
    let (Ok(w), Ok(mut t)) = (windows.single(), cams.single_mut()) else { return };
    let x = CANVAS_HEIGHT * 4.0 / 3.0 / 2.0;
    if (t.translation.x - x).abs() > 1e-3 {
        t.translation.x = x;
    }
    if let Some(mut h) = hud {
        h.wide = ((w.width() / w.height().max(1.0) - 4.0 / 3.0).max(0.0) * CANVAS_HEIGHT) / 2.0;
    }
}
fn argb(c: u32) -> [f32; 4] {
    let v = |s: u32| ((c >> s) & 0xff) as f32 / 255.0;
    [v(16), v(8), v(0), v(24)]
}

fn linear(c: [f32; 4]) -> [f32; 4] {
    let l = Color::srgba(c[0], c[1], c[2], c[3]).to_linear();
    [l.red, l.green, l.blue, l.alpha]
}

/// One batch of triangles with a single texture.
#[derive(Default)]
struct Run {
    texture: i32,
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Run {
    fn new(texture: i32) -> Run {
        Run { texture, ..Default::default() }
    }
}

fn place(m: &[f32; 6], x: f32, y: f32) -> [f32; 3] {
    [(m[0] * x + m[2] * y + m[4]) / 20.0, -(m[1] * x + m[3] * y + m[5]) / 20.0, 0.0]
}

impl Hud {
    /// How a piece spanning the stage x range `lo..hi` (pixels, 0 to 640) is laid out on a window wider than 4:3, as `(scale about the centre, shift)`.
    /// In the game the HUD pieces move out to the left or right edge by half the extra width and the rest stays; a piece as wide as the stage (a backdrop) is stretched.
    /// In a menu (`menu`) the whole layout is stretched: positions follow `x' = 320 + (x - 320) * k`, art at least 300 px wide is stretched with them, text and small pieces keep their
    /// size and are moved by their centre, and a piece that touches the centre (the Omega emblem's halves, the title plaque) stays whole.
    fn widen(&self, lo: f32, hi: f32, text: bool) -> (f32, f32) {
        if self.wide <= 0.0 {
            return (1.0, 0.0);
        }
        let w = hi - lo;
        let k = (640.0 + 2.0 * self.wide) / 640.0;
        if self.menu {
            if !text && w >= 300.0 {
                return (k, 0.0);
            }
            if lo <= 330.0 && hi >= 310.0 {
                return (1.0, 0.0);
            }
            return (1.0, ((lo + hi) / 2.0 - 320.0) * (k - 1.0));
        }
        if w >= 0.9 * 640.0 {
            return (k, 0.0);
        }
        // only pieces clear of the middle move (the Omega emblem is two halves that meet at the centre and must stay together)
        let gap = 30.0;
        if hi < 320.0 - gap {
            (1.0, -self.wide)
        } else if lo > 320.0 + gap {
            (1.0, self.wide)
        } else {
            (1.0, 0.0)
        }
    }
    fn push_shape(&self, runs: &mut Vec<Run>, d: &DrawShape) {
        let Some(sref) = self.player.flp.shapes.get(d.shape as usize) else { return };
        let Some(mesh) = self.shapes.get(sref.shape as usize) else { return };
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for im in &mesh.items {
            for v in &im.verts {
                let x = place(&d.matrix, v.x, v.y)[0];
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
        let adj = if lo <= hi { self.widen(lo, hi, false) } else { (1.0, 0.0) };
        for (item, im) in sref.items.iter().zip(&mesh.items) {
            if im.verts.is_empty() {
                continue;
            }
            let tex = if im.textured { item.texture } else { -1 };
            let base = if tex < 0 { argb(item.color) } else { [1.0; 4] };
            let c = linear([base[0] * d.cx[0], base[1] * d.cx[1], base[2] * d.cx[2], base[3] * d.cx[3]]);
            Self::emit(runs, tex, &d.matrix, 1.0, 0.0, 0.0, &im.verts, im, c, adj);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(runs: &mut Vec<Run>, tex: i32, m: &[f32; 6], scale: f32, ox: f32, oy: f32, verts: &[flp::Vtx], im: &flp::ItemMesh, color: [f32; 4], adj: (f32, f32)) {
        if runs.last().map(|r| r.texture) != Some(tex) {
            runs.push(Run::new(tex));
        }
        let run = runs.last_mut().unwrap();
        let first = run.pos.len() as u32;
        for v in verts {
            let mut p = place(m, ox + v.x * scale, oy + v.y * scale);
            p[0] = 320.0 + (p[0] - 320.0) * adj.0 + adj.1;
            run.pos.push(p);
            run.uv.push([v.u, v.v]);
            run.col.push(color);
        }
        for t in im.triangles() {
            run.idx.extend(t.iter().map(|&i| first + i as u32));
        }
    }

    /// Lays a text field out with the movie's font: glyph shapes at the field's size, aligned in its box.
    fn push_text(&self, runs: &mut Vec<Run>, d: &DrawText) {
        let flp = &self.player.flp;
        let Some(field) = flp.texts.get(d.field as usize) else { return };
        let Some(&(3, font_id)) = flp.chars.get(field.font as usize) else { return };
        let Some(font) = flp.fonts.get(font_id as usize) else { return };
        if d.text.is_empty() {
            return;
        }
        let s = field.size as f32 / 1024.0;
        let glyphs: Vec<usize> = d.text.bytes().filter_map(|c| font.map.get(c as usize).map(|&g| g as usize)).filter(|&g| g < font.glyphs.len()).collect();
        let width: f32 = glyphs.iter().map(|&g| font.advance[g] as f32).sum::<f32>() * s;
        let raw = &field.raw;
        // (the second value is the box width, not its right edge: with it read as an edge the centred "YOU ARE DEAD" sat 90 px right of the stage centre; as a width it is centred)
        let (xmin, xmax) = (i16::from_le_bytes([raw[22], raw[23]]) as f32, i16::from_le_bytes([raw[22], raw[23]]) as f32 + i16::from_le_bytes([raw[26], raw[27]]) as f32);
        let pad = u16::from_le_bytes([raw[20], raw[21]]) as f32;
        let (lo, hi) = (xmin + pad, xmax - pad);
        // alignment byte as in Flash text fields: 0 left, 1 right, 2 centre
        let mut x = match raw[30] & 3 {
            1 => hi - width,
            2 => (lo + hi) / 2.0 - width / 2.0,
            _ => lo,
        };
        let base = argb(field.color);
        let c = linear([base[0] * d.cx[0], base[1] * d.cx[1], base[2] * d.cx[2], base[3] * d.cx[3]]);
        let baseline = 844.0 * s;
        let (px0, px1) = (place(&d.matrix, x, 0.0)[0], place(&d.matrix, x + width * 1.0, 0.0)[0]);
        let adj = self.widen(px0.min(px1), px0.max(px1), true);
        if std::env::var_os("GOW_LOGTEXT").is_some() {
            println!("  raw {:02x?} matrix {:?}", &raw[..raw.len().min(48)], d.matrix);
            println!("text {:?} var {:?}: align {} box {lo:.0}..{hi:.0} width {width:.0} at px {px0:.0}..{px1:.0}", d.text, self.player.flp.string(field.var), raw[30] & 3);
        }
        for g in glyphs {
            let sref = &font.glyphs[g];
            if let Some(mesh) = self.shapes.get(sref.shape as usize) {
                for (item, im) in sref.items.iter().zip(&mesh.items) {
                    if !im.verts.is_empty() {
                        let tex = if im.textured { item.texture } else { -1 };
                        Self::emit(runs, tex, &d.matrix, s, x, baseline, &im.verts, im, c, adj);
                    }
                }
            }
            x += font.advance[g] as f32 * s;
        }
    }
}

/// Writes the game's values into the movie, runs it one tick and rebuilds the meshes.
#[allow(clippy::too_many_arguments)]
pub fn update_hud(
    time: Res<Time>,
    hud: Option<ResMut<Hud>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut vis: Query<&mut Visibility, With<HudMesh>>,
) {
    let Some(mut hud) = hud else { return };
    let hud = &mut *hud;
    let v = hud.values.clone();
    let p = &mut hud.player;
    let l = &mut hud.last;
    // the engine's bindings: values every tick, an event code when one changes
    // magic level 0 means no magic bar yet; level n shows the well `BarLevel(n - 1)`
    let (hl, ml) = (bar_level(v.health_max), bar_level(v.magic_max) + 1);
    let (hi, mi) = (v.health.round() as i32, v.magic.round() as i32);
    p.set_num("PS2_HealthMeter_Value", hi as f64);
    p.set_num("PS2_MagicMeter_Value", mi as f64);
    p.set_num("PS2_HealthMeter_Level", hl as f64);
    p.set_num("PS2_MagicMeter_Level", ml as f64);
    p.set_num("PS2_PowerOrb_Count", v.orbs as f64);
    p.set_num("PS2_GodMeter_Value", v.god as f64);
    let show = hud.visible;
    if !l.started || l.shown != show {
        p.set_num("PS2_MeterBar_Event", if show { 1.0 } else { 0.0 });
        l.shown = show;
    }
    if !l.started || hi != l.health || hl != l.health_level {
        p.set_num("PS2_HealthMeter_Event", 2.0);
        (l.health, l.health_level) = (hi, hl);
    }
    if !l.started || mi != l.magic || ml != l.magic_level {
        p.set_num("PS2_MagicMeter_Event", 2.0);
        (l.magic, l.magic_level) = (mi, ml);
    }
    // the god meter ("TMA" in the movie): shown while it holds something, hidden at zero
    let gi = v.god.round() as i32;
    if gi != l.god || !l.started && gi > 0 {
        p.set_num("PS2_GodMeter_Event", if gi > 0 { 1.0 } else { 0.0 });
        l.god = gi;
    }
    // the combo counter: shown from the second hit until 2.5 s after the last one (the rule is a stand-in, `docs/combat.md`)
    let hits = if v.hits >= 2 && v.hit_age < 2.5 { v.hits } else { 0 };
    p.set_num("PS2_HitCounter_Value", hits as f64);
    if hits != l.hits {
        p.set_num("PS2_HitCounter_Event", if hits == 0 { 0.0 } else { 1.0 });
        l.hits = hits;
    }
    l.started = true;
    for pe in hud.pending.drain(..) {
        match pe {
            Pending::Set(n, v) => p.set_num(&n, v),
            Pending::Call(l) => p.call_root(&l),
            Pending::Play(path, label) => {
                p.play_label(&path, &label);
            }
            Pending::Stop(path, frame) => {
                p.goto_instance(&path, frame);
            }
        }
    }
    p.call_root("SimKeyEvent");
    p.tick(time.delta_secs());
    match hud.magic_icon {
        Some(f) => {
            p.goto_instance("MagicIcon", f);
            p.set_visible("MagicIcon", true);
        }
        None => p.set_visible("MagicIcon", false),
    }
    // GOW_SELW=<SelWeapon frame>:<wImage frame> pins the weapon slot to look at its frames
    if let Ok(spec) = std::env::var("GOW_SELW") {
        let f: Vec<u16> = spec.split(':').filter_map(|s| s.parse().ok()).collect();
        if f.len() == 2 {
            p.goto_instance("MainMeterT/MainMeter/SelWeapon", f[0]);
            p.goto_instance("MainMeterT/MainMeter/SelWeapon/wImage", f[1]);
        }
    }

    let frame = p.draw();
    // shapes and text in drawing order; the text of a field is drawn right after the shapes before it
    let mut runs: Vec<Run> = Vec::new();
    for s in &frame.shapes {
        hud.push_shape(&mut runs, s);
    }
    for t in &frame.texts {
        hud.push_text(&mut runs, t);
    }
    // note: shapes and texts are collected separately, so text always lies on top; the HUD has no shape that should cover a number
    while hud.pool.len() < runs.len() {
        let mesh = meshes.add(empty_mesh());
        let material = materials.add(ColorMaterial::default());
        let entity = commands.spawn((Mesh2d(mesh.clone()), MeshMaterial2d(material.clone()), Transform::default(), RenderLayers::layer(HUD_LAYER), HudMesh, Visibility::Hidden)).id();
        hud.pool.push(Pooled { entity, mesh, material });
    }
    let mut hidden: HashMap<Entity, bool> = HashMap::new();
    for (i, slot) in hud.pool.iter().enumerate() {
        hidden.insert(slot.entity, i >= runs.len() || !show);
        let Some(run) = runs.get(i) else { continue };
        if let Some(mut m) = meshes.get_mut(&slot.mesh) {
            *m = build_mesh(run);
        }
        if let Some(mut mat) = materials.get_mut(&slot.material) {
            mat.texture = usize::try_from(run.texture).ok().and_then(|t| hud.textures.get(t)).cloned().flatten();
        }
        commands.entity(slot.entity).insert(Transform::from_xyz(0.0, 0.0, i as f32 * 0.01));
    }
    for (e, h) in hidden {
        if let Ok(mut vis) = vis.get_mut(e) {
            *vis = if h { Visibility::Hidden } else { Visibility::Inherited };
        }
    }
}

fn empty_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; 3]);
    m.insert_indices(Indices::U32(vec![0, 1, 2]));
    m
}

fn build_mesh(run: &Run) -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, run.pos.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, run.uv.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, run.col.clone());
    m.insert_indices(Indices::U32(run.idx.clone()));
    m
}


