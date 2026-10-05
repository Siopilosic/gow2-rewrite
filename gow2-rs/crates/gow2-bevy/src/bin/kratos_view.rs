//! Playtest 1/2: Kratos from the original WAD, in bind pose, with named clips played through our own skinning.
//!
//! ```text
//! cargo run -p gow2-bevy --bin kratos-view -- extracted/pak/R_HERO00.WAD
//! ```
//! Mouse drag orbits, wheel zooms, Space pauses, `[` and `]` change clip, `0` returns to the bind pose.
//! Everything is decoded from the user's own WAD at start-up; no game data is bundled.

use std::collections::BTreeMap;

use bevy::{
    asset::RenderAssetUsages,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use gow2_formats::{mdl, skin, texture::TextureStore, wad};
use gow2_skel::{local_parts, skin_matrices, skin_positions_blend, world_matrices, Clip, Delta, Fill, Skeleton};

/// One drawable piece: the vertices of one material slot, remapped from the model's vertex list.
struct Piece {
    mesh: Handle<Mesh>,
    src: Vec<u32>,
    mat: Handle<StandardMaterial>,
    tex: Option<Handle<Image>>,
    /// Vertex colours as decoded, and the same coloured by joint (J).
    cols: Vec<[f32; 4]>,
    joint_cols: Vec<[f32; 4]>,
}

#[derive(Resource)]
struct Character {
    skel: Skeleton,
    bind_pos: Vec<[f32; 3]>,
    joints: Vec<i64>,
    /// Per vertex: the vertex is joint-local and goes through its joint's world matrix.
    local: Vec<bool>,
    /// Second joint and its pull for the two-joint blend; `B` turns the blend off to compare with rigid skinning.
    joints2: Vec<i64>,
    weight: Vec<f32>,
    blend: bool,
    /// Undo the whole-body yaw of the clip so the pose faces the camera (Y).
    face_front: bool,
    pieces: Vec<Piece>,
    clips: Vec<(String, Clip)>,
    current: Option<usize>,
    time: f32,
    paused: bool,
    center: Vec3,
    radius: f32,
    /// Playback speed multiplier (- halves, = doubles).
    speed: f32,
    /// Keep the pelvis in place horizontally so the pose can be read without root motion (R).
    pin_root: bool,
    /// The camera follows the skinned model's bounds centre (F).
    follow: bool,
    /// Paint vertices by joint instead of texture (J).
    joint_view: bool,
    /// Hide the vertices of the skirt and cloth joints (K), to see whether the loose strips belong to them.
    hide_skirt: bool,
    /// Bounds centre of the skinned model this frame.
    focus: Vec3,
    /// Current frame number of the playing clip, for the title.
    frame: usize,
}

#[derive(Component)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    dist: f32,
    target: Vec3,
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "extracted/pak/R_HERO00.WAD".into());
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Kratos".into(), ..default() }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.08, 0.08, 0.1)))
        .insert_resource(WadPath(path))
        .insert_resource(StartState { clip: std::env::args().nth(2), time: std::env::args().nth(3).and_then(|t| t.parse().ok()) })
        .add_systems(Startup, setup)
        .add_systems(Update, (orbit_camera, controls, animate).chain())
        .run();
}

#[derive(Resource)]
struct WadPath(String);

/// Optional start state from the command line: `kratos-view <WAD> [clip name] [time in seconds]`. A given time starts paused.
#[derive(Resource)]
struct StartState {
    clip: Option<String>,
    time: Option<f32>,
}

fn rgba_image(t: &gow2_formats::texture::Texture) -> Image {
    gow2_bevy::gpu::texture_image(t, true)
}

fn setup(
    mut commands: Commands,
    path: Res<WadPath>,
    start: Res<StartState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let model = std::env::var("GOW_MODEL").unwrap_or_else(|_| "hero".into());
    let data = std::fs::read(&path.0).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.0));
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let rr = skin::find_rigs(&recs)
        .into_iter()
        .find(|r| r.model == model)
        .unwrap_or_else(|| panic!("no rig for model {model} in this WAD (set GOW_MODEL, default hero)"));
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let first: BTreeMap<&str, &[u8]> = recs.iter().filter(|r| r.tag == wad::Tag::Object && !r.data.is_empty()).fold(
        BTreeMap::new(),
        |mut m, r| {
            m.entry(r.name.as_str()).or_insert(r.data);
            m
        },
    );
    let blob = first[format!("MDL_{model}_0").as_str()];
    let sm = skin::mesh_joints(blob, 4096.0);
    // parts modelled around the origin (armour plates) are joint-local; see gow2_skel::local_parts
    let part_local = local_parts(&sm, &skel);
    let vertex_local: Vec<bool> = sm.part.iter().map(|&p| part_local[p as usize]).collect();
    println!("joint-local parts: {:?}", part_local.iter().enumerate().filter(|(_, &l)| l).map(|(i, _)| i).collect::<Vec<_>>());
    let mat_names = mdl::model_materials(&recs, &model);
    let store = TextureStore::new(recs.iter().filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)));

    // positions are bind-space, 1/16 game units (docs/models.md)
    let bind_pos: Vec<[f32; 3]> = sm.verts.iter().map(|v| [v[0] as f32 / 16.0, v[1] as f32 / 16.0, v[2] as f32 / 16.0]).collect();

    let mut slots: BTreeMap<u16, Vec<[u32; 3]>> = BTreeMap::new();
    for (t, s) in &sm.tris {
        slots.entry(*s).or_default().push(*t);
    }
    let mut pieces = Vec::new();
    // debugging aid: GOW_HIDE=B skips every material whose name ends with B (the second variant of each material)
    let hide_suffix = std::env::var("GOW_HIDE").ok();
    for (slot, tris) in slots {
        if let (Some(suf), Some(name)) = (&hide_suffix, mat_names.get(slot as usize)) {
            if name.ends_with(suf.as_str()) {
                continue;
            }
        }
        let mut remap: BTreeMap<u32, u32> = BTreeMap::new();
        let mut src = Vec::new();
        let mut idx = Vec::new();
        for t in &tris {
            for &v in t {
                let n = remap.len() as u32;
                let id = *remap.entry(v).or_insert_with(|| {
                    src.push(v);
                    n
                });
                idx.push(id);
            }
        }
        let uv: Vec<[f32; 2]> = src.iter().map(|&v| [sm.uvs[v as usize][0] as f32, sm.uvs[v as usize][1] as f32]).collect();
        let col: Vec<[f32; 4]> = src
            .iter()
            .map(|&v| {
                let c = sm.cols[v as usize];
                [c[0].min(128) as f32 / 128.0, c[1].min(128) as f32 / 128.0, c[2].min(128) as f32 / 128.0, c[3].min(128) as f32 / 128.0]
            })
            .collect();
        let pos: Vec<[f32; 3]> = src.iter().map(|&v| bind_pos[v as usize]).collect();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col.clone());
        mesh.insert_indices(Indices::U32(idx));
        let mut mat = StandardMaterial { unlit: true, cull_mode: None, perceptual_roughness: 1.0, ..default() };
        if let Some(t) = mat_names.get(slot as usize).and_then(|m| store.material_texture(m)) {
            if t.rgba.chunks(4).any(|p| p[3] < 255) {
                mat.alpha_mode = AlphaMode::Mask(0.5);
            }
            mat.base_color_texture = Some(images.add(rgba_image(&t)));
        }
        let tex = mat.base_color_texture.clone();
        let joint_cols: Vec<[f32; 4]> = src
            .iter()
            .map(|&v| {
                let j = sm.joints[v as usize] as f32;
                let h = (j * 137.508) % 360.0;
                let c = Color::hsl(h, 0.85, 0.5).to_srgba();
                [c.red, c.green, c.blue, 1.0]
            })
            .collect();
        let handle = meshes.add(mesh);
        let mat_handle = materials.add(mat);
        commands.spawn((Mesh3d(handle.clone()), MeshMaterial3d(mat_handle.clone()), Transform::default()));
        pieces.push(Piece { mesh: handle, src, mat: mat_handle, tex, cols: col, joint_cols });
    }

    // named clips of the character ANM with real channels
    let mut clips: Vec<(String, Clip)> = Vec::new();
    if let Some(anm) = rr.anm.as_deref().and_then(|n| first.get(n)) {
        for c in gow2_formats::anm::clips(anm) {
            let nm = skin::clip_name(anm, c);
            if nm.is_empty() || clips.iter().any(|(n, _)| *n == nm) {
                continue;
            }
            // junk entries (names like "TTTT", "42") fail to decode almost everywhere; real clips decode cleanly
            let broken = gow2_formats::anm::decode_clip(anm, c, &[gow2_formats::anm::Kind::Rot, gow2_formats::anm::Kind::Trans, gow2_formats::anm::Kind::Scale])
                .map_or(true, |bl| bl.iter().flat_map(|b| &b.segments).any(|s| s.is_none()));
            if broken {
                continue;
            }
            if let Some(cc) = skin::clip_channels(anm, skel.len() as u32, Some(&nm)) {
                if !cc.joints.is_empty() {
                    clips.push((nm, Clip::bake_with(&cc, &skel, Fill::Implicit)));
                }
            }
        }
    }
    println!("Kratos: {} joints, {} verts, {} pieces, {} clips (first: {:?})", skel.len(), bind_pos.len(), pieces.len(), clips.len(), clips.iter().take(6).map(|c| c.0.clone()).collect::<Vec<_>>());

    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &bind_pos {
        lo = lo.min(Vec3::from(*p));
        hi = hi.max(Vec3::from(*p));
    }
    let center = (lo + hi) / 2.0;
    let radius = (hi - lo).length() / 2.0;
    let current = start.clip.as_ref().and_then(|n| clips.iter().position(|(c, _)| c == n));
    commands.insert_resource(Character {
        skel,
        bind_pos,
        local: vertex_local,
        joints2: sm.joints2,
        weight: sm.weight,
        blend: true,
        face_front: std::env::var("GOW_RAWTURN").is_err(),
        joints: sm.joints,
        pieces,
        clips,
        current,
        time: start.time.unwrap_or(0.0),
        paused: start.time.is_some(),
        center,
        radius,
        speed: 1.0,
        pin_root: false,
        follow: true,
        joint_view: false,
        hide_skirt: false,
        focus: center,
        frame: 0,
    });
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { far: 100_000.0, ..default() }),
        Orbit {
            // GOW_CAM="yaw,pitch" in radians sets the starting camera (default 3.6, 0.1)
            yaw: std::env::var("GOW_CAM").ok().and_then(|s| s.split(',').next().and_then(|v| v.parse().ok())).unwrap_or(3.6),
            pitch: std::env::var("GOW_CAM").ok().and_then(|s| s.split(',').nth(1).and_then(|v| v.parse().ok())).unwrap_or(0.1), dist: radius * 2.6, target: center },
        Transform::default(),
    ));
}

fn orbit_camera(
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    buttons: Res<ButtonInput<MouseButton>>,
    ch: Res<Character>,
    mut q: Query<(&mut Orbit, &mut Transform)>,
) {
    for (mut o, mut t) in &mut q {
        if ch.follow {
            let k = 0.15;
            o.target = o.target + (ch.focus - o.target) * k;
        }
        if buttons.pressed(MouseButton::Left) {
            o.yaw -= motion.delta.x * 0.006;
            o.pitch = (o.pitch + motion.delta.y * 0.006).clamp(-1.5, 1.5);
        }
        o.dist = (o.dist * (1.0 - scroll.delta.y * 0.1)).max(0.5);
        let dir = Vec3::new(o.yaw.sin() * o.pitch.cos(), o.pitch.sin(), o.yaw.cos() * o.pitch.cos());
        t.translation = o.target + dir * o.dist;
        t.look_at(o.target, Vec3::Y);
    }
}

fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut ch: ResMut<Character>,
    mut windows: Query<&mut Window>,
    mut first: Local<bool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let n = ch.clips.len();
    let mut changed = !*first;
    *first = true;
    if keys.just_pressed(KeyCode::Space) {
        ch.paused = !ch.paused;
    }
    if keys.just_pressed(KeyCode::BracketRight) && n > 0 {
        ch.current = Some(ch.current.map_or(0, |c| (c + 1) % n));
        ch.time = 0.0;
        changed = true;
    }
    if keys.just_pressed(KeyCode::BracketLeft) && n > 0 {
        ch.current = Some(ch.current.map_or(n - 1, |c| (c + n - 1) % n));
        ch.time = 0.0;
        changed = true;
    }
    if keys.just_pressed(KeyCode::Digit0) {
        ch.current = None;
        changed = true;
    }
    if keys.just_pressed(KeyCode::Minus) {
        ch.speed = (ch.speed * 0.5).max(1.0 / 32.0);
        changed = true;
    }
    if keys.just_pressed(KeyCode::Equal) {
        ch.speed = (ch.speed * 2.0).min(2.0);
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        ch.pin_root = !ch.pin_root;
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        ch.joint_view = !ch.joint_view;
        for p in &ch.pieces {
            if let Some(mut m) = materials.get_mut(&p.mat) {
                m.base_color_texture = if ch.joint_view { None } else { p.tex.clone() };
            }
            if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, if ch.joint_view { p.joint_cols.clone() } else { p.cols.clone() });
            }
        }
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyY) {
        ch.face_front = !ch.face_front;
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyB) {
        ch.blend = !ch.blend;
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyK) {
        ch.hide_skirt = !ch.hide_skirt;
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        ch.follow = !ch.follow;
        changed = true;
    }
    // frame stepping while paused: . next frame, , previous frame
    if let Some(i) = ch.current {
        let dt = ch.clips[i].1.dt;
        let dur = ch.clips[i].1.duration;
        if keys.just_pressed(KeyCode::Period) {
            ch.paused = true;
            ch.time = (ch.time + dt) % dur.max(dt);
            changed = true;
        }
        if keys.just_pressed(KeyCode::Comma) {
            ch.paused = true;
            ch.time = (ch.time - dt + dur.max(dt)) % dur.max(dt);
            changed = true;
        }
    }
    if changed || (ch.current.is_some() && !ch.paused) {
        let flags = format!("speed {}x{}{}{}", ch.speed, if ch.paused { " paused" } else { "" }, if ch.pin_root { " in place" } else { "" }, if ch.follow { "" } else { " fixed camera" })
            + if ch.joint_view { " joint colours" } else { "" }
            + if ch.blend { "" } else { " rigid" }
            + if ch.face_front { " facing front" } else { " raw turn" };
        let title = match ch.current {
            Some(i) => {
                let c = &ch.clips[i].1;
                format!("Kratos: {} ({}/{}) frame {}/{} {}", ch.clips[i].0, i + 1, n, ch.frame, (c.duration / c.dt).round() as usize, flags)
            }
            None => "Kratos (bind pose)".to_string(),
        };
        for mut w in &mut windows {
            w.title = title.clone();
        }
    }
}

fn animate(time: Res<Time>, mut ch: ResMut<Character>, mut meshes: ResMut<Assets<Mesh>>) {
    let step = time.delta_secs() * ch.speed;
    if !ch.paused {
        ch.time += step;
    }
    let skinned = {
        let mut frame = 0;
        let mut local = match ch.current {
            Some(i) => {
                let c = &ch.clips[i].1;
                let t = if c.duration > 0.0 { ch.time % c.duration } else { 0.0 };
                frame = (t / c.dt).round() as usize;
                // GOW_DELTA: 0 old rule, 1 bind then delta, 2 (default) delta then bind; see docs/animation.md
                let delta = match std::env::var("GOW_DELTA").ok().as_deref() { Some("0") => Delta::Off, Some("1") => Delta::BindThenDelta, _ => Delta::DeltaThenBind };
                c.sample_with(&ch.skel, t, delta)
            }
            None => ch.skel.bind.clone(),
        };
        ch.frame = frame;
        // experiment: GOW_ROT picks how a clip rotation combines with the bind rotation (0 = clip rotation replaces bind)
        if let (Some(i), Ok(mode)) = (ch.current, std::env::var("GOW_ROT")) {
            let mode: u32 = mode.parse().unwrap_or(0);
            let qmul = |a: [f64; 4], b: [f64; 4]| -> [f64; 4] {
                [
                    a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
                    a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
                    a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
                    a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
                ]
            };
            for j in 0..local.len() {
                let animated = ch.clips[i].1.joints.get(j).map_or(false, |t| t.rot.is_some());
                if !animated {
                    continue;
                }
                let (qa, qb) = (local[j].q, ch.skel.bind[j].q);
                let conj = |q: [f64; 4]| [-q[0], -q[1], -q[2], q[3]];
                local[j].q = match mode {
                    1 => conj(qa),
                    2 => qmul(qb, qa),       // R_row(qa) * R_row(qb)
                    3 => qmul(qa, qb),       // R_row(qb) * R_row(qa)
                    4 => qmul(conj(qb), qa), // bind removed from the clip rotation, in one order
                    5 => qmul(qa, conj(qb)), // and in the other
                    _ => qa,
                };
            }
        }
        if ch.pin_root {
            if let Some(p) = ch.skel.names.iter().position(|n| n == "pelvis") {
                local[p].t[0] = ch.skel.bind[p].t[0];
                local[p].t[2] = ch.skel.bind[p].t[2];
            }
        }
        let world = world_matrices(&ch.skel, &local);
        let sk = skin_matrices(&ch.skel, &world);
        let mut out = if ch.blend {
            skin_positions_blend(&ch.bind_pos, &ch.joints, &ch.joints2, &ch.weight, &ch.local, &sk, &world)
        } else {
            let zero = vec![0.0f32; ch.weight.len()];
            skin_positions_blend(&ch.bind_pos, &ch.joints, &ch.joints2, &zero, &ch.local, &sk, &world)
        };
        if ch.face_front {
            // The clips turn the whole body (hips and shoulders together) by tens of degrees: a side-on stance. Undo the yaw
            // of the hip line about the vertical axis through the pelvis, so the pose can be judged from the front.
            let find = |n: &str| ch.skel.names.iter().position(|x| x == n);
            if let (Some(l), Some(r), Some(p)) = (find("lFemur"), find("rFemur"), find("pelvis")) {
                let (dx, dz) = (world[r][12] - world[l][12], world[r][14] - world[l][14]);
                let yaw = dz.atan2(dx);
                let (c, s) = (yaw.cos(), yaw.sin());
                let (px, pz) = (world[p][12], world[p][14]);
                for v in out.iter_mut() {
                    let (x, z) = (v[0] - px, v[2] - pz);
                    v[0] = px + x * c + z * s;
                    v[2] = pz - x * s + z * c;
                }
            }
        }
        out
    };
    let mut skinned = skinned;
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &skinned {
        lo = lo.min(Vec3::from(*p));
        hi = hi.max(Vec3::from(*p));
    }
    ch.focus = (lo + hi) / 2.0;
    if ch.hide_skirt {
        // joints named skirtJoints and joint0..joint41 (rig indices 65..=107 on Kratos); parked far away after the
        // camera bounds are taken
        let skirt: Vec<bool> = ch.skel.names.iter().map(|n| n == "skirtJoints" || n.strip_prefix("joint").map_or(false, |r| r.parse::<u32>().is_ok())).collect();
        for (p, &j) in skinned.iter_mut().zip(&ch.joints) {
            if skirt.get(j as usize).copied().unwrap_or(false) {
                *p = [0.0, -1000.0, 0.0];
            }
        }
    }
    for p in &ch.pieces {
        if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
            let pos: Vec<[f32; 3]> = p.src.iter().map(|&v| skinned[v as usize]).collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            }
    }
}


