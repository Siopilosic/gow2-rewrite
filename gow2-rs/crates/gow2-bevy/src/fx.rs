//! Drawing the magic: `gow2_kratos::magic::MagicSys::visuals` says what to show; [`update`] places pooled things for it each frame.
//!
//! The game builds most of its magic effects from particle systems, which are not ported; what is drawn here are the effect models that are plain geometry (`earthRock` of
//! `R_M_EARTH0`, `windBow` of `R_M_WIND0`) and simple shapes for the rest: jagged lines for bolts, translucent spheres for blasts and the electric core, a cylinder for the Medusa
//! beam, a streak for a wind gust, a cone for the tornado and the tempest. The models `MedusaBeam`, `windGust`, `WindTornado` and `WindTempest` were tried first, but their
//! animations scale them in ways that are not understood (the bind pose of `windGust` is 2,400 units long), so they stay unused for now.

use std::path::Path;

use bevy::prelude::*;
use gow2_kratos::magic::{Visual, VisualKind};

use crate::hero::{spawn_hero, Hero};

/// A model kept ready: hidden until a visual needs it.
pub struct Item {
    pub hero: Hero,
}

struct Prim {
    entity: Entity,
    material: Handle<StandardMaterial>,
}

struct PrimPool {
    items: Vec<Prim>,
}

impl PrimPool {
    fn new(commands: &mut Commands, mesh: Handle<Mesh>, materials: &mut Assets<StandardMaterial>, count: usize) -> Self {
        let items = (0..count)
            .map(|_| {
                let material = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, 0.0), unlit: true, alpha_mode: AlphaMode::Add, cull_mode: None, ..default() });
                let entity = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::default(), Visibility::Hidden)).id();
                Prim { entity, material }
            })
            .collect();
        PrimPool { items }
    }
}

#[derive(Resource)]
pub struct Fx {
    rock: Vec<Item>,
    /// The game's own effect models (`WindChargeGust`, `WindTornado`, `WindTempest`, `MedusaBeam`); GOW_FXPROC=1 draws the simple shapes instead.
    gust: Vec<Item>,
    tornado: Vec<Item>,
    tempest: Vec<Item>,
    beam: Vec<Item>,
    /// `lghtnSingleBolt` of `R_M_LGHTN2` and `electricBolt` of `R_M_ELCTRC0`: 29-joint bolts that run along -y, 233 units below their origin.
    lbolt: Vec<Item>,
    ebolt: Vec<Item>,
    /// `earthStomp` (the earth blasts: rock chunks flung outward), `lghtnMain` (the lightning column) and `lghtnRadius` (the ring a lightning bolt leaves on the ground). `medusaBomb` and `medusaBombHit` are black proxy spheres (vertex colour 0, material `MAT_transp`/`lambert1New`): the game draws the bomb with particles, so the bomb and its blast stay shapes.
    stomp: Vec<Item>,
    lmain: Vec<Item>,
    lring: Vec<Item>,
    pub bow: Option<Item>,
    sphere: PrimPool,
    /// A cylinder of radius 1 along -z from 0 to -1 (beam, gust).
    cylinder: PrimPool,
    /// A cone of radius 1 and height 1 standing on y = 0 with its point at the top (tornado, tempest).
    cone: PrimPool,
}

fn load(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, wad: &Path, model: &str, count: usize) -> Vec<Item> {
    if !wad.exists() {
        return Vec::new();
    }
    let path = wad.to_string_lossy().to_string();
    (0..count)
        .map(|_| {
            let hero = spawn_hero(commands, meshes, materials, images, &path, model, &["*"]);
            commands.entity(hero.root).insert(Visibility::Hidden);
            Item { hero }
        })
        .collect()
}

/// Loads the effect objects from the WADs next to the hero WAD.
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, dir: &Path) -> Fx {
    let rock = load(commands, meshes, materials, images, &dir.join("R_M_EARTH0.WAD"), "earthRock", 10);
    let bow = load(commands, meshes, materials, images, &dir.join("R_M_WIND0.WAD"), "windBow", 1).pop();
    let gust = load(commands, meshes, materials, images, &dir.join("R_M_WIND0.WAD"), "WindChargeGust", 3);
    let tornado = load(commands, meshes, materials, images, &dir.join("R_M_WIND0.WAD"), "WindTornado", 2);
    let tempest = load(commands, meshes, materials, images, &dir.join("R_M_WIND0.WAD"), "WindTempest", 1);
    let beam = load(commands, meshes, materials, images, &dir.join("R_M_MEDUSA0.WAD"), "MedusaBeam", 1);

    let lbolt = load(commands, meshes, materials, images, &dir.join("R_M_LGHTN2.WAD"), "lghtnSingleBolt", 6);
    let ebolt = load(commands, meshes, materials, images, &dir.join("R_M_ELCTRC0.WAD"), "electricBolt", 16);
    let stomp = load(commands, meshes, materials, images, &dir.join("R_M_EARTH0.WAD"), "earthStomp", 4);
    let lmain = load(commands, meshes, materials, images, &dir.join("R_M_LGHTN2.WAD"), "lghtnMain", 4);
    let lring = load(commands, meshes, materials, images, &dir.join("R_M_LGHTN2.WAD"), "lghtnRadius", 6);
    let sphere = PrimPool::new(commands, meshes.add(Sphere::new(1.0).mesh().uv(20, 12)), materials, 20);
    // a cylinder along y from -0.5 to 0.5, turned to run along -z from 0 to -1
    let cyl = Cylinder::new(1.0, 1.0).mesh().resolution(16).build().rotated_by(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).translated_by(Vec3::new(0.0, 0.0, -0.5));
    let cylinder = PrimPool::new(commands, meshes.add(cyl), materials, 14);
    let cone_mesh = Cone { radius: 1.0, height: 1.0 }.mesh().resolution(20).build().translated_by(Vec3::new(0.0, 0.5, 0.0));
    let cone = PrimPool::new(commands, meshes.add(cone_mesh), materials, 6);
    println!("magic effects: rocks {} bow {}", rock.len(), bow.is_some());
    Fx { rock, gust, tornado, tempest, beam, lbolt, ebolt, stomp, lmain, lring, bow, sphere, cylinder, cone }
}

fn place(item: &Item, t: Transform, clip_time: f32, shown: bool, meshes: &mut Assets<Mesh>, tr: &mut Query<&mut Transform>, vis: &mut Query<&mut Visibility>) {
    if let Ok(mut v) = vis.get_mut(item.hero.root) {
        *v = if shown { Visibility::Inherited } else { Visibility::Hidden };
    }
    if !shown {
        return;
    }
    let dur = item.hero.infos.get("*").map_or(1.0, |i| i.duration.max(0.05));
    let local = item.hero.pose(&[("*", clip_time % dur, 1.0)]);
    item.hero.apply(&local, meshes);
    if std::env::var_os("GOW_LOGFX2").is_some() {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        if N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 60 == 0 {
            let p = item.hero.posed(&local);
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &p {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            println!("fx place: clips {} dur {dur:.2} t {clip_time:.2} translation {:.0?} scale {:.2?} bounds {lo:.0?}..{hi:.0?}", item.hero.clips.len(), t.translation, t.scale);
        }
    }
    if let Ok(mut x) = tr.get_mut(item.hero.root) {
        *x = t;
    }
}

/// A jagged line from `a` to `b` (lightning), redrawn each frame with a seed from the time so it crackles.
fn bolt(gizmos: &mut Gizmos, a: Vec3, b: Vec3, seed: f32, colour: Color) {
    let d = b - a;
    let len = d.length().max(1.0);
    let dir = d / len;
    let side = dir.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
    let up = side.cross(dir);
    let segs = ((len / 12.0) as usize).clamp(4, 40);
    let mut prev = a;
    for i in 1..=segs {
        let t = i as f32 / segs as f32;
        let wob = if i == segs { 0.5 } else { ((seed * 12.9898 + t * 78.233).sin() * 43758.5453).fract().abs() };
        let wob2 = ((seed * 4.1414 + t * 31.7).sin() * 12345.678).fract().abs();
        let amp = 6.0 * (1.0 - (2.0 * t - 1.0).abs());
        let p = a + d * t + side * (wob - 0.5) * 2.0 * amp + up * (wob2 - 0.5) * 2.0 * amp;
        gizmos.line(prev, p, colour);
        // a second and third pass a little to the side thicken it
        gizmos.line(prev + side * 0.7, p + side * 0.7, colour);
        gizmos.line(prev + up * 0.7, p + up * 0.7, colour);
        prev = p;
    }
}

#[allow(clippy::too_many_arguments)]
fn show(pool: &PrimPool, n: &mut usize, t: Transform, colour: Color, blend: AlphaMode, materials: &mut Assets<StandardMaterial>, tr: &mut Query<&mut Transform>, vis: &mut Query<&mut Visibility>) {
    let Some(p) = pool.items.get(*n) else { return };
    *n += 1;
    if let Some(mut m) = materials.get_mut(&p.material) {
        m.base_color = colour;
        m.alpha_mode = blend;
    }
    if let Ok(mut x) = tr.get_mut(p.entity) {
        *x = t;
    }
    if let Ok(mut v) = vis.get_mut(p.entity) {
        *v = Visibility::Inherited;
    }
}

fn hide_rest(pool: &PrimPool, used: usize, vis: &mut Query<&mut Visibility>) {
    for p in pool.items.iter().skip(used) {
        if let Ok(mut v) = vis.get_mut(p.entity) {
            *v = Visibility::Hidden;
        }
    }
}

/// Shows the visuals of this frame; `bow` is where the bow sits when it is shown.
#[allow(clippy::too_many_arguments)]
pub fn update(fx: &Fx, visuals: &[Visual], electric: bool, bow: Option<Transform>, now: f32, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, tr: &mut Query<&mut Transform>, vis: &mut Query<&mut Visibility>, gizmos: &mut Gizmos) {
    let (mut spheres, mut cylinders, mut cones, mut rocks) = (0usize, 0usize, 0usize, 0usize);
    let (mut gusts, mut tornados, mut tempests, mut beams) = (0usize, 0usize, 0usize, 0usize);
    // bolt models in use: lightning bolts, electric bolts (arcs and the stars of the electric blasts), and the other new model pools
    let (mut lbolts, mut ebolts, mut stomps, mut lmains, mut lrings) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let proc_only = std::env::var_os("GOW_FXPROC").is_some();
    for v in visuals {
        let p = Vec3::from(v.pos);
        let to = Vec3::from(v.to);
        let dir = (to - p).try_normalize().unwrap_or(Vec3::NEG_Z);
        let look = |len: f32, radius: f32| Transform::from_translation(p).looking_to(dir, Vec3::Y).with_scale(Vec3::new(radius, radius, len));
        let fade = (1.0 - v.age / v.life.max(1e-3)).clamp(0.0, 1.0);
        match v.kind {
            VisualKind::Bolt if !proc_only && (if electric { fx.ebolt.get(ebolts) } else { fx.lbolt.get(lbolts) }).is_some() => {
                // the bolt model hangs 233 units down its -y axis from the origin: turned so that -y points at the target and stretched to the distance
                let len = (to - p).length().max(1.0);
                let rot = Quat::from_rotation_arc(Vec3::NEG_Y, dir);
                let t = Transform::from_translation(p).with_rotation(rot).with_scale(Vec3::new(0.6, len / 233.0, 0.6));
                if electric {
                    place(&fx.ebolt[ebolts], t, v.age, true, meshes, tr, vis);
                    ebolts += 1;
                } else {
                    place(&fx.lbolt[lbolts], t, v.age, true, meshes, tr, vis);
                    lbolts += 1;
                    // `lghtnMain`, the strike column (448 tall, y up from the target), stands where it lands
                    if let Some(col) = fx.lmain.get(lmains) {
                        place(col, Transform::from_translation(to).with_scale(Vec3::splat(0.5)), v.age, true, meshes, tr, vis);
                        lmains += 1;
                    }
                    // where a lightning bolt lands, `lghtnRadius` (a flat ring 42 across) opens on the ground and fades with the bolt
                    if let Some(ring) = fx.lring.get(lrings) {
                        let grow = 0.6 + 1.6 * (v.age / v.life.max(1e-3)).clamp(0.0, 1.0);
                        place(ring, Transform::from_translation(to).with_scale(Vec3::splat(grow)), v.age, true, meshes, tr, vis);
                        lrings += 1;
                    }
                }
            }
            VisualKind::EarthBlast if !proc_only && fx.stomp.get(stomps).is_some() => {
                // `earthStomp`: its own clip (0.63 s) takes it from 33 to 256 units wide; the clip is paced to the blast's growth and the model scaled to the blast's end radius
                let (end, grow) = (v.to[0].max(1.0), v.to[2].max(0.05));
                let clip = (v.age / grow).clamp(0.0, 1.0) * 0.62;
                place(&fx.stomp[stomps], Transform::from_translation(p).with_scale(Vec3::splat(end / 256.0)), clip, true, meshes, tr, vis);
                stomps += 1;
            }
            VisualKind::ElectricBlast if !proc_only && fx.ebolt.len() > ebolts + 6 => {
                // no model of its own: a star of the electric bolts, the length of the blast's radius
                let seed = (v.pos[0] * 0.37 + v.pos[2] * 0.11).sin();
                for k in 0..7 {
                    let a = k as f32 * 0.8976 + seed * 3.0;
                    let up = ((k * 5 % 7) as f32 / 6.0 - 0.3).clamp(-0.3, 0.7);
                    let d = Vec3::new(a.cos(), up, a.sin()).normalize();
                    let rot = Quat::from_rotation_arc(Vec3::NEG_Y, d);
                    place(&fx.ebolt[ebolts], Transform::from_translation(p).with_rotation(rot).with_scale(Vec3::new(0.5, (v.radius / 233.0).max(0.02), 0.5)), v.age + k as f32 * 0.07, true, meshes, tr, vis);
                    ebolts += 1;
                }
            }
            VisualKind::Bolt => {
                bolt(gizmos, p, to, now * 40.0 + v.pos[0], Color::srgba(0.7, 0.85, 1.0, fade));
                bolt(gizmos, p, to, now * 53.0 + v.pos[2] + 7.0, Color::srgba(1.0, 1.0, 1.0, fade));
            }
            VisualKind::ElectricCore => {
                // `goelectriccore` follows the core as particles
                if !proc_only {
                    continue;
                }
                let r = v.radius * (0.8 + 0.2 * (now * 40.0).sin());
                show(&fx.sphere, &mut spheres, Transform::from_translation(p).with_scale(Vec3::splat(r)), Color::srgba(0.6, 0.8, 1.0, 0.9), AlphaMode::Add, materials, tr, vis);
            }
            VisualKind::Blast | VisualKind::EarthBlast | VisualKind::ElectricBlast | VisualKind::MedusaBlast => {
                // the blasts are drawn by `particles` (the wind's blow has `gowindblowhit`); the sphere stays for GOW_FXPROC=1 only
                if !proc_only {
                    continue;
                }
                let c = v.tint;
                show(&fx.sphere, &mut spheres, Transform::from_translation(p).with_scale(Vec3::splat(v.radius.max(0.5))), Color::srgba(c[0], c[1], c[2], 0.45 * fade), AlphaMode::Add, materials, tr, vis);
            }
            VisualKind::Rock => {
                if let Some(it) = fx.rock.get(rocks) {
                    rocks += 1;
                    place(it, Transform::from_translation(p).with_rotation(Quat::from_rotation_y(v.age * 5.0) * Quat::from_rotation_x(v.age * 3.0)).with_scale(Vec3::splat(0.5)), v.age, true, meshes, tr, vis);
                }
            }
            VisualKind::Gust if !proc_only && fx.gust.get(gusts).is_some() => {
                // `WindChargeGust` runs from its origin along -z for 204 units, 22 to 30 across; its clip swells it over a second
                place(&fx.gust[gusts], Transform::from_translation(p).looking_to(dir, Vec3::Y), v.age, true, meshes, tr, vis);
                gusts += 1;
            }
            VisualKind::Tornado if !proc_only && fx.tornado.get(tornados).is_some() => {
                // `WindTornado`: about 100 across and 75 tall at its own size; scaled to the radius the magic uses
                place(&fx.tornado[tornados], Transform::from_translation(p).with_rotation(Quat::from_rotation_y(v.age * 2.0)).with_scale(Vec3::splat(v.radius / 50.0)), now, true, meshes, tr, vis);
                tornados += 1;
            }
            VisualKind::Tempest if !proc_only && fx.tempest.get(tempests).is_some() => {
                // `WindTempest`: 676 across and 445 tall at its own size
                place(&fx.tempest[tempests], Transform::from_translation(p).with_scale(Vec3::splat(v.radius / 338.0)), now, true, meshes, tr, vis);
                tempests += 1;
            }
            VisualKind::MedusaBeam if !proc_only && fx.beam.get(beams).is_some() => {
                // `MedusaBeam`: 402 long along -z, about 85 across at its own size
                let len = (to - p).length().max(1.0);
                let width = (v.radius / 30.0).max(0.2);
                place(&fx.beam[beams], Transform::from_translation(p).looking_to(dir, Vec3::Y).with_scale(Vec3::new(width, width, len / 402.0)), now, true, meshes, tr, vis);
                beams += 1;
            }
            VisualKind::Gust => {
                show(&fx.cylinder, &mut cylinders, look(40.0, 4.0), Color::srgba(0.8, 0.9, 1.0, 0.6), AlphaMode::Add, materials, tr, vis);
            }
            VisualKind::Tornado | VisualKind::Tempest => {
                let (h, col) = if v.kind == VisualKind::Tempest { (v.radius * 2.2, Color::srgba(0.7, 0.8, 0.9, 0.35)) } else { (v.radius * 2.6, Color::srgba(0.75, 0.85, 0.95, 0.4)) };
                // the cone is turned over so that it is wide at the top, as the wind column looks; turning it moves its base to the top, so it is lifted by its height
                let spin = Quat::from_rotation_y(v.age * 6.0);
                let t = Transform { translation: p + Vec3::Y * h, rotation: spin * Quat::from_rotation_x(std::f32::consts::PI), scale: Vec3::new(v.radius, h, v.radius) };
                show(&fx.cone, &mut cones, t, col, AlphaMode::Blend, materials, tr, vis);
            }
            VisualKind::MedusaBeam => {
                let len = (to - p).length();
                let pulse = 0.85 + 0.15 * (now * 30.0).sin();
                show(&fx.cylinder, &mut cylinders, look(len, v.radius * pulse), Color::srgba(0.45, 1.0, 0.55, 0.35), AlphaMode::Add, materials, tr, vis);
                show(&fx.cylinder, &mut cylinders, look(len, v.radius * 0.35), Color::srgba(1.0, 1.0, 1.0, 0.7), AlphaMode::Add, materials, tr, vis);
            }
            VisualKind::MedusaBomb => {
                // the bomb has no visible model (see `Fx`), the effect `gomedusabomb` follows it
                if !proc_only {
                    continue;
                }
                show(&fx.sphere, &mut spheres, Transform::from_translation(p).with_scale(Vec3::splat(v.radius)), Color::srgba(0.5, 1.0, 0.5, 0.9), AlphaMode::Add, materials, tr, vis);
            }
            VisualKind::MedusaFlash => {
                // the particle effect `gomedusaflash` is the flash; the cone is the stand-in for GOW_FXPROC=1
                if !proc_only {
                    continue;
                }
                // a cone opening from the chest along the aim to the flash's range (no model: the game's flash is made of particles), quick to open, fading out
                let open = (v.age / 0.15).clamp(0.0, 1.0);
                let base = v.radius * open;
                let t = Transform { translation: p + dir * base, rotation: Quat::from_rotation_arc(Vec3::Y, -dir), scale: Vec3::new(base * 0.9, base.max(1.0), base * 0.9) };
                show(&fx.cone, &mut cones, t, Color::srgba(1.0, 0.95, 0.6, 0.4 * fade), AlphaMode::Add, materials, tr, vis);
            }
        }
    }
    hide_rest(&fx.sphere, spheres, vis);
    hide_rest(&fx.cylinder, cylinders, vis);
    hide_rest(&fx.cone, cones, vis);
    for (pool, used) in [(&fx.rock, rocks), (&fx.gust, gusts), (&fx.tornado, tornados), (&fx.tempest, tempests), (&fx.beam, beams), (&fx.lbolt, lbolts), (&fx.ebolt, ebolts), (&fx.stomp, stomps), (&fx.lmain, lmains), (&fx.lring, lrings)] {
        for it in pool.iter().skip(used) {
            if let Ok(mut x) = vis.get_mut(it.hero.root) {
                *x = Visibility::Hidden;
            }
        }
    }
    if let Some(b) = &fx.bow {
        place(b, bow.unwrap_or_default(), now, bow.is_some(), meshes, tr, vis);
    }
}
