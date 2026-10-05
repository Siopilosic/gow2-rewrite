//! Playtest: walk Kratos around on flat ground with a DualSense (or WASD).
//!
//! ```text
//! cargo run -p gow2-bevy --bin kratos-play -- extracted/pak/R_HERO01.WAD [extracted/pak/RHOD10.WAD]
//! ```
//! Controls (DualSense / keyboard):
//!
//! * Left stick / WASD move; Cross / Space jump and double jump.
//! * Square / J, Triangle / K, Circle / L attack (Square-Square-Triangle is the slam); R1 / Q block (a press at the right moment parries); L1 / E magic.
//! * Right stick / arrow keys evade (forward, back, left, right: `MOV_Evade*`), as in the game. The camera follows him; turn it by dragging the mouse, Z / X or the pad triggers, mouse wheel zooms.
//! * Esc / Start opens the pause menu (up and down, Cross / Enter selects, Circle / Backspace goes back); dying opens the dead menu. Tab / d-pad left and right pick a magic (id shown in the title), L1 / E casts it.
//! * B cycles the sub-weapon (none, Bone, Hammer, Olympus); with a magic selected and L1 / E held, Square, Triangle and Circle pick the Medusa beam, flash and bomb, or the Wind gust, tornado and tempest.
//! * R resets, F2 collision polygons, F3 attack balls, F4 hides the HUD, F5 mutes, [ ] sound pitch, H hurts, M spends magic, O adds orbs.
//! * V: the training dummies fight back (walk up, wind up, strike; blocking, evading and the hit reactions are the game's own moves). N: walk through walls.
//!   P: jump to the next walkable part of the level.
//!
//! Environment (for testing without a controller): `GOW_DEMO`, `GOW_DEMO_SLAM`, `GOW_EVADE`, `GOW_BLOCK`, `GOW_TOUR`, `GOW_AGGRO`, `GOW_NOCLIP`, `GOW_REGION`, `GOW_CAM`,
//! `GOW_NOCAMCOL`, `GOW_NOAUTOCAM`, `GOW_OVERLAY`, `GOW_LOGMOVES`, `GOW_HUDTEST`, `GOW_BALLS`, `GOW_MATLIST`, `GOW_MODELLIST`, `GOW_AT`, `GOW_KILL`, `GOW_AUTORESPAWN`, `GOW_MENU`, `GOW_LOGBREAK`, `GOW_LOGMISSING`, `GOW_DUMPHUD` (see `docs/rust-port.md`).
//! The simulation runs on a fixed 59.94 Hz tick; the controller is `gow2-kratos` (no engine types) and the pose is blended from the game's own clips.
//! The window title shows speed, position and the active clips.
use bevy::{
    input::{gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest}, mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll}},
    prelude::*,
};
use std::sync::Arc;

use gow2_bevy::{soldier::{self, Soldiers}, audio::{GameAudioPlugin, Pcm, SoundBoard}, blades::{spawn_blades, Blades}, fx::{self, Fx}, subweapon::{self, SubWeapons}, hero::{spawn_hero, Hero}, hud::{self, Hud}, level::{spawn_level, CollisionKind}, volumes::BodyBalls};
use gow2_formats::{dc, wad};
use gow2_kratos::{
    anim::{AirInfo, ClipInfo, Layer, Locomotion, ALL_CLIPS},
    combat::{self, Attacker, Effect, Meters, Victim},
    magic,
    enemy::{Brain, Phase},
    moves::{self, pad, Env, Input, MoveSys, Pad, Sticks},
    world::{FlatGround, World},
    AirTuning, Body, Controls, Mode, StickInput, Tuning, TICK_HZ,
};

#[derive(Resource)]
struct WadPath(String, Option<String>);

/// A breakable object as the simulation sees it: a cylinder over its bounds, solid until it breaks.
struct SolidObj {
    name: String,
    entities: Vec<Entity>,
    lo: [f32; 3],
    hi: [f32; 3],
    alive: bool,
    /// Columns and the like never break.
    fixed: bool,
}

impl SolidObj {
    /// Centre (x, z), radius, bottom and top.
    fn cylinder(&self) -> ([f32; 2], f32, f32, f32) {
        let c = [(self.lo[0] + self.hi[0]) / 2.0, (self.lo[2] + self.hi[2]) / 2.0];
        let r = ((self.hi[0] - self.lo[0]).max(self.hi[2] - self.lo[2]) / 2.0 * 0.85).max(4.0);
        (c, r, self.lo[1], self.hi[1])
    }
}

#[derive(Resource, Default)]
struct Solids {
    items: Vec<SolidObj>,
}

/// P jumps to the next walkable region of the level (the parts the game joins with doors and scripts).
#[derive(Resource)]
struct Regions {
    list: Vec<gow2_kratos::world::Region>,
    next: usize,
}

/// The opaque render geometry, which the camera must not enter.
#[derive(Resource)]
struct CamWorld(gow2_kratos::world::CollisionWorld);

/// The floor Kratos walks on: the level's triangles, or a flat plane.
/// Sound name hashes (from the hero data) to `SND_*` names.
#[derive(Resource)]
struct SndNames(std::collections::HashMap<u32, String>);

/// How much the Medusa head model is shrunk when held (visual choice; see `render_pose`).
const MEDUSA_HEAD_SCALE: f32 = 1.0;

/// The Medusa magic's head model (shown only while a Medusa move runs).
#[derive(Resource)]
struct MedusaHead(Hero);

#[derive(Resource)]
struct Ground(Box<dyn World + Send + Sync>);

/// Where Kratos starts (and returns to with R).
#[derive(Resource, Clone, Copy)]
struct Start(Vec3, f32);

#[derive(Resource)]
struct HasLevel(bool);

/// The collision polygons for the F2 overlay.
#[derive(Resource)]
struct Overlay {
    tris: Vec<([[f32; 3]; 3], CollisionKind)>,
    on: bool,
}

/// Player input gathered each frame in `Update`, consumed by the fixed tick.
#[derive(Resource, Default)]
struct PadInput {
    /// Left stick, x right, y forward (up), dead zone applied.
    stick: Vec2,
    /// Whether a gamepad supplied it.
    from_pad: bool,
    /// Raw left stick straight from the pad (debug).
    raw: Vec2,
    /// A jump press waiting for the next fixed tick (an edge, taken by the tick).
    jump_latch: bool,
    /// Pad word bits held now (`moves::pad`), and the ones pressed since the last tick (so a quick tap is never lost).
    held: u32,
    latched: u32,
    /// Right stick (arrow keys), x right, y forward: the camera outside a fight, the evades during one.
    right: Vec2,
}

#[derive(Resource)]
struct Sim {
    body: Body,
    loco: Locomotion,
    tuning: Tuning,
    air: AirTuning,
    layers: Vec<Layer>,
    moves: MoveSys,
    pad_prev: u32,
    victims: Vec<Victim>,
    meters: Meters,
    /// Seconds of hit-stop left (everything freezes), the pause the current move stored (`tActionHitPause` is kept until the move
    /// deals damage), and slow motion with its scale.
    hit_stop: f32,
    stored_pause: f32,
    slow: (f32, f32),
    /// The layers shown last tick, and the cross-fade from them: `(layers, elapsed, duration)`.
    shown: Vec<Layer>,
    fade: Option<(Vec<Layer>, f32, f32)>,
    last_move: Option<usize>,
    /// Kratos's own collision balls (fists, feet, body), and the balls of the last tick for the F3 overlay: (centre, radius, volume id).
    body_balls: Option<BodyBalls>,
    balls_world: Vec<([f32; 3], f32, u32)>,
    show_balls: bool,
    /// In a fight (stance, a move running, or a recent hit): the right stick evades instead of turning the camera.
    fighting: bool,
    /// N: walk through walls (floors still hold), to look at parts of a level the game only opens with scripts
    noclip: bool,
    /// V: the dummies fight back (walk up, wind up, strike).
    aggro: bool,
    brains: Vec<Brain>,
    /// A push from a blow Kratos took: velocity (x, z) and seconds left.
    push: ([f32; 2], f32),
    /// Seconds until he gets up after dying (0 when alive).
    dead_for: f32,
    /// Concussions in flight (the slam's shock spheres).
    blasts: Vec<combat::ActiveBlast>,
    /// The combo counter (`HitCounter_Value`), seconds since the last hit, the last victim hit, and the orb count.
    combo: u32,
    combo_age: f32,
    last_enemy: Option<u32>,
    orbs: u32,
    /// Where the dummies stand, for respawning.
    dummy_home: Vec<[f32; 3]>,
    hits: u32,
    /// The menu on screen (the simulation stands still while one is open) and how long it has been open.
    menu: Menu,
    menu_age: f32,
    /// The aim value the aim clips are blended by (eased toward `aim_goal`) and the aim angle in degrees it stands for.
    aim: f32,
    aim_deg: f32,
    /// The sub-weapon's ball centres of the last tick (for the path a ball swept).
    sub_prev: Vec<[f32; 3]>,
    /// The idle move of the magic in use, where the aim moves go back to while the magic button is held.
    idle_mv: Option<usize>,
    /// The magic natives (`gow2_kratos::magic`).
    magic: magic::MagicSys,
    /// The player's progress block as the move tests see it: `selected_magic` is cycled with Tab or the d-pad (no unlock bits are set).
    progress: moves::Progress,
    /// Seconds left of the upgrade menu's closing animation (0 = not closing).
    upgrade_closing: f32,
}

/// The magic ids the move data's unlock codes name (`BRA_*Enter` branches: 1, 2, 3, 6 and 16), in the order Tab cycles them; 0 is none.
const MAGIC_IDS: [u8; 6] = [0, 1, 2, 3, 6, 16];

/// Where the aim clips of a group are blended to, from `SCR_AimDownUp` (`FUN_00250ea0` and `FUN_00250c38`; the move data gives the range -30 to +60 degrees):
/// the angle from Kratos to his target above the horizontal, clamped to that range and mapped to -1 (the low member `...00`) .. +1 (the high member `...02`),
/// `2 * (angle + 30) / 90 - 1`. With no target the code uses a level direction (0 degrees, so -1/3). The game's target is the one the character has locked; the port
/// has no target system, so the nearest live dummy within 30 m (a guess, LOW) stands in. The aim value then eases toward its goal by 10 % per update.
fn aim_goal(sim: &Sim, range: (f32, f32)) -> f32 {
    let (min, max) = range;
    let chest = [sim.body.pos[0], sim.body.pos[1] + 22.0, sim.body.pos[2]];
    let near = sim
        .victims
        .iter()
        .filter(|v| v.alive())
        .map(|v| [v.pos[0] - chest[0], v.pos[1] + v.height * 0.5 - chest[1], v.pos[2] - chest[2]])
        .map(|d| (d, (d[0] * d[0] + d[2] * d[2]).sqrt()))
        .filter(|(_, h)| *h < 480.0)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    let deg = near.map_or(0.0, |(d, h)| d[1].atan2(h.max(1.0)).to_degrees());
    2.0 * (deg.clamp(min, max) - min) / (max - min) - 1.0
}
/// The menus of the HUD movie that stop the game: the pause menu (Start) and the one after dying.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Menu {
    #[default]
    Closed,
    Pause,
    Dead,
    /// The weapon and magic upgrade menu (`PowerUpMenu` of the HUD movie, opened with `PS2_PSMMenu_Event`).
    PowerUp,
}

/// Puts Kratos back at the start with full health, the dummies back up and the fight state cleared.
fn respawn(sim: &mut Sim, start: &Start) {
    sim.dead_for = 0.0;
    sim.body = Body::new(start.0.into(), start.1);
    sim.meters.health = sim.meters.health_max;
    sim.moves.cancel();
    for b in sim.brains.iter_mut() {
        *b = Brain::default();
    }
}

/// The training dummies' look.
#[derive(Resource)]
struct DummyVisual {
    entities: Vec<Entity>,
    materials: Vec<Handle<StandardMaterial>>,
}

const DUMMY_COLOR: Color = Color::srgb(0.55, 0.57, 0.62);

/// Moves the dummies to their simulated places, flashes them red when hit, and draws a health bar above each.
fn dummy_visual(
    time: Res<Time>,
    sim: Res<Sim>,
    visual: Option<Res<DummyVisual>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut transforms: Query<&mut Transform, Without<FollowCamera>>,
    mut gizmos: Gizmos,
) {
    let Some(visual) = visual else { return };
    let sim_time = time.elapsed_secs();
    for (i, ((v, e), m)) in sim.victims.iter().zip(&visual.entities).zip(&visual.materials).enumerate() {
        if let Ok(mut t) = transforms.get_mut(*e) {
            t.translation = Vec3::from(v.pos) + Vec3::Y * 17.0;
            // lie down when dead
            t.rotation = if v.alive() { Quat::IDENTITY } else { Quat::from_rotation_z(1.2) };
        }
        if let Some(mut mat) = materials.get_mut(m) {
            let hurt = (1.0 - v.since_hit / 0.18).clamp(0.0, 1.0);
            // a dummy about to strike glows orange
            let winding = sim.brains.get(i).map_or(0.0, |b| if b.phase == Phase::WindUp { 0.5 + 0.5 * (sim_time * 18.0).sin().abs() } else { 0.0 });
            mat.base_color = if v.petrify > 0.0 { Color::srgb(0.32, 0.31, 0.3) } else { Color::srgb(0.55 + 0.45 * hurt.max(winding), 0.57 * (1.0 - hurt) * (1.0 - 0.45 * winding), 0.62 * (1.0 - hurt) * (1.0 - winding)) };
        }
        if v.alive() {
            let p = Vec3::from(v.pos) + Vec3::Y * 46.0;
            let frac = (v.health / v.max_health).clamp(0.0, 1.0);
            gizmos.line(p - Vec3::X * 15.0, p + Vec3::X * 15.0, Color::srgb(0.25, 0.05, 0.05));
            gizmos.line(p - Vec3::X * 15.0, p + Vec3::X * (30.0 * frac - 15.0), Color::srgb(0.2, 0.9, 0.3));
        }
    }
}

/// Poses the Rhodes soldiers: each takes its animation from the state of its victim and brain, and gets a health bar above it.
fn soldier_visual(
    time: Res<Time>,
    sim: Res<Sim>,
    soldiers: Option<ResMut<Soldiers>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut transforms: Query<&mut Transform, Without<FollowCamera>>,
    mut gizmos: Gizmos,
) {
    let Some(mut soldiers) = soldiers else { return };
    let dt = if sim.menu == Menu::Closed { time.delta_secs() } else { 0.0 };
    for (i, (s, v)) in soldiers.0.iter_mut().zip(&sim.victims).enumerate() {
        let phase = sim.brains.get(i).map_or(Phase::Idle, |b| b.phase);
        s.step(dt, &soldier::Input { victim: v, phase, target: sim.body.pos });
        let (root, place) = s.show(v, &mut meshes, &mut materials);
        if let Ok(mut t) = transforms.get_mut(root) {
            *t = place;
        }
        if v.alive() {
            let p = Vec3::from(v.pos) + Vec3::Y * 52.0;
            let frac = (v.health / v.max_health).clamp(0.0, 1.0);
            gizmos.line(p - Vec3::X * 15.0, p + Vec3::X * 15.0, Color::srgb(0.25, 0.05, 0.05));
            gizmos.line(p - Vec3::X * 15.0, p + Vec3::X * (30.0 * frac - 15.0), Color::srgb(0.2, 0.9, 0.3));
        }
    }
}

/// GOW_SNAP=<file.png>@<seconds>[@<count>[@<gap seconds>]] saves the rendered frame of the game's own window to a file (the GPU copy, so it works while another program has the screen) and
/// quits a second after the last one; with a count, the files are numbered `<name>_1.png`, `_2.png`... half a second apart. For testing without taking the focus.
fn snap_and_exit(time: Res<Time>, mut commands: Commands, mut exit: MessageWriter<AppExit>, mut done: Local<u32>) {
    let Ok(spec) = std::env::var("GOW_SNAP") else { return };
    let parts: Vec<&str> = spec.split('@').collect();
    let (Some(file), Some(at)) = (parts.first(), parts.get(1).and_then(|s| s.parse::<f32>().ok())) else { return };
    let count: u32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let gap: f32 = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.5);
    let t = time.elapsed_secs();
    if *done < count && t >= at + gap * *done as f32 {
        *done += 1;
        let name = if count == 1 { file.to_string() } else { file.replace(".png", &format!("_{}.png", *done)) };
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(name));
    }
    if *done >= count && t >= at + gap * count as f32 + 1.0 {
        exit.write(AppExit::Success);
    }
}

#[derive(Component)]
struct FollowCamera {
    yaw: f32,
    pitch: f32,
    dist: f32,
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "extracted/pak/R_HERO01.WAD".into());
    let level = std::env::args().nth(2);
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            // GOW_WINPOS=x,y puts the window there (for testing: off the screen another game is on)
            primary_window: Some(Window {
                title: "Kratos".into(),
                focused: std::env::var_os("GOW_WINPOS").is_none(),
                position: std::env::var("GOW_WINPOS").ok().and_then(|s| {
                    let v: Vec<i32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    (v.len() == 2).then(|| WindowPosition::At(IVec2::new(v[0], v[1])))
                }).unwrap_or_default(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(GameAudioPlugin)
        .insert_resource(ClearColor(Color::srgb(0.06, 0.07, 0.09)))
        .insert_resource(Time::<Fixed>::from_hz(TICK_HZ as f64))
        .insert_resource(WadPath(path, level))
        .init_resource::<PadInput>()
        .init_resource::<Solids>()
        .add_systems(Startup, setup)
        .add_systems(Update, (read_input, camera_input, render_pose, dummy_visual, soldier_visual, follow_camera, gow2_bevy::level::sky_follow, ground_grid, collision_overlay, ball_overlay, magic_fx, gow2_bevy::particles::update, sub_weapon_fx, menu_system, hud_feed, hud::update_hud, hud_sounds, hud::scale_hud, title).chain())
        .add_systems(Update, (snap_and_exit, selection_label))
        .add_systems(FixedUpdate, sim_tick)
        .run();
}

/// Optional text under the HUD (GOW_LABEL=1) that names the selected magic and sub-weapon.
#[derive(Component)]
struct SelectionLabel;

/// The names the game's own message table gives the magics and sub-weapons: `PS2_4705` to `PS2_4713` come in the order of the upgrade menu's list (`PowerUpMenu:setMed`:
/// Blades, Lightning, Olympus, Wind, Electric, Hammer, Medusa, Bone, Earth; the upgrade texts 4731 to 4811 name the same items in that order). MEDIUM.
fn magic_message(id: u8) -> Option<&'static str> {
    Some(match id {
        1 => "PS2_4706",
        2 => "PS2_4709",
        3 => "PS2_4708",
        6 => "PS2_4713",
        16 => "PS2_4711",
        _ => return None,
    })
}

fn weapon_message(i: u8) -> &'static str {
    ["PS2_4712", "PS2_4710", "PS2_4707"][i as usize % 3]
}

/// Keeps the label under the HUD current: the selected magic and sub-weapon, hidden with the HUD and in menus.
fn selection_label(sim: Res<Sim>, hud: Option<Res<Hud>>, mut labels: Query<(&mut Text, &mut Visibility), With<SelectionLabel>>) {
    let Some(hud) = hud else { return };
    for (mut text, mut vis) in &mut labels {
        let mut lines = Vec::new();
        if let Some(m) = magic_message(sim.progress.selected_magic) {
            lines.push(format!("Magic: {}", hud.text(m)));
        }
        if let Some(w) = sim.progress.sub_weapon {
            lines.push(format!("Weapon: {}", hud.text(weapon_message(w))));
        }
        // (the game shows the magic and the sub-weapon as pictures in the HUD's circles; this text is for GOW_LABEL=1 only)
        let shown = std::env::var_os("GOW_LABEL").is_some() && hud.visible && sim.menu == Menu::Closed && !lines.is_empty();
        *vis = if shown { Visibility::Inherited } else { Visibility::Hidden };
        let s = lines.join("\n");
        if text.0 != s {
            text.0 = s;
        }
    }
}

fn setup(
    mut commands: Commands,
    path: Res<WadPath>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // the move graph from the hero WAD's gameplay data, and every clip its moves play
    let (move_set, attachments, snd_names): (Arc<dc::MoveSet>, Vec<dc::Attachment>, std::collections::HashMap<u32, String>) = {
        let data = std::fs::read(&path.0).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.0));
        let recs: Vec<wad::Record> = wad::records(&data).collect();
        let d = dc::Dc::from_records(&recs).expect("no DC data in the hero WAD");
        (Arc::new(d.move_set()), d.chained_attachments(), d.names.clone())
    };
    commands.insert_resource(SndNames(snd_names));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 20.0.into(), ..default() },
        TextColor(Color::srgb(0.92, 0.82, 0.55)),
        Node { position_type: PositionType::Absolute, left: Val::Percent(3.5), top: Val::Percent(24.0), ..default() },
        Visibility::Hidden,
        SelectionLabel,
    ));
    let mut clip_names: Vec<&str> = ALL_CLIPS.to_vec();
    // the full-body stances the magic aim clips are laid over
    clip_names.extend(["magMedusaStrafeIdle", "magWindStrafeIdle"]);
    // the clips Kratos stands, walks and jumps with while he holds a sub-weapon
    for w in subweapon::NAMES {
        for stem in ["Idle", "WalkSlow", "WalkFast", "Jump", "DoubleJump", "Fall", "FallLoop", "Land"] {
            clip_names.push(moves::intern(&format!("wpn{w}{stem}")));
        }
    }
    for m in &move_set.moves {
        if !m.anim.is_empty() && !clip_names.contains(&m.anim.as_str()) {
            clip_names.push(m.anim.as_str());
        }
    }
    println!("{} moves, {} entry branches, {} distinct clips named", move_set.moves.len(), move_set.entry.len(), clip_names.len());
    let hero = spawn_hero(&mut commands, &mut meshes, &mut materials, &mut images, &path.0, "hero", &clip_names);
    // the blades: stage 5 is the default blades' level in the RAM captures (`docs/combat.md` 4.1)
    let weapon = std::path::Path::new(&path.0).with_file_name("R_WEAPON0_5.WAD");
    if let Some(b) = spawn_blades(&mut commands, &mut meshes, &mut materials, &mut images, &weapon.to_string_lossy(), &hero, &attachments, hero.root) {
        commands.insert_resource(b);
    } else {
        eprintln!("no blades: {} missing or its joints not found", weapon.display());
    }
    // the head of the Medusa magic: a rigged model of `R_M_MEDUSA0.WAD` whose clips carry the names of Kratos's own `magMedusa*` clips; it is shown while a Medusa move runs
    let head_wad = std::path::Path::new(&path.0).with_file_name("R_M_MEDUSA0.WAD");
    if head_wad.exists() {
        let mut names: Vec<String> = Vec::new();
        for m in &move_set.moves {
            if m.anim.starts_with("magMedusa") {
                for suffix in ["", "00", "01", "02"] {
                    let n = format!("{}{suffix}", m.anim);
                    if !names.contains(&n) {
                        names.push(n);
                    }
                }
            }
        }
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let head = spawn_hero(&mut commands, &mut meshes, &mut materials, &mut images, &head_wad.to_string_lossy(), "medusaHead", &refs);
        commands.entity(head.root).insert(Visibility::Hidden);
        if std::env::var_os("GOW_LOGFX").is_some() {
            let posed = head.posed(&head.skel.bind);
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in &posed {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            println!("fx model medusaHead (bind pose): x {:.0}..{:.0} y {:.0}..{:.0} z {:.0}..{:.0}", lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]);
        }
        commands.insert_resource(MedusaHead(head));
    }
    // the magic effects (models of the R_M_* WADs next to the hero WAD)
    {
        let dir = std::path::Path::new(&path.0).parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let f = fx::spawn(&mut commands, &mut meshes, &mut materials, &mut images, &dir);
        commands.insert_resource(f);
        let pfx = gow2_bevy::particles::spawn(&mut commands, &mut meshes, &mut materials, &mut images, &dir, &["R_M_EARTH0.WAD", "R_M_MEDUSA0.WAD", "R_M_LGHTN2.WAD", "R_M_ELCTRC0.WAD", "R_M_WIND0.WAD", "R_WEAPON0_5.WAD"]);
        commands.insert_resource(pfx);
        if let Some(sw) = subweapon::spawn(&mut commands, &mut meshes, &mut materials, &mut images, &dir, &clip_names) {
            commands.insert_resource(sw);
        }
    }
    let body_balls = BodyBalls::load(&path.0, &hero.skel);
    let perma = std::path::Path::new(&path.0).with_file_name("R_PERMA.WAD");
    // sound: the shared banks and Kratos's voice from R_PERMA, plus the level's own bank
    {
        let perma_s = perma.to_string_lossy().to_string();
        // the sound banks of the sub-weapons and the magic (SBP_* in their WADs) join the shared ones
        let dir = perma.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let extra: Vec<String> = ["R_S_BONE0", "R_S_HAMMER0", "R_S_OLYMPUS0", "R_M_EARTH0", "R_M_ELCTRC0", "R_M_LGHTN2", "R_M_MEDUSA0", "R_M_WIND0"].iter().map(|n| dir.join(format!("{n}.WAD")).to_string_lossy().to_string()).collect();
        let mut wads: Vec<&str> = vec![perma_s.as_str()];
        wads.extend(extra.iter().map(|s| s.as_str()));
        if let Some(l) = &path.1 {
            wads.push(l.as_str());
        }
        match SoundBoard::load(&wads) {
            Some(b) => commands.insert_resource(b),
            None => eprintln!("no sound banks in {wads:?}"),
        }
    }
    match hud::spawn_hud(&mut commands, &mut images, &perma.to_string_lossy()) {
        Some(h) => commands.insert_resource(h),
        None => eprintln!("no HUD: {} missing or has no HUDA010", perma.display()),
    }
    commands.insert_resource(hero);
    // the level, if one was given: its static models and a floor index
    let (ground, start, has_level): (Box<dyn World + Send + Sync>, Start, bool) = match &path.1 {
        Some(lp) => {
            let lv = spawn_level(&mut commands, &mut meshes, &mut materials, &mut images, lp);
            println!("level {lp}: {} models, {} triangles, bounds {:?}", lv.models, lv.triangles, lv.bounds);
            println!("{}", lv.collision);
            commands.insert_resource(CamWorld(lv.camera));
            println!("walkable regions: {}", lv.regions.iter().map(|r| format!("{} cells at ({:.0}, {:.0})", r.cells, r.centre[0], r.centre[2])).collect::<Vec<_>>().join("; "));
            // GOW_REGION=n starts in the n-th walkable region (0 is the largest)
            let region_start: Option<[f32; 3]> = std::env::var("GOW_REGION").ok().and_then(|v| v.parse::<usize>().ok()).and_then(|k| lv.regions.get(k)).map(|r| r.centre);
            // levels without a known start begin in the largest walkable region
            let focus = lv.focus;
            let largest: Option<[f32; 3]> = lv.regions.iter().max_by(|a, b| {
                // big regions near the middle of the models score highest
                let score = |r: &gow2_kratos::world::Region| (r.cells as f32).sqrt() / (1.0 + ((r.centre[0] - focus[0]).powi(2) + (r.centre[2] - focus[2]).powi(2)).sqrt() / 300.0).powi(2);
                score(a).total_cmp(&score(b))
            }).map(|r| r.centre);
            commands.insert_resource(Regions { list: lv.regions, next: 0 });
            println!("breakable objects: {}", lv.breakables.len());
            if std::env::var_os("GOW_LOGBREAK").is_some() {
                for b in &lv.breakables {
                    println!("breakable {} lo {:?} hi {:?}", b.name, b.lo, b.hi);
                }
            }
            commands.insert_resource(Solids { items: lv.breakables.into_iter().map(|b| SolidObj { name: b.name, entities: b.entities, lo: b.lo, hi: b.hi, alive: true, fixed: false }).chain(lv.pillars.iter().map(|&(lo, hi)| SolidObj { name: "pillar".into(), entities: Vec::new(), lo, hi, alive: true, fixed: true })).collect() });
            commands.insert_resource(Overlay { tris: lv.overlay, on: std::env::var("GOW_OVERLAY").is_ok() });
            // RHOD10 (the Rhodes opening): where the game put Kratos in the RAM captures (docs/animation.md, ground truth)
            let name = lp.rsplit(['/', '\\']).next().unwrap_or("").to_ascii_uppercase();
            let (mut p, heading) = if name.starts_with("RHOD10") {
                (Vec3::new(-1704.28, 3712.0, -5353.45), 0.5862)
            } else {
                let c = (lv.bounds.0 + lv.bounds.1) / 2.0;
                (Vec3::new(c.x, lv.bounds.1.y, c.z), 0.0)
            };
            if let Some(c) = region_start.or(if name.starts_with("RHOD10") { None } else { largest }) {
                p = Vec3::from(c);
            }
            // GOW_AT=x:y:z starts there (y only picks the floor level)
            if let Some(f) = std::env::var("GOW_AT").ok().map(|v| v.split(':').filter_map(|s| s.parse::<f32>().ok()).collect::<Vec<_>>()).filter(|f| f.len() == 3) {
                p = Vec3::new(f[0], f[1], f[2]);
            }
            match lv.world.floor(p.x, p.z, p.y + 60.0) {
                Some(fy) => p.y = fy,
                None => println!("no floor under the start position {p}; check the level"),
            }
            (lv.world, Start(p, heading), true)
        }
        None => {
            // the sandbox: no level, a flat floor at y = 0 (a dark plane under the grid) and the dummies
            let floor = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(6000.0)));
            let mat = materials.add(StandardMaterial { base_color: Color::srgb(0.11, 0.12, 0.15), unlit: true, ..default() });
            commands.spawn((Mesh3d(floor), MeshMaterial3d(mat), Transform::from_xyz(0.0, -0.2, 0.0)));
            (Box::new(FlatGround(0.0)), Start(Vec3::ZERO, std::f32::consts::PI), false)
        }
    };
    // training dummies a few metres ahead of the start, on the floor
    let face = Vec2::new(-start.1.sin(), -start.1.cos());
    let mut home = Vec::new();
    let mut victims = Vec::new();
    for (i, (fwd, side)) in [(48.0f32, 0.0f32), (70.0, 48.0), (70.0, -48.0)].into_iter().enumerate() {
        let (x, z) = (start.0.x + face.x * fwd - face.y * side, start.0.z + face.y * fwd + face.x * side);
        let y = ground.floor(x, z, start.0.y + 60.0).unwrap_or(start.0.y);
        home.push([x, y, z]);
        victims.push(Victim::new(i as u32 + 1, [x, y, z], 100.0, 100.0));
    }
    let n_victims = victims.len();
    // the targets are Rhodes soldiers (the game's model and clips) when R_RHSOLD00.WAD is beside the hero WAD; GOW_CAPSULES=1 brings the grey capsules back
    let soldier_wad = std::path::Path::new(&path.0).with_file_name("R_RHSOLD00.WAD");
    let mut soldiers = Vec::new();
    if std::env::var_os("GOW_CAPSULES").is_none() {
        for _ in &victims {
            // they start turned toward Kratos
            let heading = face.x.atan2(face.y);
            match soldier::spawn(&mut commands, &mut meshes, &mut materials, &mut images, &soldier_wad.to_string_lossy(), heading) {
                Some(s) => soldiers.push(s),
                None => break,
            }
        }
    }
    let has_soldiers = soldiers.len() == n_victims;
    if has_soldiers {
        commands.insert_resource(Soldiers(soldiers));
    } else {
        let capsule = meshes.add(Capsule3d::new(10.0, 14.0));
        let mats: Vec<Handle<StandardMaterial>> = victims.iter().map(|_| materials.add(StandardMaterial { base_color: DUMMY_COLOR, unlit: true, ..default() })).collect();
        let entities: Vec<Entity> = victims
            .iter()
            .zip(&mats)
            .map(|(v, m)| commands.spawn((Mesh3d(capsule.clone()), MeshMaterial3d(m.clone()), Transform::from_translation(Vec3::from(v.pos) + Vec3::Y * 17.0))).id())
            .collect();
        commands.insert_resource(DummyVisual { entities, materials: mats });
    }
    commands.insert_resource(Sim {
        body: Body::new(start.0.into(), start.1),
        loco: Locomotion::default(),
        tuning: Tuning::default(),
        air: AirTuning::default(),
        layers: Vec::new(),
        moves: MoveSys::new(move_set),
        pad_prev: 0,
        victims,
        // GOW_HUDTEST=1 starts with a damaged, partly spent state and 318 orbs (the reference screenshot's count) to look at the HUD
        meters: if std::env::var("GOW_HUDTEST").is_ok() { Meters { health: 140.0, magic: 95.0, god: 60.0, ..Meters::default() } } else { Meters::default() },
        hit_stop: 0.0,
        stored_pause: 0.0,
        slow: (1.0, 0.0),
        shown: Vec::new(),
        fade: None,
        last_move: None,
        body_balls,
        balls_world: Vec::new(),
        blasts: Vec::new(),
        combo: 0,
        combo_age: 99.0,
        last_enemy: None,
        orbs: std::env::var("GOW_ORBS").ok().and_then(|v| v.parse().ok()).unwrap_or(if std::env::var("GOW_HUDTEST").is_ok() { 318 } else { 0 }),
        show_balls: std::env::var("GOW_BALLS").is_ok(),
        fighting: false,
        noclip: std::env::var_os("GOW_NOCLIP").is_some(),
        aggro: std::env::var_os("GOW_AGGRO").is_some() || (has_soldiers && std::env::var_os("GOW_PASSIVE").is_none()),
        brains: (0..n_victims).map(|_| Brain::default()).collect(),
        push: ([0.0; 2], 0.0),
        dead_for: 0.0,
        dummy_home: home,
        hits: 0,
        menu: Menu::Closed,
        menu_age: 0.0,
        aim: -1.0 / 3.0,
        aim_deg: 0.0,
        idle_mv: None,
        upgrade_closing: 0.0,
        sub_prev: Vec::new(),
        magic: magic::MagicSys::new(std::env::var("GOW_MAGIC_LEVEL").ok().and_then(|v| v.parse().ok()).unwrap_or(2)),
        // GOW_MAGIC=<id> starts with a magic selected (1, 2, 3, 6, 16)
        progress: moves::Progress { selected_magic: std::env::var("GOW_MAGIC").ok().and_then(|v| v.parse().ok()).unwrap_or(0), sub_weapon: std::env::var("GOW_SUBWEAPON").ok().and_then(|v| v.parse().ok()), ..moves::Progress::default() },
    });
    commands.insert_resource(Ground(ground));
    commands.insert_resource(start);
    commands.insert_resource(HasLevel(has_level));
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { far: 100_000.0, ..default() }),
        {
            // GOW_CAM=yaw,pitch,dist overrides the start (a high pitch and a long distance give a map view with GOW_NOCAMCOL=1)
            let o: Vec<f32> = std::env::var("GOW_CAM").map(|s| s.split(',').filter_map(|v| v.parse().ok()).collect()).unwrap_or_default();
            FollowCamera { yaw: o.first().copied().unwrap_or(start.1), pitch: o.get(1).copied().unwrap_or(0.28), dist: o.get(2).copied().unwrap_or(110.0) }
        },
        Transform::default(),
    ));
}

const DEAD_ZONE: f32 = 0.15;

fn dead_zone(v: Vec2) -> Vec2 {
    let m = v.length();
    if m < DEAD_ZONE {
        Vec2::ZERO
    } else {
        v * ((m - DEAD_ZONE) / (1.0 - DEAD_ZONE)).min(1.0) / m
    }
}

fn read_input(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, time: Res<Time>, mut input: ResMut<PadInput>) {
    let mut stick = Vec2::ZERO;
    let mut from_pad = false;
    input.raw = Vec2::ZERO;
    if let Some(pad) = pads.iter().next() {
        input.raw = pad.left_stick();
        stick = dead_zone(pad.left_stick());
        from_pad = stick != Vec2::ZERO;
    }
    if stick == Vec2::ZERO {
        let axis = |pos: KeyCode, neg: KeyCode| (keys.pressed(pos) as i8 - keys.pressed(neg) as i8) as f32;
        stick = Vec2::new(axis(KeyCode::KeyD, KeyCode::KeyA), axis(KeyCode::KeyW, KeyCode::KeyS));
        if stick.length() > 1.0 {
            stick = stick.normalize();
        }
    }
    // GOW_TOUR="secs:x:y;secs:x:y;..." scripts the left stick (camera-relative) for walking the level without a controller; `j` as a fourth field jumps
    static TOUR: std::sync::OnceLock<Vec<(f32, Vec2, bool)>> = std::sync::OnceLock::new();
    let tour = TOUR.get_or_init(|| {
        std::env::var("GOW_TOUR")
            .map(|s| {
                s.split(';')
                    .filter_map(|seg| {
                        let f: Vec<&str> = seg.split(':').collect();
                        Some((f.first()?.parse().ok()?, Vec2::new(f.get(1)?.parse().ok()?, f.get(2)?.parse().ok()?), f.get(3) == Some(&"j")))
                    })
                    .collect()
            })
            .unwrap_or_default()
    });
    if !tour.is_empty() {
        let mut t = time.elapsed_secs() - 3.0;
        for (secs, s, jump) in tour {
            if t < 0.0 {
                break;
            }
            if t < *secs {
                stick = *s;
                if *jump && (t % 1.0) < time.delta_secs() {
                    input.jump_latch = true;
                }
                break;
            }
            t -= secs;
        }
    }
    input.right = pads.iter().next().map_or(Vec2::ZERO, |p| dead_zone(p.right_stick()));
    if input.right == Vec2::ZERO {
        let axis = |pos: KeyCode, neg: KeyCode| (keys.pressed(pos) as i8 - keys.pressed(neg) as i8) as f32;
        input.right = Vec2::new(axis(KeyCode::ArrowRight, KeyCode::ArrowLeft), axis(KeyCode::ArrowUp, KeyCode::ArrowDown));
    }
    // GOW_EVADE=start:x:y flicks the right stick that way for 0.25 s every 3 s (to check the evades without a controller)
    if let Ok(v) = std::env::var("GOW_EVADE") {
        let f: Vec<f32> = v.split(':').filter_map(|s| s.parse().ok()).collect();
        if f.len() == 3 {
            let t = time.elapsed_secs() - f[0];
            if t > 0.0 && t % 3.0 < 0.25 {
                input.right = Vec2::new(f[1], f[2]);
            }
        }
    }
    input.stick = stick;
    input.from_pad = from_pad;
    // jump: Cross on the pad, or Space
    let pad_jump = pads.iter().next().map_or(false, |p| p.just_pressed(GamepadButton::South));
    if pad_jump || keys.just_pressed(KeyCode::Space) {
        input.jump_latch = true;
    }
    // combat buttons: Square J / West, Triangle K / North, Circle L / East, block Q / R1 (held), magic E / L1
    let map = [
        (pad::SQUARE, KeyCode::KeyJ, GamepadButton::West),
        (pad::TRIANGLE, KeyCode::KeyK, GamepadButton::North),
        (pad::CIRCLE, KeyCode::KeyL, GamepadButton::East),
        (pad::BLOCK, KeyCode::KeyQ, GamepadButton::RightTrigger),
        (pad::MAGIC, KeyCode::KeyE, GamepadButton::LeftTrigger),
    ];
    let gp = pads.iter().next();
    let (mut held, mut pressed) = (0u32, 0u32);
    for (bit, key, button) in map {
        if keys.pressed(key) || gp.map_or(false, |p| p.pressed(button)) {
            held |= bit;
        }
        if keys.just_pressed(key) || gp.map_or(false, |p| p.just_pressed(button)) {
            pressed |= bit;
        }
    }
    // GOW_DEMO=1: a scripted Square tap every 0.3 s, to check the combo without a controller
    static DEMO: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    // the value is the number of seconds to wait before the first tap (default 3)
    if let Some(start) = *DEMO.get_or_init(|| std::env::var("GOW_DEMO").ok().map(|v| v.parse().unwrap_or(3.0))) {
        let t = time.elapsed_secs();
        if (t % 0.3) < time.delta_secs() && t > start {
            pressed |= pad::SQUARE;
        }
    }
    // GOW_DEMO_SLAM=<start>: Square, Square, Triangle every 4 s (the slam finisher), to check it without a controller
    static SLAM: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    if let Some(start) = *SLAM.get_or_init(|| std::env::var("GOW_DEMO_SLAM").ok().map(|v| v.parse().unwrap_or(3.0))) {
        let t = time.elapsed_secs() - start;
        if t > 0.0 {
            let phase = t % 4.0;
            let d = time.delta_secs();
            let crossed = |at: f32| phase >= at && phase - d < at;
            if crossed(0.0) || crossed(0.35) {
                pressed |= pad::SQUARE;
            }
            if crossed(0.7) {
                pressed |= pad::TRIANGLE;
            }
        }
    }
    // GOW_PAD="from:to:button,button;from:to:button" holds those buttons (square triangle circle block magic) between the two times, pressing them on the first frame:
    // `GOW_PAD=3:8:magic;3.3:8:circle` holds the magic button from 3 s and Circle from 3.3 s, to try a cast without a controller
    if let Ok(spec) = std::env::var("GOW_PAD") {
        let t = time.elapsed_secs();
        let d = time.delta_secs();
        for seg in spec.split(|c| c == ';' || c == '|') {
            let f: Vec<&str> = seg.split(':').collect();
            let (Some(a), Some(b), Some(names)) = (f.first().and_then(|s| s.parse::<f32>().ok()), f.get(1).and_then(|s| s.parse::<f32>().ok()), f.get(2)) else { continue };
            if t < a || t > b {
                continue;
            }
            for n in names.split(',') {
                let bit = match n {
                    "square" => pad::SQUARE,
                    "triangle" => pad::TRIANGLE,
                    "circle" => pad::CIRCLE,
                    "block" => pad::BLOCK,
                    "magic" => pad::MAGIC,
                    _ => 0,
                };
                held |= bit;
                if t - d < a {
                    pressed |= bit;
                }
            }
        }
    }
    // GOW_BLOCK=1 holds the block button, to check the defence without a controller
    if std::env::var_os("GOW_BLOCK").is_some() {
        held |= pad::BLOCK;
        if time.elapsed_secs() < time.delta_secs() * 3.0 + 2.0 {
            pressed |= pad::BLOCK;
        }
    }
    input.held = held;
    input.latched |= pressed;
}

fn camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    start: Res<Start>,
    input: Res<PadInput>,
    mut sim: ResMut<Sim>,
    mut cams: Query<&mut FollowCamera>,
    mut manual_until: Local<f32>,
    mut regions: Option<ResMut<Regions>>,
) {
    let dt = time.delta_secs();
    for mut c in &mut cams {
        let mut manual = false;
        if buttons.pressed(MouseButton::Left) {
            c.yaw -= motion.delta.x * 0.006;
            c.pitch = (c.pitch + motion.delta.y * 0.006).clamp(-0.2, 1.3);
            manual = true;
        }
        // the right stick (arrow keys) is the evade, as in the game; the camera turns with the mouse (left button), Z and X, or the pad triggers
        let mut turn = (keys.pressed(KeyCode::KeyZ) as i8 - keys.pressed(KeyCode::KeyX) as i8) as f32;
        if let Some(pad) = pads.iter().next() {
            turn += pad.get(GamepadButton::LeftTrigger2).unwrap_or(0.0) - pad.get(GamepadButton::RightTrigger2).unwrap_or(0.0);
        }
        c.yaw += turn * 2.2 * dt;
        c.pitch = (c.pitch + (keys.pressed(KeyCode::PageUp) as i8 - keys.pressed(KeyCode::PageDown) as i8) as f32 * 1.2 * dt).clamp(-0.2, 1.3);
        manual |= turn != 0.0;
        // auto-follow: while Kratos walks forward and the player is not steering the camera, it swings round behind him
        // (the camera sits at `dir(yaw)` from him and he faces `-dir(heading)`, so the target yaw is his heading)
        let now = time.elapsed_secs();
        if manual {
            *manual_until = now + 1.5;
        }
        let walking = input.stick.length() > 0.3 && input.stick.y > -0.5 && sim.moves.current().is_none();
        if walking && now > *manual_until && std::env::var_os("GOW_NOAUTOCAM").is_none() {
            let diff = gow2_kratos::wrap_angle(sim.body.heading - c.yaw);
            let rate = 1.6 * (0.25 + 0.75 * input.stick.y.max(0.0));
            c.yaw += diff.clamp(-rate * dt, rate * dt) * if diff.abs() > 0.02 { 1.0 } else { 0.0 };
        }
        // GOW_CAMBEHIND=<offset radians> keeps the camera behind his heading (plus the offset): for looking along the aim of a magic without steering by hand
        if let Some(off) = std::env::var("GOW_CAMBEHIND").ok().and_then(|v| v.parse::<f32>().ok()) {
            c.yaw = sim.body.heading + off;
        }
        c.dist = (c.dist * (1.0 - scroll.delta.y * 0.1)).clamp(40.0, 400.0);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        sim.body = Body::new(start.0.into(), start.1);
    }
    if keys.just_pressed(KeyCode::KeyN) {
        sim.noclip = !sim.noclip;
    }
    if keys.just_pressed(KeyCode::KeyV) {
        sim.aggro = !sim.aggro;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        if let Some(r) = regions.as_deref_mut().filter(|r| !r.list.is_empty()) {
            let at = r.list[r.next % r.list.len()].centre;
            r.next += 1;
            sim.body = Body::new(at, sim.body.heading);
        }
    }
    // fell out of the world: back to the start
    if sim.body.pos[1] < start.0[1] - 2500.0 {
        sim.body = Body::new(start.0.into(), start.1);
    }
}

/// The level without its walls: floors and ceilings still answer.
struct Ghost<'a>(&'a dyn World);

impl World for Ghost<'_> {
    // with no floor in reach he keeps his height instead of falling, so he can cross the gaps between floors
    fn floor(&self, x: f32, z: f32, max_y: f32) -> Option<f32> {
        self.0.floor(x, z, max_y).or(Some(max_y - gow2_kratos::STEP_UP))
    }
    fn ceiling(&self, x: f32, z: f32, min_y: f32) -> Option<f32> {
        self.0.ceiling(x, z, min_y)
    }
}

/// Camera-relative stick to a world direction (x, z). The camera sits at `target + dir(yaw) * dist` and looks along `-dir(yaw)`.
fn world_dir(stick: Vec2, cam_yaw: f32) -> [f32; 2] {
    let f = Vec2::new(-cam_yaw.sin(), -cam_yaw.cos());
    let r = Vec2::new(-f.y, f.x);
    let d = f * stick.y + r * stick.x;
    [d.x, d.y]
}

/// The velocity (world x, z in units per second) the running move's clip asks for over this tick: the change of `zeroJoint`'s position in the clip,
/// turned by the heading (rig x is to the right, rig -z is forward). `None` for clips that do not move it.
fn root_motion(hero: &Hero, sim: &Sim, dt: f32) -> Option<[f32; 2]> {
    let (m, inst) = sim.moves.current()?;
    let clip = hero.clips.get(m.anim.as_str())?;
    let zero = hero.skel.names.iter().position(|n| n == "zeroJoint")?;
    let step = dt * if m.rate > 0.0 { m.rate } else { 1.0 };
    let t1 = inst.clip_time.min(clip.duration);
    let t0 = (inst.clip_time - step).max(0.0).min(clip.duration);
    if t1 <= t0 || inst.age <= step * 0.5 {
        return Some([0.0, 0.0]);
    }
    let at = |t: f32| {
        let w = gow2_skel::world_matrices(&hero.skel, &clip.sample(&hero.skel, t));
        [w[zero][12], w[zero][14]]
    };
    let (a, b) = (at(t0), at(t1));
    let (dx, dz) = ((b[0] - a[0]) / dt, (b[1] - a[1]) / dt);
    let h = sim.body.heading;
    // rotate by the heading about y: rig (x, z) to world
    Some([dx * h.cos() + dz * h.sin(), -dx * h.sin() + dz * h.cos()])
}

fn sim_tick(time: Res<Time>, start: Res<Start>, mut input: ResMut<PadInput>, cams: Query<&FollowCamera>, hero: Option<Res<Hero>>, ground: Option<Res<Ground>>, mut blades: Option<ResMut<Blades>>, mut sim: ResMut<Sim>, names: Res<SndNames>, mut board: Option<ResMut<SoundBoard>>, mut pcm: ResMut<Assets<Pcm>>, mut commands: Commands, mut step_clock: Local<f32>, pads: Query<Entity, With<Gamepad>>, mut rumble: MessageWriter<GamepadRumbleRequest>, (mut solids, subw): (ResMut<Solids>, Option<Res<SubWeapons>>)) {
    // controller vibration: the move data's own rumble actions are not decoded (the sound bank drives them in the game), so these are set by hand
    let mut shake = |strong: f32, weak: f32, secs: f32| {
        for g in &pads {
            rumble.write(GamepadRumbleRequest::Add { gamepad: g, duration: std::time::Duration::from_secs_f32(secs), intensity: GamepadRumbleIntensity { strong_motor: strong, weak_motor: weak } });
        }
    };
    let (Some(hero), Some(ground), Some(cam)) = (hero, ground, cams.iter().next()) else { return };
    let real_dt = time.delta_secs();
    // a menu stops the game; what was pressed meanwhile is dropped
    if sim.menu != Menu::Closed {
        input.latched = 0;
        input.jump_latch = false;
        return;
    }
    // hit-stop freezes everything; presses made meanwhile stay latched
    if sim.hit_stop > 0.0 {
        sim.hit_stop -= real_dt;
        return;
    }
    let mut dt = real_dt;
    if sim.slow.1 > 0.0 {
        dt *= sim.slow.0;
        sim.slow.1 -= real_dt;
    }
    let dir = world_dir(input.stick, cam.yaw);
    let jump_req = std::mem::take(&mut input.jump_latch);
    let cur = input.held | std::mem::take(&mut input.latched);
    let pad_word = Pad { cur, prev: sim.pad_prev };
    sim.pad_prev = cur;

    // the move system: sticks in the character's frame (right, forward)
    let f = sim.body.facing();
    let r = [-f[1], f[0]];
    let local = |v: [f32; 2]| [v[0] * r[0] + v[1] * r[1], v[0] * f[0] + v[1] * f[1]];
    let env = Env { state_mask: if sim.body.mode == Mode::Ground { moves::STATE_GROUND } else { moves::STATE_AIR }, health: sim.meters.health, progress: moves::Progress { magic_ok: sim.meters.magic >= magic::cast_cost(sim.progress.selected_magic, sim.magic.level), ..sim.progress }, ..Default::default() };
    let clip_len = |n: &str| hero.infos.get(n).map(|i| i.duration);
    // the combat stance has no exit in the move data except attacking (see MoveSys::leave_stance_when)
    let walking = dir[0] * dir[0] + dir[1] * dir[1] > 0.1;
    sim.moves.leave_stance_when(walking, 4.0);
    let right_dir = world_dir(input.right, cam.yaw);
    let right = local(right_dir);
    if std::env::var_os("GOW_LOGMOVES").is_some() && (right[0] * right[0] + right[1] * right[1]) > 0.25 {
        println!("t={:.2} right stick (right, forward) = {:.2?} raw {:.2?}", time.elapsed_secs(), right, input.right);
    }
    let alive = sim.dead_for <= 0.0;
    let (pad_word, sticks) = if alive { (pad_word, Sticks { left: local(dir), right }) } else { (Pad { cur: 0, prev: 0 }, Sticks { left: [0.0; 2], right: [0.0; 2] }) };
    if std::env::var_os("GOW_LOGMOVES").is_some() && pad_word.cur & pad::MAGIC != 0 && pad_word.prev & pad::MAGIC == 0 {
        println!("t={:.2} magic pressed: selected {} magic {:.0} state {:?} busy {}", time.elapsed_secs(), sim.progress.selected_magic, sim.meters.magic, sim.body.mode, sim.moves.is_busy());
        let inp = Input { pad: pad_word, sticks };
        for b in sim.moves.set.entry.iter().filter(|b| b.button == 6 && b.name.contains("Enter") && !b.name.contains("Air")) {
            println!("   {} input_ok {} state_ok {:?}", b.name, moves::input_ok(b, &inp), moves::state_ok(b, None, None, &env));
        }
    }
    // The Lightning loop ends through a branch that needs the instance flag 2, which the game's move scripts set (not decoded): a stand-in sets it when the magic
    // button is let go or the meter is empty (LOW).
    let in_magic_loop = sim.moves.current().is_some_and(|(m, _)| m.anim.starts_with("mag") && m.name.contains("Loop"));
    sim.moves.flag2 = in_magic_loop && (pad_word.cur & pad::MAGIC == 0 || sim.meters.magic <= 0.0);
    // A magic idle (`MOV_MedusaIdle`, `MOV_WindIdle`) has no branch back to itself; on the ground nothing but the magic button's release ends it (its "move ended"
    // branch is for the air only), so it plays on while the button is held: when its clip runs out it starts again (LOW: the game's aim clips loop the same way).
    // The aim moves (`MOV_MedusaBeam`, `MOV_WindGustShot` ...) end with their clip as well, and the entry branches then pick the Enter move again; the port goes back to the idle
    // of the same magic instead (LOW: the game's own rule for staying in the aim state is not found).
    if let Some((m, i)) = sim.moves.current() {
        if m.anim.starts_with("mag") && m.name.contains("Idle") && !m.name.contains("Combat") {
            sim.idle_mv = Some(i.mv);
        }
    }
    let before = sim.moves.current().map(|(m, i)| (i.mv, m.anim.starts_with("mag") && !m.name.ends_with("Enter") && !m.name.ends_with("Exit")));
    let tick = sim.moves.update(dt, &Input { pad: pad_word, sticks }, &env, &clip_len);
    if let (Some((was, true)), Some(idle)) = (before, sim.idle_mv) {
        let entered = sim.moves.current().is_some_and(|(m, i)| i.mv != was && m.name.ends_with("Enter") && m.anim.starts_with("mag"));
        if (entered || tick.ended && !sim.moves.is_busy()) && pad_word.cur & pad::MAGIC != 0 {
            sim.moves.start(idle, 0.0, &clip_len);
        }
    }
    sim.fighting = sim.moves.in_stance() || sim.moves.current().is_some() || sim.combo_age < 3.0;
    if tick.started.is_some() {
        if std::env::var_os("GOW_LOGMOVES").is_some() {
            if let Some((m, _)) = sim.moves.current() {
                println!("t={:.2} move {} (clip {})", time.elapsed_secs(), m.name, m.anim);
            }
        }
        sim.stored_pause = 0.0;
        // a new attack turns Kratos toward the stick
        if dir[0] * dir[0] + dir[1] * dir[1] > 0.04 {
            sim.body.heading = gow2_kratos::heading_of(dir);
        }
    }
    let mut pending_blasts: Vec<gow2_formats::dc::Blast> = Vec::new();
    for e in combat::effects(&sim.moves, &tick.fired) {
        match e {
            Effect::Blast(b) => pending_blasts.push(b),
            Effect::Meter { selector, amount, relative } => sim.meters.adjust(selector, amount, relative),
            Effect::HitPause(s) => sim.stored_pause = s,
            Effect::Sound(h) => {
                if let (Some(b), Some(n)) = (board.as_deref_mut(), names.0.get(&h)) {
                    b.play(n, 1, &mut commands, &mut pcm);
                }
            }
            Effect::Slowdown { scale, secs } => sim.slow = (scale.max(0.01), secs),
            _ => {}
        }
    }

    // while a move runs the stick and jump belong to it; the body still falls and slides
    let busy = sim.moves.is_busy();
    // root motion: a move's clip moves the origin joint `zeroJoint` and the game moves the body with it (the mesh itself stays in place)
    let root = if busy { root_motion(&hero, &sim, dt) } else { None };
    // a blow Kratos took pushes him for a moment
    let pushed = if sim.push.1 > 0.0 {
        sim.push.1 -= dt;
        Some(sim.push.0)
    } else {
        None
    };
    let drive = match (root, pushed) {
        (None, None) => None,
        (a, b) => {
            let (a, b) = (a.unwrap_or([0.0; 2]), b.unwrap_or([0.0; 2]));
            Some([a[0] + b[0], a[1] + b[1]])
        }
    };
    let controls = if busy || sim.dead_for > 0.0 { Controls { drive, ..Controls::default() } } else { Controls { stick: StickInput { dir }, jump: jump_req, drive } };
    let (tuning, air) = (sim.tuning, sim.air);
    let events = if sim.noclip { sim.body.tick_world(dt, controls, &tuning, &air, &Ghost(&*ground.0)) } else { sim.body.tick_world(dt, controls, &tuning, &air, &*ground.0) };
    // solid things Kratos cannot walk through: unbroken crates, pots and barrels, and the dummies
    if !sim.noclip {
        let mut cyl: Vec<([f32; 2], f32, f32, f32)> = solids.items.iter().filter(|s| s.alive).map(|s| s.cylinder()).collect();
        cyl.extend(sim.victims.iter().filter(|v| v.alive()).map(|v| ([v.pos[0], v.pos[2]], v.radius, v.pos[1], v.pos[1] + v.height)));
        for (c, r, y0, y1) in cyl {
            let p = &mut sim.body.pos;
            if p[1] >= y1 - 2.0 || p[1] + gow2_kratos::BODY_HEIGHT <= y0 {
                continue;
            }
            let (dx, dz) = (p[0] - c[0], p[2] - c[1]);
            let d = (dx * dx + dz * dz).sqrt();
            let min = r + gow2_kratos::BODY_RADIUS * 0.8;
            if d < min {
                let n = if d > 1e-3 { [dx / d, dz / d] } else { [0.0, 1.0] };
                p[0] = c[0] + n[0] * min;
                p[2] = c[1] + n[1] * min;
            }
        }
    }
    let speed = (sim.body.vel[0] * sim.body.vel[0] + sim.body.vel[1] * sim.body.vel[1]).sqrt();
    let info = |n: &str| hero.infos.get(n).copied().unwrap_or(ClipInfo { duration: 1.0, ground_speed: 1.0 });
    let air_info = AirInfo { mode: sim.body.mode, vy: sim.body.vy, events };
    let mut loco_layers = sim.loco.update(dt, speed, air_info, &info);
    // holding a sub-weapon, Kratos stands, walks, jumps and falls with the weapon's clips (wpnBoneIdle, wpnBoneWalkSlow ...)
    if let Some(w) = sim.progress.sub_weapon {
        let name = subweapon::NAMES[w as usize];
        for l in loco_layers.iter_mut() {
            let stem = match l.clip {
                "navIdle" => "Idle",
                "navWalkSlow" => "WalkSlow",
                "navWalkFast" => "WalkFast",
                "navJump" | "navJumpUp" => "Jump",
                "navDoubleJump" => "DoubleJump",
                "navFall" => "Fall",
                "navFallLoop" => "FallLoop",
                "navLand" => "Land",
                _ => continue,
            };
            let mapped = format!("wpn{name}{stem}");
            if hero.clips.contains_key(&mapped) {
                l.clip = moves::intern(&mapped);
            }
        }
    }
    // footsteps: one per step of the gait (register 0 of `SND_FOOTSTEP_*`: 1 walk, 2 run, 3 land); the surface is stone throughout RHOD10
    if let Some(b) = board.as_deref_mut() {
        if events.landed {
            b.play("SND_FOOTSTEP_STONE", 3, &mut commands, &mut pcm);
        }
        if sim.body.mode == Mode::Ground && !busy && speed > 8.0 {
            *step_clock += dt;
            let (gait, every) = if speed < 70.0 { (1, 0.62) } else { (2, 0.34) };
            if *step_clock >= every {
                *step_clock = 0.0;
                b.play("SND_FOOTSTEP_STONE", gait, &mut commands, &mut pcm);
            }
        } else {
            *step_clock = 0.0;
        }
    }

    let sim = &mut *sim;
    for v in &mut sim.victims {
        v.tick(dt, &*ground.0, &tuning);
    }
    // a dead dummy gets back up after a moment
    for (i, v) in sim.victims.iter_mut().enumerate() {
        if !v.alive() && v.since_hit > 2.0 {
            let h = sim.dummy_home[i];
            *v = Victim::new(v.id, h, 100.0, 100.0);
        }
    }
    // the dummies fight back when V is on: walk up, wind up, strike
    let kratos = sim.body.pos;
    let mut blows = Vec::new();
    for (v, b) in sim.victims.iter_mut().zip(sim.brains.iter_mut()) {
        // stone does not move or strike
        if v.petrify > 0.0 {
            v.vel = [0.0; 2];
            continue;
        }
        if let Some(s) = b.update(dt, v, kratos, sim.aggro && sim.dead_for <= 0.0) {
            blows.push(s);
        }
    }
    // enemies do not stand inside each other: overlapping pairs are pushed apart along the line between them
    for i in 0..sim.victims.len() {
        for j in i + 1..sim.victims.len() {
            let (a, b) = (&sim.victims[i], &sim.victims[j]);
            if a.airborne || b.airborne || !a.alive() || !b.alive() || (a.pos[1] - b.pos[1]).abs() > 40.0 {
                continue;
            }
            let (dx, dz) = (b.pos[0] - a.pos[0], b.pos[2] - a.pos[2]);
            let d = (dx * dx + dz * dz).sqrt();
            let min = a.radius + b.radius;
            if d < min {
                let (nx, nz) = if d > 1e-3 { (dx / d, dz / d) } else { (1.0, 0.0) };
                let push = (min - d) * 0.5;
                sim.victims[i].pos[0] -= nx * push;
                sim.victims[i].pos[2] -= nz * push;
                sim.victims[j].pos[0] += nx * push;
                sim.victims[j].pos[2] += nz * push;
            }
        }
    }
    let ground_state = if sim.body.mode == Mode::Ground { moves::STATE_GROUND } else { moves::STATE_AIR };
    let mut last_heavy = false;
    for s in blows {
        last_heavy = s.heavy;
        let name = sim.moves.current().map_or(String::new(), |(m, _)| m.name.clone());
        if name.contains("Evade") {
            continue; // rolling: nothing lands
        }
        if name.contains("Block") && !name.contains("BlockBreak") {
            sim.meters.adjust(0, -s.damage * 0.1, false);
            sim.moves.register_blocked();
            shake(0.0, 0.35, 0.1);
            sim.push = ([s.dir[0] * 50.0, s.dir[1] * 50.0], 0.12);
            if std::env::var_os("GOW_LOGMOVES").is_some() {
                println!("t={:.2} blocked a blow of {:.0} in {name}, health {:.0}", time.elapsed_secs(), s.damage, sim.meters.health);
            }
            continue;
        }
        sim.meters.adjust(0, -s.damage, false);
        if s.heavy {
            shake(1.0, 0.6, 0.4)
        } else {
            shake(0.5, 0.3, 0.2)
        }
        if let Some(b) = board.as_deref_mut() {
            b.play("SND_HERO_GETHITVOC", 0, &mut commands, &mut pcm);
            b.play("SND_BODYFALL_LIGHT", 0, &mut commands, &mut pcm);
        }
        let power = if s.heavy { 230.0 } else { 90.0 };
        sim.push = ([s.dir[0] * power, s.dir[1] * power], 0.22);
        sim.hit_stop = 0.06;
        sim.combo = 0;
        // the reaction: event 0x10 with the hit class (10 light -> `MOV_HitFront`, 11 heavy -> `MOV_HitBlowBack`)
        let env = Env { state_mask: ground_state, health: sim.meters.health, target_class: if s.heavy { 11 } else { 10 }, ..Default::default() };
        let t = sim.moves.post_event(0x10, &env, &clip_len);
        if std::env::var_os("GOW_LOGMOVES").is_some() {
            println!("t={:.2} hurt {:.0} (heavy {}), health {:.0}, reaction {:?}", time.elapsed_secs(), s.damage, s.heavy, sim.meters.health, t.started.map(|m| sim.moves.set.moves[m].name.clone()));
        }
    }
    // death and getting up again
    if sim.meters.health <= 0.0 && sim.dead_for <= 0.0 {
        sim.dead_for = 3.5;
        if let Some(b) = board.as_deref_mut() {
            b.play("SND_HERO_DIEVOC", 0, &mut commands, &mut pcm);
        }
        let env = Env { state_mask: ground_state, health: 0.0, ..Default::default() };
        // the death branches tie (stumble, fly back, plane, smash ...): a heavy last blow throws him back, a light one makes him stumble
        sim.moves.post_event(0x0d, &env, &clip_len);
        if let Some(i) = sim.moves.set.find(if last_heavy { "MOV_DeathFlyBack" } else { "MOV_DeathStumble" }) {
            sim.moves.start(i, 0.0, &clip_len);
        }
        if std::env::var_os("GOW_LOGMOVES").is_some() {
            println!("t={:.2} Kratos died, move {:?}", time.elapsed_secs(), sim.moves.current().map(|(m, _)| m.name.clone()));
        }
    }
    if sim.dead_for > 0.0 {
        sim.dead_for -= dt;
        if sim.dead_for <= 0.0 {
            // the dead menu asks what to do next (`GOW_AUTORESPAWN=1` gets up at once, for runs without a controller)
            if std::env::var_os("GOW_AUTORESPAWN").is_some() {
                respawn(sim, &start);
            } else {
                sim.dead_for = 0.0;
                sim.menu = Menu::Dead;
                sim.menu_age = 0.0;
            }
        }
    }

    // the aim value eases toward the goal by 10 % per update (the game's smoothing in `FUN_00250c38`, at 60 updates a second)
    let range = sim.magic.aim_range.unwrap_or((-30.0, 60.0));
    let goal = aim_goal(sim, range);
    sim.aim += (goal - sim.aim) * (0.1 * dt * 60.0).min(1.0);
    sim.aim_deg = range.0 + (sim.aim + 1.0) * 0.5 * (range.1 - range.0);
    // animation: the move's clip, cross-faded from whatever was shown before
    let target: Vec<Layer> = match sim.moves.current() {
        Some((m, inst)) => match hero.groups.get(m.anim.as_str()) {
            // an aim group: a full-body stance underneath and the three partial aim clips over it, blended by the aim height (see `aim_height`)
            Some(members) => {
                let base = if m.anim.starts_with("magMedusa") { "magMedusaStrafeIdle" } else if m.anim.starts_with("magWind") { "magWindStrafeIdle" } else { gow2_kratos::anim::IDLE };
                let bd = hero.infos.get(base).map_or(1.0, |i| i.duration.max(0.1));
                let a = sim.aim;
                let t = inst.clip_time.min(inst.duration);
                let mut v = vec![Layer { clip: moves::intern(base), time: inst.clip_time % bd, weight: 1.0 }];
                for (k, w) in [(-a).max(0.0), 1.0 - a.abs(), a.max(0.0)].into_iter().enumerate() {
                    if w > 0.0 {
                        v.push(Layer { clip: moves::intern(&members[k]), time: t, weight: w });
                    }
                }
                v
            }
            None => vec![Layer { clip: moves::intern(&m.anim), time: inst.clip_time.min(inst.duration), weight: 1.0 }],
        },
        None => loco_layers,
    };
    let now_move = sim.moves.current().map(|(_, i)| i.mv);
    if now_move != sim.last_move {
        // entering, leaving or chaining moves: fade from the last pose over the move's blend time (or the locomotion fade)
        let dur = sim.moves.current().map_or(gow2_kratos::anim::FADE, |(m, _)| m.blend.max(0.05));
        sim.fade = Some((sim.shown.clone(), 0.0, dur));
        sim.last_move = now_move;
    }
    let mut out = target;
    if let Some((from, t, dur)) = &mut sim.fade {
        *t += dt;
        let k = (*t / *dur).clamp(0.0, 1.0);
        for l in out.iter_mut() {
            l.weight *= k;
        }
        for l in from.iter_mut() {
            l.time += dt;
            out.push(Layer { clip: l.clip, time: l.time, weight: l.weight * (1.0 - k) });
        }
        if k >= 1.0 {
            sim.fade = None;
        }
    }
    // GOW_FREEZE=clip:seconds shows one frame of a clip, to look at a pose from every side
    if let Ok(f) = std::env::var("GOW_FREEZE") {
        if let Some((name, t)) = f.split_once(':') {
            out = vec![Layer { clip: moves::intern(name), time: t.parse().unwrap_or(0.0), weight: 1.0 }];
        }
    }
    sim.shown = out.clone();
    sim.layers = out;

    // hits: the pose of this tick gives the blades and the body their collision balls
    let named: Vec<(&str, f32, f32)> = sim.layers.iter().map(|l| (l.clip, l.time, l.weight)).collect();
    let world = gow2_skel::world_matrices(&hero.skel, &hero.pose(&named));
    // the magic moves use both hands (the Medusa head, the bow): the blades stay stowed (the game's clips put them away; here the mode is forced, LOW)
    let casting = sim.moves.current().is_some_and(|(m, _)| m.anim.starts_with("mag"));
    let keep = casting || sim.moves.current().map_or(false, |(m, _)| m.flags & 2 != 0);
    let mut model_balls: Vec<(u32, [f32; 3], [f32; 3], f32)> = Vec::new();
    if let Some(b) = blades.as_deref_mut() {
        if casting {
            b.stow();
        }
        b.step(&world, dt, keep);
        model_balls.extend(b.balls());
    }
    // the sub-weapon's attack balls (volume ids 12, 11, 10 for Bone, Hammer, Olympus), at the hand joint; hidden during the magic
    if let (Some(w), Some(sw), false) = (sim.progress.sub_weapon, subw.as_ref(), casting) {
        if let Some(ih) = hero.skel.names.iter().position(|n| n.eq_ignore_ascii_case("rWeapIH")) {
            let now = sw.items[w as usize].balls(&world[ih], &named);
            let prev = std::mem::take(&mut sim.sub_prev);
            for (i, (id, c, r)) in now.iter().enumerate() {
                model_balls.push((*id, *c, prev.get(i).copied().unwrap_or(*c), *r));
            }
            sim.sub_prev = now.iter().map(|b| b.1).collect();
        }
    } else {
        sim.sub_prev.clear();
    }
    if let Some(bb) = sim.body_balls.as_mut() {
        model_balls.extend(bb.step(&world));
    }
    let root = Transform { translation: Vec3::from(sim.body.pos), rotation: Quat::from_rotation_y(sim.body.heading), scale: Vec3::ONE };
    let to_world = |p: [f32; 3]| root.transform_point(Vec3::from(p)).to_array();
    sim.balls_world = model_balls.iter().map(|&(id, c, _, r)| (to_world(c), r, id)).collect();
    let mut attacker = Attacker::new(sim.body.pos, sim.body.facing());
    attacker.balls = model_balls
        .iter()
        .map(|&(id, c, p, r)| combat::WorldBall { id, centre: to_world(c), prev: to_world(p), radius: r })
        .collect();
    let reports = combat::resolve(&mut sim.moves, &attacker, &mut sim.victims, combat::DAMAGE_MULTIPLIER);
    if !reports.is_empty() {
        shake(0.0, 0.45, 0.09);
        sim.hits += reports.len() as u32;
        sim.hit_stop = sim.stored_pause;
    }

    // concussions: spawned at the clip's joint now that the pose is known, then grown and tested each tick
    for b in pending_blasts {
        if let Some(j) = hero.skel.names.iter().position(|n| n.eq_ignore_ascii_case(&b.joint)) {
            let centre = to_world([world[j][12], world[j][13], world[j][14]]);
            sim.blasts.push(combat::ActiveBlast::new(&b, centre, sim.body.facing(), combat::DAMAGE_MULTIPLIER));
        }
    }
    // magic: the script natives of the magic moves (gow2_kratos::magic); its hits count like blast hits
    let mut blast_hits = {
        let s = &mut *sim;
        let (sin, cos) = s.aim_deg.to_radians().sin_cos();
        let f = s.body.facing();
        let mctx = magic::Ctx {
            dt,
            pad: pad_word,
            pos: s.body.pos,
            facing: f,
            chest: [s.body.pos[0], s.body.pos[1] + 22.0, s.body.pos[2]],
            aim_dir: [f[0] * cos, sin, f[1] * cos],
            grounded: s.body.mode == Mode::Ground,
            names: &names.0,
        };
        let mo = s.magic.update(&mctx, &mut s.moves, &env, &clip_len, &mut s.meters, &mut s.victims, &tick);
        if mo.shake > 0.0 {
            shake(mo.shake * 0.6, mo.shake, 0.18);
        }
        mo.hits
    };
    for b in &mut sim.blasts {
        blast_hits.extend(b.tick(dt, &mut sim.victims));
        sim.balls_world.push((b.centre, b.radius(), 8));
    }
    sim.blasts.retain(|b| !b.finished());
    for h in &blast_hits {
        // the move's "on hit" actions (meter gain, shake) follow a blast hit like a window hit
        sim.moves.register_hit(h.victim, 200, 1, h.lethal);
    }
    if !blast_hits.is_empty() {
        sim.hits += blast_hits.len() as u32;
        sim.hit_stop = sim.stored_pause;
    }
    // the combo counter: every hit adds one and restarts the timer (the real counter's rules are not decoded, LOW)
    let new_hits = (reports.len() + blast_hits.len()) as u32;
    sim.combo_age += dt;
    if new_hits > 0 {
        sim.combo += new_hits;
        sim.combo_age = 0.0;
        sim.last_enemy = reports.last().map(|r| r.victim).or(blast_hits.last().map(|r| r.victim));
    } else if sim.combo_age > 2.5 {
        sim.combo = 0;
    }
    // breaking: a blade, fist or foot of an attack in progress, or a slam, that touches an unbroken object breaks it; it gives red orbs
    let attacking = sim.moves.is_busy() && !sim.moves.in_stance() && (!sim.moves.open_windows().is_empty() || !sim.blasts.is_empty());
    if attacking {
        let balls = sim.balls_world.clone();
        let near_slam = !sim.blasts.is_empty();
        let kratos = sim.body.pos;
        let mut broke = 0;
        for s in solids.items.iter_mut().filter(|s| s.alive && !s.fixed) {
            let (c, r, y0, y1) = s.cylinder();
            let touched = balls.iter().any(|(bc, br, _)| {
                let d = ((bc[0] - c[0]).powi(2) + (bc[2] - c[1]).powi(2)).sqrt();
                d < r + br && bc[1] > y0 - br && bc[1] < y1 + br
            }) || (near_slam && ((kratos[0] - c[0]).powi(2) + (kratos[2] - c[1]).powi(2)).sqrt() < 70.0 && (kratos[1] - y0).abs() < 40.0);
            if touched {
                s.alive = false;
                broke += 1;
                for e in &s.entities {
                    commands.entity(*e).insert(Visibility::Hidden);
                }
                sim.orbs += 3 + (s.entities.len() as u32 + (c[0].abs() as u32)) % 4;
                if let Some(b) = board.as_deref_mut() {
                    let name = if s.name.to_ascii_lowercase().contains("pot") || s.name.to_ascii_lowercase().contains("basket") || s.name.to_ascii_lowercase().contains("bag") { "SND_BREAKABLE_POT_M" } else { "SND_WOODBREAK" };
                    b.play(name, 0, &mut commands, &mut pcm);
                    b.play("SND_PICKUPS_ORBS_M", 0, &mut commands, &mut pcm);
                }
            }
        }
        if broke > 0 {
            shake(0.0, 0.5, 0.12);
        }
    }
}
fn render_pose(
    sim: Res<Sim>,
    hero: Option<Res<Hero>>,
    head: Option<Res<MedusaHead>>,
    mut visibility: Query<&mut Visibility>,
    blades: Option<ResMut<Blades>>,
    time: Res<Time>,
    mut gizmos: Gizmos,
    mut meshes: ResMut<Assets<Mesh>>,
    mut roots: Query<&mut Transform, Without<FollowCamera>>,
) {
    let Some(hero) = hero else { return };
    let layers: Vec<(&str, f32, f32)> = sim.layers.iter().map(|l| (l.clip, l.time, l.weight)).collect();
    // GOW_BIND=1 shows the bind pose (to tell holes in the model from holes made by the animation)
    let local = if std::env::var_os("GOW_BIND").is_some() { hero.skel.bind.clone() } else { hero.pose(&layers) };
    hero.apply(&local, &mut meshes);
    if let Some(head) = head {
        // GOW_HEADCLIP=<clip> shows the head beside Kratos playing that clip on a loop, to look at the model and its clips apart from the move
        let test = std::env::var("GOW_HEADCLIP").ok();
        let show = test.is_some() || sim.magic.head_visible;
        if let Ok(mut v) = visibility.get_mut(head.0.root) {
            *v = if show { Visibility::Inherited } else { Visibility::Hidden };
        }
        if show {
            // the head's clips carry Kratos's clip names, except that where Kratos has three aim members (`magMedusaIdle00` to `02`) the head has one clip (`magMedusaIdle`):
            // a Kratos layer is matched by its own name or by its name without the two digits, and layers that land on the same head clip add their weights
            // Kratos's loco layer (`magMedusaStrafeIdle`) and the aim members both name head clips; the head follows the aim members when there are any (they are the weight that sums to
            // one), because adding the two layers made the weights sum to two, which doubled the joint scales and blew the head up to 80 times its size
            let mut names: Vec<(String, f32, f32)> = Vec::new();
            let mut aim: Vec<(String, f32, f32)> = Vec::new();
            for l in &sim.layers {
                let stripped = l.clip.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
                let is_member = stripped != l.clip;
                let Some(n) = [l.clip.to_string(), stripped].into_iter().find(|n| head.0.clips.contains_key(n)) else { continue };
                let list = if is_member { &mut aim } else { &mut names };
                match list.iter_mut().find(|(m, _, _)| *m == n) {
                    Some(e) => e.2 += l.weight,
                    None => list.push((n, l.time, l.weight)),
                }
            }
            if !aim.is_empty() {
                names = aim;
            }
            let total: f32 = names.iter().map(|n| n.2).sum();
            if total > 1.0 {
                for n in &mut names {
                    n.2 /= total;
                }
            }            let looped: Vec<(String, f32, f32)> = match &test {
                Some(c) => {
                    let d = head.0.infos.get(c.as_str()).map_or(1.0, |i| i.duration.max(0.1));
                    vec![(c.clone(), time.elapsed_secs() % d, 1.0)]
                }
                None => names,
            };
            let layers: Vec<(&str, f32, f32)> = looped.iter().map(|(n, t, w)| (n.as_str(), *t, *w)).collect();
            let head_local = head.0.pose(&layers);
            head.0.apply(&head_local, &mut meshes);
            if std::env::var_os("GOW_LOGHEAD").is_some() && (time.elapsed_secs() as u32) % 2 == 0 && time.elapsed_secs().fract() < 0.03 {
                let posed = head.0.posed(&head_local);
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for p in &posed {
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                {
                    let kp = hero.posed(&local);
                    let (mut klo, mut khi) = ([f32::MAX; 3], [f32::MIN; 3]);
                    for p in &kp {
                        for k in 0..3 {
                            klo[k] = klo[k].min(p[k]);
                            khi[k] = khi[k].max(p[k]);
                        }
                    }
                    println!("kratos posed joints: x {:.0}..{:.0} y {:.0}..{:.0} z {:.0}..{:.0}", klo[0], khi[0], klo[1], khi[1], klo[2], khi[2]);
                }
                for (i, pc) in head.0.pieces.iter().enumerate() {
                    let (mut plo, mut phi) = ([f32::MAX; 3], [f32::MIN; 3]);
                    for &v in &pc.src {
                        let p = posed[v as usize];
                        for k in 0..3 {
                            plo[k] = plo[k].min(p[k]);
                            phi[k] = phi[k].max(p[k]);
                        }
                    }
                    println!("head piece {i}: {} verts, size {:.1} x {:.1} x {:.1}, centre {:.1} {:.1} {:.1}", pc.src.len(), phi[0] - plo[0], phi[1] - plo[1], phi[2] - plo[2], (phi[0] + plo[0]) / 2.0, (phi[1] + plo[1]) / 2.0, (phi[2] + plo[2]) / 2.0);
                }
                if let Some(h) = hero.skel.names.iter().position(|x| x.eq_ignore_ascii_case("head")) {
                    let w = gow2_skel::world_matrices(&hero.skel, &local);
                    println!("kratos head joint at {:.1} {:.1} {:.1}", w[h][12], w[h][13], w[h][14]);
                }
                let sc: Vec<String> = head_local.iter().take(8).map(|m| format!("{:.2}/{:.2}/{:.2}", m.s[0], m.s[1], m.s[2])).collect();
                println!("head posed: x {:.0}..{:.0} y {:.0}..{:.0} z {:.0}..{:.0} layers {:?} first joint scales {:?}", lo[0], hi[0], lo[1], hi[1], lo[2], hi[2], layers.iter().map(|l| l.0).collect::<Vec<_>>(), sc);
            }
            // The head is held in the right hand (`ATT_Medusa`: hand slot `RWeapIH`, free slot `RWeapOH`, snap 2 m). Its model faces -z with the hair up (the beam model runs along -z from
            // the eyes), and only `RWeapOH` has that orientation in the aim clips (its axes are the body's own: x right, y up, z back), while `RWeapIH` carries the forearm's axes (y along the
            // forearm), which would hold the face down and back. So the head sits at the hand joint's position with the free joint's rotation (MEDIUM: found by comparing the joint frames of
            // the clips, the game's own rule for the mode is not decoded).
            let find = |n: &str| hero.skel.names.iter().position(|x| x.eq_ignore_ascii_case(n));
            let world = gow2_skel::world_matrices(&hero.skel, &local);
            let kratos = Transform { translation: Vec3::from(sim.body.pos), rotation: Quat::from_rotation_y(sim.body.heading), scale: Vec3::ONE }.to_matrix();
            let head_transform = match (find("rWeapIH"), find("rWeapOH")) {
                _ if test.is_some() => Transform::from_translation(Vec3::from(sim.body.pos) + Vec3::new(0.0, 45.0, 0.0)),
                (Some(hand), Some(free)) => {
                    let mut m = world[free];
                    m[12..15].copy_from_slice(&world[hand][12..15]);
                    Transform::from_matrix(kratos * Mat4::from_cols_array(&m))
                }
                _ => Transform::from_matrix(kratos),
            };
            if let Ok(mut t) = roots.get_mut(head.0.root) {
                // the data holds no scale for the head (object scale 1, model scale 1); GOW_HEADSCALE=<factor> is for trying another size
                let k = std::env::var("GOW_HEADSCALE").ok().and_then(|s| s.parse::<f32>().ok()).unwrap_or(MEDUSA_HEAD_SCALE);
                *t = Transform { scale: Vec3::splat(k), ..head_transform };
            }
        }
    }
    for (e, t) in hero.filler_transforms(&local) {
        if let Ok(mut tr) = roots.get_mut(e) {
            *tr = t;
        }
    }
    let root = Transform { translation: Vec3::from(sim.body.pos), rotation: Quat::from_rotation_y(sim.body.heading), scale: Vec3::ONE };
    if let Ok(mut t) = roots.get_mut(hero.root) {
        *t = root;
    }
    if let Some(mut b) = blades {
        // the blades are stepped in the fixed tick (a stance keeps their mode); here they are shown with their chains and trails
        let mut sets: Vec<(Entity, Transform)> = Vec::new();
        b.draw(&root, time.elapsed_secs(), &mut meshes, &mut |e, t| sets.push((e, t)), &mut gizmos);
        // GOW_NOBLADES=1 hides the blades (to look at the model they cover)
        let hide = std::env::var_os("GOW_NOBLADES").is_some();
        for (e, t) in sets {
            if let Ok(mut tr) = roots.get_mut(e) {
                *tr = if hide { Transform::from_scale(Vec3::ZERO) } else { t };
            }
        }
    }
}

/// Draws the magic's effects: the visuals of MagicSys, and the bow in the left hand while the wind magic is up.
#[allow(clippy::too_many_arguments)]
fn magic_fx(
    mut sim: ResMut<Sim>,
    hero: Option<Res<Hero>>,
    fx: Option<Res<Fx>>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tr: Query<&mut Transform>,
    mut vis: Query<&mut Visibility>,
    mut gizmos: Gizmos,
    mut particles: Option<ResMut<gow2_bevy::particles::Particles>>,
    mut pfx_next: Local<f32>,
    blades: Option<Res<Blades>>,
) {
    // GOW_PFX=<effect>[@scale[@up]] starts the named effect (`goearthstomp`, `gomedusaflash`...) every two seconds 40 ahead and 70 to the side of Kratos; with @up the effect's
    // local y axis points up (default), `@fwd` makes it point along Kratos's facing
    if let (Ok(spec), Some(pt)) = (std::env::var("GOW_PFX"), particles.as_deref_mut()) {
        let t = time.elapsed_secs();
        if t > 3.0 && t >= *pfx_next {
            *pfx_next = t + 2.0;
            let parts: Vec<&str> = spec.split('@').collect();
            let scale = parts.get(1).and_then(|s| s.parse::<f32>().ok()).unwrap_or(1.0);
            let f = sim.body.facing();
            let at = Vec3::new(sim.body.pos[0] + f[0] * 40.0 - f[1] * 70.0, sim.body.pos[1] + 5.0, sim.body.pos[2] + f[1] * 40.0 + f[0] * 70.0);
            let root = if parts.get(2) == Some(&"fwd") { gow2_bevy::particles::root_look(at, Vec3::new(f[0], 0.0, f[1]), scale) } else { gow2_bevy::particles::root_up(at, Vec3::Y, scale) };
            let ok = pt.start(parts[0], root, None);
            println!("particles: started {} at {:.0?}: {ok}", parts[0], at);
        }
    }
    // the magic's particle effects: those that go off once, and those that follow a moving shot or core
    {
        let events = sim.magic.take_fx();
        let follow = sim.magic.following();
        if let Some(pt) = particles.as_deref_mut() {
            for e in events {
                pt.start(e.name, gow2_bevy::particles::place(e.name, Vec3::from(e.pos), Vec3::from(e.dir), e.scale), None);
            }
            let mut items: Vec<(u64, &str, gow2_skel::Mat4)> = follow.iter().map(|f| (f.key, f.name, gow2_bevy::particles::place(f.name, Vec3::from(f.pos), Vec3::from(f.dir), f.scale))).collect();
            // the lightning on Kratos's arms: three effects on the joints of both sides. Their curves run along the joint's +y axis for the length of the bone (5.5 for the
            // `golghtnshoulder` curve = humerus to radius, 6.9 for `golghtnelbow` = radius to wrist, `docs/particles.md` 7), so the effect's root is the joint's world matrix
            // (GOW_LBODY=1 keeps them on without casting, to look at them)
            if sim.magic.body_fx_on() || std::env::var_os("GOW_LBODY").is_some() {
                if let Some(h) = hero.as_deref() {
                    let layers: Vec<(&str, f32, f32)> = sim.layers.iter().map(|l| (l.clip, l.time, l.weight)).collect();
                    let world = gow2_skel::world_matrices(&h.skel, &h.pose(&layers));
                    let kratos = gow2_bevy::particles::root(Vec3::from(sim.body.pos), sim.body.heading, 1.0);
                    const ARMS: [(&str, [&str; 2]); 3] = [("golghtnshoulder", ["lHumerus", "rHumerus"]), ("golghtnelbow", ["lRadius", "rRadius"]), ("golghtnwrist", ["lWrist", "rWrist"])];
                    for (i, (name, joints)) in ARMS.iter().enumerate() {
                        for (side, jn) in joints.iter().enumerate() {
                            if let Some(j) = h.skel.names.iter().position(|x| x.eq_ignore_ascii_case(jn)) {
                                items.push((3u64 << 32 | (i * 2 + side) as u64, name, gow2_skel::mat_mul(&world[j], &kratos)));
                            }
                        }
                    }
                }
            }
            // the blades' glow and flames (`gomaiblade` of the weapon WAD, rig joint with the blade model's 1/64 scale): the cyan discs at the handle (`FXC_EG*`) are on all the time, as in
            // the PCSX2 frame of the opening; the flames on the blade (`FXC_BD*`) only while an attack move runs (MEDIUM: the reference videos show yellow fire during slashes and no
            // fire in the idle frame)
            let attacking = sim.moves.current().is_some_and(|(m, _)| m.anim.starts_with("att"));
            let mut with: Vec<(u64, &str, gow2_skel::Mat4, &[&str])> = items.iter().map(|&(k, n, m)| (k, n, m, &[] as &[&str])).collect();
            if let Some(b) = blades.as_deref() {
                let kratos = gow2_bevy::particles::root(Vec3::from(sim.body.pos), sim.body.heading, 1.0);
                for (i, it) in b.items.iter().enumerate() {
                    let m = gow2_skel::mat_mul(&it.blade.matrix, &kratos);
                    with.push((4u64 << 32 | i as u64, "gomaiblade", m, &["FXC_EG"]));
                    if attacking {
                        with.push((5u64 << 32 | i as u64, "gomaiblade", m, &["FXC_BD"]));
                    }
                }
            }
            pt.sync_follow_filtered(&with);

        }
    }
    let (Some(hero), Some(fx)) = (hero, fx) else { return };
    // the bow sits in the left hand like the head in the right: the hand joint's position with the free joint's rotation
    let bow = sim.magic.bow_visible.then(|| {
        let layers: Vec<(&str, f32, f32)> = sim.layers.iter().map(|l| (l.clip, l.time, l.weight)).collect();
        let world = gow2_skel::world_matrices(&hero.skel, &hero.pose(&layers));
        let find = |n: &str| hero.skel.names.iter().position(|x| x.eq_ignore_ascii_case(n));
        let kratos = Transform { translation: Vec3::from(sim.body.pos), rotation: Quat::from_rotation_y(sim.body.heading), scale: Vec3::ONE }.to_matrix();
        match (find("lWeapIH"), find("lWeapOH")) {
            (Some(h), Some(f)) => {
                let mut m = world[f];
                m[12..15].copy_from_slice(&world[h][12..15]);
                Transform::from_matrix(kratos * Mat4::from_cols_array(&m))
            }
            _ => Transform::from_matrix(kratos),
        }
    });
    let mut visuals = sim.magic.visuals();
    if std::env::var_os("GOW_LOGVIS").is_some() && !visuals.is_empty() && (time.elapsed_secs() * 4.0) as u32 != ((time.elapsed_secs() - time.delta_secs()) * 4.0) as u32 {
        println!("t {:.2}: visuals {:?}", time.elapsed_secs(), visuals.iter().map(|v| (v.kind, v.radius as i32)).collect::<Vec<_>>());
    }
    // GOW_FXTEST=beam|bomb|gust|tornado|tempest|rock|blast|bolt|core|earth|eblast|medblast|flash shows one effect in front of Kratos all the time, to look at the models and their scale
    if let Ok(kind) = std::env::var("GOW_FXTEST") {
        use gow2_kratos::magic::{Visual, VisualKind};
        let f = sim.body.facing();
        // 40 ahead and 70 to the side (the dummies stand straight ahead and would hide the effect)
        let p = [sim.body.pos[0] + f[0] * 40.0 - f[1] * 70.0, sim.body.pos[1] + 20.0, sim.body.pos[2] + f[1] * 40.0 + f[0] * 70.0];
        let far = [p[0] + f[0] * 200.0, p[1], p[2] + f[1] * 200.0];
        let k = match kind.as_str() {
            "beam" => VisualKind::MedusaBeam,
            "bomb" => VisualKind::MedusaBomb,
            "gust" => VisualKind::Gust,
            "tornado" => VisualKind::Tornado,
            "tempest" => VisualKind::Tempest,
            "rock" => VisualKind::Rock,
            "bolt" => VisualKind::Bolt,
            "core" => VisualKind::ElectricCore,
            "earth" => VisualKind::EarthBlast,
            "eblast" => VisualKind::ElectricBlast,
            "medblast" => VisualKind::MedusaBlast,
            "flash" => VisualKind::MedusaFlash,
            _ => VisualKind::Blast,
        };
        let t = time.elapsed_secs();
        let age = t % 1.0;
        // a red cross marks where the effect is placed
        gizmos.line(Vec3::from(p) - Vec3::X * 10.0, Vec3::from(p) + Vec3::X * 10.0, Color::srgb(1.0, 0.0, 0.0));
        gizmos.line(Vec3::from(p) - Vec3::Y * 10.0, Vec3::from(p) + Vec3::Y * 10.0, Color::srgb(1.0, 0.0, 0.0));
        let blast = matches!(k, VisualKind::Blast | VisualKind::EarthBlast | VisualKind::ElectricBlast | VisualKind::MedusaBlast);
        // the blasts carry their end radius and growth time in `to`; the flash its range in `radius`
        let (to, radius) = if blast { ([120.0, 0.0, 0.35], (age / 0.35).min(1.0) * 120.0) } else if k == VisualKind::MedusaFlash { (far, 240.0) } else { (far, 40.0) };
        visuals.push(Visual { kind: k, pos: p, to, age: if k == VisualKind::MedusaFlash { age * 0.5 } else { age }, life: 1.0, radius, tint: [0.7, 0.9, 0.6] });
    }
    fx::update(&fx, &visuals, sim.progress.selected_magic == 2, bow, time.elapsed_secs(), &mut meshes, &mut materials, &mut tr, &mut vis, &mut gizmos);
}

/// Shows the sub-weapon in Kratos's hand (hidden during the magic) and puts the chain blades away while one is held.
#[allow(clippy::too_many_arguments)]
fn sub_weapon_fx(
    sim: Res<Sim>,
    hero: Option<Res<Hero>>,
    subw: Option<Res<SubWeapons>>,
    blades: Option<Res<Blades>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tr: Query<&mut Transform>,
    mut vis: Query<&mut Visibility>,
) {
    let (Some(hero), Some(sw)) = (hero, subw) else { return };
    let casting = sim.moves.current().is_some_and(|(m, _)| m.anim.starts_with("mag"));
    let layers: Vec<(&str, f32, f32)> = sim.layers.iter().map(|l| (l.clip, l.time, l.weight)).collect();
    let root = Transform { translation: Vec3::from(sim.body.pos), rotation: Quat::from_rotation_y(sim.body.heading), scale: Vec3::ONE }.to_matrix();
    let world = gow2_skel::world_matrices(&hero.skel, &hero.pose(&layers));
    let ih = hero.skel.names.iter().position(|n| n.eq_ignore_ascii_case("rWeapIH"));
    for (i, item) in sw.items.iter().enumerate() {
        let shown = sim.progress.sub_weapon == Some(i as u8) && !casting;
        match ih {
            Some(j) => item.place(&root, &world[j], &layers, shown, &mut meshes, &mut tr, &mut vis),
            None => {
                if let Ok(mut v) = vis.get_mut(item.hero.root) {
                    *v = Visibility::Hidden;
                }
            }
        }
    }
    // the blades are not shown while a sub-weapon is held
    if let Some(b) = blades {
        for it in &b.items {
            if let Ok(mut v) = vis.get_mut(it.entity) {
                *v = if sim.progress.sub_weapon.is_some() { Visibility::Hidden } else { Visibility::Inherited };
            }
        }
    }
}

fn follow_camera(sim: Res<Sim>, ground: Option<Res<Ground>>, camw: Option<Res<CamWorld>>, mut cams: Query<(&FollowCamera, &mut Transform)>, mut frame: Local<u32>) {
    *frame += 1;
    for (c, mut t) in &mut cams {
        let target = Vec3::from(sim.body.pos) + Vec3::new(0.0, 22.0, 0.0);
        // camera collision: the collision polygons and the opaque render geometry both stop it. When the way back is short it tries steeper
        // angles (looking down over his shoulder) before it settles for sitting close, so it never ends up inside him.
        let hit_along = |dir: Vec3, dist: f32| -> Option<f32> {
            if std::env::var_os("GOW_NOCAMCOL").is_some() {
                return None;
            }
            let want = target + dir * dist;
            let a = ground.as_ref().and_then(|g| g.0.raycast(target.to_array(), want.to_array()));
            let b = camw.as_ref().and_then(|g| g.0.raycast(target.to_array(), want.to_array()));
            match (a, b) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            }
        };
        let mut best: Option<(f32, Vec3, bool)> = None;
        for lift in [0.0f32, 0.25, 0.5, 0.8, 1.1] {
            let pitch = (c.pitch + lift).min(1.45);
            let dir = Vec3::new(c.yaw.sin() * pitch.cos(), pitch.sin(), c.yaw.cos() * pitch.cos());
            let (reach, blocked) = match hit_along(dir, c.dist) {
                Some(f) => ((f * c.dist - 6.0).clamp(14.0f32.min(c.dist), c.dist), true),
                None => (c.dist, false),
            };
            if best.map_or(true, |b| reach > b.0 + 1e-3) {
                best = Some((reach, dir, blocked));
            }
            if reach >= c.dist * 0.8 {
                break;
            }
        }
        let (reach, dir, blocked) = best.unwrap();
        let want = target + dir * reach;        // pulling in is immediate (so the camera is never inside a wall), easing back out is smooth
        t.translation = if blocked || (want - target).length() < (t.translation - target).length() { want } else { t.translation.lerp(want, 0.2) };
        t.look_at(target, Vec3::Y);
        if std::env::var("GOW_CAMDEBUG").is_ok() && *frame % 120 == 0 {
            println!("camera at {:.0?} target {:.0?} yaw {:.2} pitch {:.2} dist {:.0} blocked {blocked}", t.translation, target, c.yaw, c.pitch, c.dist);
        }
    }
}

fn ground_grid(mut gizmos: Gizmos, sim: Res<Sim>, ground: Option<Res<Ground>>, has_level: Option<Res<HasLevel>>) {
    let (Some(ground), Some(has_level)) = (ground, has_level) else { return };
    // a ring on the floor under Kratos: it shrinks and fades as he rises, so jump height is easy to read
    let (px, py, pz) = (sim.body.pos[0], sim.body.pos[1], sim.body.pos[2]);
    let floor = ground.0.floor(px, pz, py + 1.0).unwrap_or(py);
    let h = (py - floor).max(0.0);
    let r = (9.0 - h * 0.05).max(4.0);
    let a = (0.9 - h * 0.006).clamp(0.25, 0.9);
    let c = Vec3::new(px, floor + 0.3, pz);
    let n = 24;
    for i in 0..n {
        let (t0, t1) = (i as f32 / n as f32 * std::f32::consts::TAU, (i + 1) as f32 / n as f32 * std::f32::consts::TAU);
        gizmos.line(c + Vec3::new(t0.cos() * r, 0.0, t0.sin() * r), c + Vec3::new(t1.cos() * r, 0.0, t1.sin() * r), Color::srgba(1.0, 0.85, 0.4, a));
    }
    // an arrow along the logical facing (the way attacks and walking go), to compare with the animation
    let fwd = sim.body.facing();
    let a = Vec3::new(px, floor + 1.0, pz);
    gizmos.arrow(a, a + Vec3::new(fwd[0], 0.0, fwd[1]) * 40.0, Color::srgb(1.0, 0.9, 0.2));
    if has_level.0 {
        return;
    }
    let step = 32.0;
    let cx = (px / step).round() * step;
    let cz = (pz / step).round() * step;
    let n = 24;
    for i in -n..=n {
        let o = i as f32 * step;
        let c = if i == 0 { Color::srgb(0.35, 0.4, 0.55) } else { Color::srgb(0.16, 0.18, 0.24) };
        gizmos.line(Vec3::new(cx + o, 0.0, cz - n as f32 * step), Vec3::new(cx + o, 0.0, cz + n as f32 * step), c);
        gizmos.line(Vec3::new(cx - n as f32 * step, 0.0, cz + o), Vec3::new(cx + n as f32 * step, 0.0, cz + o), c);
    }
}
/// F2 toggles the level's collision polygons as lines near Kratos: floors green, ceilings blue, walls orange, skipped surfaces grey.
fn collision_overlay(keys: Res<ButtonInput<KeyCode>>, overlay: Option<ResMut<Overlay>>, sim: Res<Sim>, mut gizmos: Gizmos) {
    let Some(mut o) = overlay else { return };
    if keys.just_pressed(KeyCode::F2) {
        o.on = !o.on;
    }
    if !o.on {
        return;
    }
    let me = Vec3::from(sim.body.pos);
    for (t, kind) in &o.tris {
        let c = (Vec3::from(t[0]) + Vec3::from(t[1]) + Vec3::from(t[2])) / 3.0;
        if (c - me).length() > 700.0 {
            continue;
        }
        let col = match kind {
            CollisionKind::Floor => Color::srgb(0.2, 0.95, 0.35),
            CollisionKind::Ceiling => Color::srgb(0.3, 0.55, 1.0),
            CollisionKind::Wall => Color::srgb(1.0, 0.55, 0.15),
            CollisionKind::Skipped => Color::srgba(0.6, 0.6, 0.7, 0.5),
        };
        for i in 0..3 {
            gizmos.line(Vec3::from(t[i]) + Vec3::Y * 0.5, Vec3::from(t[(i + 1) % 3]) + Vec3::Y * 0.5, col);
        }
    }
}

/// F3 draws the collision balls that attacks use: blades gold, fists red, feet blue, body green, throw grey, concussion spheres orange.
fn ball_overlay(keys: Res<ButtonInput<KeyCode>>, mut sim: ResMut<Sim>, mut gizmos: Gizmos) {
    if keys.just_pressed(KeyCode::F3) {
        sim.show_balls = !sim.show_balls;
    }
    if !sim.show_balls {
        return;
    }
    for &(c, r, id) in &sim.balls_world {
        let col = match id {
            2 | 3 => Color::srgb(1.0, 0.8, 0.2),
            4 | 5 => Color::srgb(1.0, 0.3, 0.3),
            6 | 7 => Color::srgb(0.3, 0.5, 1.0),
            1 => Color::srgb(0.3, 0.9, 0.4),
            8 => Color::srgb(1.0, 0.45, 0.1),
            _ => Color::srgb(0.7, 0.7, 0.7),
        };
        gizmos.sphere(Isometry3d::from_translation(Vec3::from(c)), r, col);
    }
}

/// Feeds the HUD from the simulation. Debug keys: H hurts Kratos (-10 health), M spends magic (-20), O adds 25 orbs, F4 hides the HUD.
fn hud_feed(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, time: Res<Time>, menu_clock: Local<f32>, mut menu_done: Local<bool>, mut sim: ResMut<Sim>, hud: Option<ResMut<Hud>>, board: Option<ResMut<SoundBoard>>) {
    if let Some(mut b) = board {
        if keys.just_pressed(KeyCode::F5) {
            b.volume = if b.volume > 0.0 { 0.0 } else { 0.8 };
        }
        // [ and ] lower and raise every sound by a semitone (for tuning the playback rates by ear)
        for (key, k) in [(KeyCode::BracketLeft, 2f32.powf(-1.0 / 12.0)), (KeyCode::BracketRight, 2f32.powf(1.0 / 12.0))] {
            if keys.just_pressed(key) {
                b.pitch *= k;
                println!("sound pitch x{:.3}", b.pitch);
            }
        }
    }
    // Tab or the d-pad left and right cycle the selected magic (the move branches test it: `BRA_*Enter` unlock codes 1, 2, 3, 6, 16)
    let cycle = if keys.just_pressed(KeyCode::Tab) || pads.iter().any(|p| p.just_pressed(GamepadButton::DPadRight)) {
        1
    } else if pads.iter().any(|p| p.just_pressed(GamepadButton::DPadLeft)) {
        MAGIC_IDS.len() - 1
    } else {
        0
    };
    if cycle != 0 && sim.menu == Menu::Closed {
        let at = MAGIC_IDS.iter().position(|&m| m == sim.progress.selected_magic).unwrap_or(0);
        sim.progress.selected_magic = MAGIC_IDS[(at + cycle) % MAGIC_IDS.len()];
        println!("selected magic id {}", sim.progress.selected_magic);
    }
    // B cycles the sub-weapon: none, Bone, Hammer, Olympus (the data's names); the moves of the weapon are chosen by Progress::sub_weapon (sub_weapon_ok)
    if keys.just_pressed(KeyCode::KeyB) && sim.menu == Menu::Closed {
        sim.progress.sub_weapon = match sim.progress.sub_weapon {
            None => Some(0),
            Some(i) if i < 2 => Some(i + 1),
            _ => None,
        };
        println!("sub-weapon: {}", sim.progress.sub_weapon.map_or("none", |i| subweapon::NAMES[i as usize]));
    }
    let Some(mut hud) = hud else { return };
    // GOW_MENU=pause|dead:<seconds>[:key,key,...] opens a menu after the delay and presses the keys (up, down, left, right) 1 s apart, to look at them without a controller
    if let Ok(spec) = std::env::var("GOW_MENU") {
        let f: Vec<&str> = spec.split(':').collect();
        let at: f32 = f.get(1).and_then(|s| s.parse().ok()).unwrap_or(3.0);
        let t = time.elapsed_secs() - at;
        if t > 0.0 && (t - *menu_clock).abs() < 10.0 && !*menu_done {
            *menu_done = true;
            match f[0] {
                "pause" => {
                    hud.set("PS2_PauseMenu_Event", 1.0);
                    hud.set("PS2_EnableButtons", 1.0);
                }
                "dead" => hud.set("PS2_DeadMenu_Event", 1.0),
                _ => {}
            }
        }
        if t > 0.0 && (t % 1.0) < time.delta_secs() {
            println!("t={t:.1} PauseMenu_State {} DeadMenu_State {} Pause_Check {} EnableButtons {}", hud.num("PauseMenu_State"), hud.num("DeadMenu_State"), hud.num("Pause_Check"), hud.num("PS2_EnableButtons"));
        }
        if let Some(keys_spec) = f.get(2) {
            for (i, k) in keys_spec.split(',').enumerate() {
                let when = 2.0 + i as f32;
                if t > when && t - time.delta_secs() <= when {
                    hud.call(match k {
                        "up" => "PressUp",
                        "down" => "PressDown",
                        "left" => "PressLeft",
                        _ => "PressRight",
                    });
                }
            }
        }
    }
    if keys.just_pressed(KeyCode::F6) || (std::env::var_os("GOW_DUMPHUD").is_some() && (time.elapsed_secs() % 3.0) < time.delta_secs()) {
        println!("--- HUD tree at {:.1}s\n{}", time.elapsed_secs(), hud.dump(std::env::var("GOW_DUMPHUD").ok().and_then(|v| v.parse().ok()).filter(|&d| d > 1).unwrap_or(4)));
    }
    // GOW_KILL=<seconds> takes all of Kratos's health at that moment (to see the death flow without a fight)
    if let Some(at) = std::env::var("GOW_KILL").ok().and_then(|v| v.parse::<f32>().ok()) {
        if time.elapsed_secs() > at && time.elapsed_secs() - time.delta_secs() <= at {
            sim.meters.adjust(0, -9999.0, false);
        }
    }
    // GOW_KILLENEMY=<seconds> strikes the first soldier dead at that moment and flings the second into the air, to look at the death and launch animations
    if let Some(at) = std::env::var("GOW_KILLENEMY").ok().and_then(|v| v.parse::<f32>().ok()) {
        if time.elapsed_secs() > at && time.elapsed_secs() - time.delta_secs() <= at {
            if let Some(v) = sim.victims.get_mut(0) {
                v.health = 0.0;
                v.since_hit = 0.0;
            }
            if let Some(v) = sim.victims.get_mut(1) {
                v.vy = 380.0;
                v.airborne = true;
                v.since_hit = 0.0;
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyH) {
        sim.meters.adjust(0, -10.0, false);
    }
    if keys.just_pressed(KeyCode::KeyM) {
        sim.meters.adjust(1, -20.0, false);
    }
    if keys.just_pressed(KeyCode::KeyO) {
        sim.orbs += 25;
    }
    if keys.just_pressed(KeyCode::F4) {
        hud.visible = !hud.visible;
    }
    // the HUD movie's own weapon slot (`SelWeapon`) shows the sub-weapon: `PS2_SubWeapon_Choice` 14 Bone, 15 Hammer, 17 Olympus (the values `PowerUpMenu:setWeapon` and `setSWIcon` test), with the `PS2_SubWeaponEnable_Event` code
    let choice = sim.progress.sub_weapon.map_or(0.0, |i| [14.0, 15.0, 17.0][i as usize % 3]);
    if (hud.num("PS2_SubWeapon_Choice") - choice).abs() > 0.5 {
        hud.set("PS2_SubWeapon_Choice", choice);
        hud.set("PS2_SubWeaponEnable_Event", 1.0);
    }
    // the upgrade menu spends the orbs itself (it counts `PS2_PowerOrb_Count` down) and raises the levels it shows
    if sim.menu == Menu::PowerUp {
        sim.orbs = hud.num("PS2_PowerOrb_Count").max(0.0) as u32;
        let best = ["Lightning", "Medusa", "Wind", "Electric", "Earth"].iter().map(|n| hud.num(&format!("PS2_{n}_Level")) as usize).max().unwrap_or(0);
        if best > sim.magic.level {
            sim.magic.level = best.min(2);
            println!("magic level {}", sim.magic.level);
        }
    }
    hud.menu = sim.menu != Menu::Closed;
    // the selected magic's picture in the HUD's big circle (frames of the movie's icon clip: 2 Lightning, 3 Wind, 4 Electric, 5 Medusa, 6 Earth)
    hud.magic_icon = match sim.progress.selected_magic {
        1 => Some(2),
        3 => Some(3),
        2 => Some(4),
        16 => Some(5),
        6 => Some(6),
        _ => None,
    };
    // the sandbox owns every sub-weapon (the weapon slot `setWeapon` shows only owned ones)
    for n in ["Hammer", "Bone", "Olympus"] {
        if hud.num(&format!("PS2_{n}_Status")) < 0.5 {
            hud.set(&format!("PS2_{n}_Status"), 1.0);
        }
    }
    let m = sim.meters;
    let enemy = sim.last_enemy.and_then(|id| sim.victims.iter().find(|v| v.id == id)).map(|v| (format!("DUMMY {}", v.id), v.health, v.max_health));
    hud.values = hud::HudValues {
        health: m.health,
        health_max: m.health_max,
        magic: m.magic,
        magic_max: m.magic_max,
        god: m.god,
        god_max: m.god_max,
        orbs: sim.orbs,
        hits: sim.combo,
        hit_age: sim.combo_age,
        enemy,
    };
}

/// The pause and dead menus of the HUD movie. The movie runs the menu itself (the highlight, the sounds' variables, the animation); the game opens it with the
/// `PS2_PauseMenu_Event` / `PS2_DeadMenu_Event` codes, presses the keys by calling the root frames `PressUp` and the like with `PS2_EnableButtons` set, and reads the
/// choice from `PauseMenu_State` (1 Continue, 2 Options, 3 Restart, 4 Quit) and `DeadMenu_State` (1 Continue, 2 Restart, 3 Quit). Which action each value means
/// is read from the message text on each line (`MSGS_TXT` 4010 to 4012); the confirm step the game asks for restart and quit is left out.
#[allow(clippy::too_many_arguments)]
fn menu_system(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    start: Res<Start>,
    mut sim: ResMut<Sim>,
    mut input: ResMut<PadInput>,
    hud: Option<ResMut<Hud>>,
    mut board: Option<ResMut<SoundBoard>>,
    mut pcm: ResMut<Assets<Pcm>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    mut stick_prev: Local<Vec2>,
    mut announced: Local<bool>,
    mut options_shown: Local<bool>,
) {
    let Some(mut hud) = hud else { return };
    let pad = pads.iter().next();
    let pressed = |b: GamepadButton| pad.is_some_and(|p| p.just_pressed(b));
    // direction presses: d-pad, arrows, and a flick of the left stick past 0.6
    let stick = pad.map_or(Vec2::ZERO, |p| p.left_stick());
    let flick = |now: f32, before: f32| now.abs() > 0.6 && before.abs() <= 0.6;
    let up = pressed(GamepadButton::DPadUp) || keys.just_pressed(KeyCode::ArrowUp) || (flick(stick.y, stick_prev.y) && stick.y > 0.0);
    let down = pressed(GamepadButton::DPadDown) || keys.just_pressed(KeyCode::ArrowDown) || (flick(stick.y, stick_prev.y) && stick.y < 0.0);
    let left = pressed(GamepadButton::DPadLeft) || keys.just_pressed(KeyCode::ArrowLeft) || (flick(stick.x, stick_prev.x) && stick.x < 0.0);
    let right = pressed(GamepadButton::DPadRight) || keys.just_pressed(KeyCode::ArrowRight) || (flick(stick.x, stick_prev.x) && stick.x > 0.0);
    *stick_prev = stick;
    let select = pressed(GamepadButton::South) || keys.just_pressed(KeyCode::Enter);
    let back = pressed(GamepadButton::East) || keys.just_pressed(KeyCode::Backspace);
    let toggle = pressed(GamepadButton::Start) || keys.just_pressed(KeyCode::Escape);
    // the upgrade menu: Select (the Create button) or U
    // (GOW_UPGRADE=<seconds> opens it at that time without input)
    let auto = std::env::var("GOW_UPGRADE").ok().and_then(|v| v.parse::<f32>().ok()).is_some_and(|at| time.elapsed_secs() > at && time.elapsed_secs() - time.delta_secs() <= at);
    let open_upgrade = pressed(GamepadButton::Select) || keys.just_pressed(KeyCode::KeyU) || auto;

    // dying: the movie shows its title at once, the choices once the fall is over
    if sim.dead_for > 0.0 && !*announced {
        *announced = true;
        hud.set("PS2_DeadMenu_Event", 1.0);
    }
    if sim.dead_for <= 0.0 && sim.menu != Menu::Dead {
        *announced = false;
    }
    let mut play = |name: &str| {
        if let Some(b) = board.as_deref_mut() {
            b.play(name, 0, &mut commands, &mut pcm);
        }
    };
    match sim.menu {
        Menu::Closed => {
            if toggle && sim.dead_for <= 0.0 {
                sim.menu = Menu::Pause;
                sim.menu_age = 0.0;
                hud.set("PS2_PauseMenu_Event", 1.0);
                hud.set("PS2_EnableButtons", 1.0);
            } else if open_upgrade && sim.dead_for <= 0.0 {
                sim.menu = Menu::PowerUp;
                sim.menu_age = 0.0;
                // what the menu shows: the unlock state, the level and the orbs put into each weapon and magic (`PS2_<name>_Status`, `_Level`, `Orb_Count`)
                let lvl = sim.magic.level as f64;
                for name in ["Lightning", "Medusa", "Wind", "Electric", "Earth", "Hammer", "Bone", "Olympus"] {
                    hud.set(&format!("PS2_{name}_Status"), 1.0);
                    hud.set(&format!("PS2_{name}_Level"), if matches!(name, "Hammer" | "Bone" | "Olympus") { 0.0 } else { lvl });
                    hud.set(&format!("PS2_{name}Orb_Count"), 0.0);
                }
                hud.set("PS2_Blades_Level", 2.0);
                hud.set("PS2_BladesOrb_Count", 0.0);
                hud.set("PS2_PSMMenu_Event", 1.0);
                hud.set("PS2_EnableButtons", 1.0);
            }
        }
        Menu::PowerUp => {
            sim.menu_age += time.delta_secs();
            if std::env::var_os("GOW_LOGMENU").is_some() && (sim.menu_age % 0.5) < time.delta_secs() {
                println!("upgrade menu {:.1}s: PSMPause_Check {} LevelUp_Check {} PSMDone {} orbs {} Wind level {} sel {} X {}", sim.menu_age, hud.num("PSMPause_Check"), hud.num("PowerUp_LevelUp_Check"), hud.num("PS2_PSMDone"), hud.num("PS2_PowerOrb_Count"), hud.num("PS2_Wind_Level"), hud.num("mSel"), hud.num("PauseXButton_Check"));
            }
            for (hit, label) in [(up, "PressUp"), (down, "PressDown"), (left, "PressLeft"), (right, "PressRight")] {
                if hit {
                    hud.call(label);
                }
            }
            // Cross held drains orbs into the highlighted item: the movie runs the fill from `PowerStart` while `PauseXButton_Check` is 1
            let mut x_down = pad.is_some_and(|p| p.pressed(GamepadButton::South)) || keys.pressed(KeyCode::Enter);
            // GOW_UPSEQ="2:down,3:down,4:x,6:xup" presses keys in the menu (seconds since it opened); `x` holds Cross until `xup`
            if let Ok(seq) = std::env::var("GOW_UPSEQ") {
                let mut held = false;
                for item in seq.split(',') {
                    let Some((at, key)) = item.split_once(':') else { continue };
                    let Ok(at) = at.parse::<f32>() else { continue };
                    if sim.menu_age < at {
                        continue;
                    }
                    let fresh = sim.menu_age - time.delta_secs() < at;
                    match key {
                        "x" => {
                            held = true;
                            if fresh {
                                hud.play("PowerUpMenu", "PowerStart");
                            }
                        }
                        "xup" => held = false,
                        "up" if fresh => hud.call("PressUp"),
                        "down" if fresh => hud.call("PressDown"),
                        "left" if fresh => hud.call("PressLeft"),
                        "right" if fresh => hud.call("PressRight"),
                        _ => {}
                    }
                }
                x_down |= held;
            }
            hud.set("PauseXButton_Check", if x_down { 1.0 } else { 0.0 });
            if select && sim.menu_age > 0.4 {
                hud.play("PowerUpMenu", "PowerStart");
            }
            // R1 and L1 (E and Q) change the page (`PressRight` and `PressLeft`)
            if pressed(GamepadButton::RightTrigger) || keys.just_pressed(KeyCode::KeyE) {
                hud.call("PressRight");
            }
            if pressed(GamepadButton::LeftTrigger) || keys.just_pressed(KeyCode::KeyQ) {
                hud.call("PressLeft");
            }
            // the movie reports that it has closed (`PS2_PSMDone`, set when its `aOff` ends: after the level-up screen, or after the close we ask for)
            let done = hud.num("PS2_PSMDone") > 0.5 && sim.menu_age > 0.6;
            let mut finish = false;
            if sim.upgrade_closing > 0.0 {
                sim.upgrade_closing -= time.delta_secs();
                // (a movie that did not report in time is put back by hand)
                finish = done || sim.upgrade_closing <= 0.0;
            } else if done {
                finish = true;
            } else if (toggle || back || open_upgrade || std::env::var("GOW_UPCLOSE").ok().and_then(|v| v.parse::<f32>().ok()).is_some_and(|at| sim.menu_age >= at)) && sim.menu_age > 0.4 {
                // closing is the movie's own `aOff` (it hides the pieces, resets itself and reports `PS2_PSMDone`); asking for the event 0 alone left the menu on screen
                hud.set("PS2_PauseMenu_Event", -1.0);
                hud.play("PowerUpMenu", "aOff");
                sim.upgrade_closing = 1.2;
                play("SND_MM_BACK");
            }
            if finish {
                hud.set("PS2_PSMDone", 0.0);
                hud.set("PS2_PSMMenu_Event", -1.0);
                hud.call("PauseReset");
                hud.stop_at("PowerUpMenu", 0);
                sim.upgrade_closing = 0.0;
                sim.menu = Menu::Closed;
                input.latched = 0;
                input.jump_latch = false;
            }
        }
        Menu::Pause => {
            sim.menu_age += time.delta_secs();
            for (hit, label) in [(up, "PressUp"), (down, "PressDown"), (left, "PressLeft"), (right, "PressRight")] {
                if hit {
                    hud.call(label);
                }
            }
            let state = hud.num("PauseMenu_State") as i32;
            if up || down || left || right || select || back {
                println!("pause menu: up {up} down {down} select {select} back {back}, choice {state}");
            }
            let close =toggle || back || (select && state == 1);
            if close && sim.menu_age > 0.4 {
                hud.call("ClosePause");
                hud.set("PS2_PauseMenu_Event", 0.0);
                sim.menu = Menu::Closed;
                input.latched = 0;
                input.jump_latch = false;
                play("SND_MM_BACK");
            } else if select && sim.menu_age > 0.4 {
                match state {
                    3 => {
                        respawn(&mut sim, &start);
                        hud.call("ClosePause");
                        hud.set("PS2_PauseMenu_Event", 0.0);
                        sim.menu = Menu::Closed;
                    }
                    4 => {
                        exit.write(AppExit::Success);
                    }
                    _ => {}
                }
            }
        }
        Menu::Dead => {
            sim.menu_age += time.delta_secs();
            if !*options_shown && sim.menu_age > 0.3 {
                *options_shown = true;
                hud.set("PS2_DeadMenu_Event", 2.0);
                hud.set("PS2_EnableButtons", 1.0);
            }
            if *options_shown {
                for (hit, label) in [(up, "PressUp"), (down, "PressDown")] {
                    if hit {
                        hud.call(label);
                    }
                }
                if select && sim.menu_age > 1.0 {
                    match hud.num("DeadMenu_State") as i32 {
                        3 => {
                            exit.write(AppExit::Success);
                        }
                        _ => {
                            hud.set("PS2_DeadMenu_Event", 0.0);
                            hud.set("PS2_EnableButtons", 0.0);
                            respawn(&mut sim, &start);
                            sim.menu = Menu::Closed;
                            *options_shown = false;
                            *announced = false;
                            input.latched = 0;
                            input.jump_latch = false;
                        }
                    }
                }
            }
        }
    }
}

/// Plays the sounds the HUD movie asked for (menu moves, pop-ups) through its `PS2_Sound1` to `PS2_Sound3` variables.
fn hud_sounds(hud: Option<ResMut<Hud>>, mut board: Option<ResMut<SoundBoard>>, mut pcm: ResMut<Assets<Pcm>>, mut commands: Commands) {
    let (Some(mut hud), Some(b)) = (hud, board.as_deref_mut()) else { return };
    for name in hud.take_sounds() {
        b.play(&name, 0, &mut commands, &mut pcm);
    }
}

fn title(sim: Res<Sim>, blades: Option<Res<Blades>>, input: Res<PadInput>, pads: Query<&Gamepad>, mut windows: Query<&mut Window>, mut tick: Local<u32>) {
    *tick += 1;
    if *tick % 10 != 0 {
        return;
    }
    let layers = sim.layers.iter().map(|l| format!("{} {:.0}%", l.clip, l.weight * 100.0)).collect::<Vec<_>>().join(", ");
    let pad = if pads.iter().next().is_some() { if input.from_pad { "pad" } else { "pad idle" } } else { "keyboard" };
    for mut w in &mut windows {
        let mv = sim.moves.current().map_or("-".to_string(), |(m, i)| format!("{} {:.2}", m.name.trim_start_matches("MOV_"), i.t()));
        let dummies = sim.victims.iter().map(|v| format!("{:.0}", v.health)).collect::<Vec<_>>().join("/");
        w.title = format!(
            "Kratos | {:.1} m/s | {:?} at {:.0} {:.0} {:.0} | magic {} | move {} | blades {} | dummies {} | hits {} | god {:.2} | {} | {}",
            sim.body.speed_mps(), sim.body.mode, sim.body.pos[0], sim.body.pos[1], sim.body.pos[2], sim.progress.selected_magic, mv, blades.as_ref().map_or("-".into(), |b| b.modes()), dummies, sim.hits, sim.meters.god, layers, pad
        );
    }
}



































































