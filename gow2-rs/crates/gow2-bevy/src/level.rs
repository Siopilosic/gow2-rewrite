//! Loads a level's static models from the user's WAD into Bevy, and builds the floor index for the controller.
//!
//! Placement follows `gow2_formats::level` (ref instances and model records); animated (rigged) objects are skipped. Triangles are
//! merged per material so a level is a few hundred meshes. Textures and blend modes follow `docs/models.md`.

use std::collections::{BTreeMap, HashMap};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use gow2_formats::{level, mdl, sheet, skin, texture::TextureStore, wad};
use gow2_kratos::world::{CollisionWorld, TriWorld, World};
use gow2_skel::{transform_point, Skeleton};

/// How a collision triangle is drawn in the debug overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionKind {
    Floor,
    Ceiling,
    Wall,
    /// In the sheet, but the hero's queries skip it (guides, water, `NoPlayerCollision`, ...).
    Skipped,
}

/// A sky dome: drawn around the camera, far beyond the level.
#[derive(Component)]
pub struct SkyDome;

/// The domes are about 300 units across in model space; scaled up they lie beyond everything in the level but inside the camera's far plane.
const SKY_SCALE: f32 = 40.0;

/// Keeps the sky domes centred on the camera.
pub fn sky_follow(cams: Query<&Transform, (With<Camera3d>, Without<SkyDome>)>, mut sky: Query<&mut Transform, With<SkyDome>>) {
    let Ok(c) = cams.single() else { return };
    for mut t in &mut sky {
        t.translation = c.translation;
    }
}

/// What loading a level produced.
pub struct LoadedLevel {
    /// What the controller collides with: the level's collision sheet, or a floor built from render triangles when it has none.
    pub world: Box<dyn World + Send + Sync>,
    /// Render triangles drawn.
    pub triangles: usize,
    pub models: usize,
    /// Bounding box of everything drawn.
    pub bounds: (Vec3, Vec3),
    /// One line about the collision source, for the log.
    pub collision: String,
    /// The collision triangles with their kind, for the overlay.
    pub overlay: Vec<([[f32; 3]; 3], CollisionKind)>,
    /// The opaque render triangles, for the camera (it must not sit inside columns and walls that have no collision polygon).
    pub camera: CollisionWorld,
    /// Connected parts of the walkable floor, largest first (a level whose parts the game joins by script has several).
    pub regions: Vec<gow2_kratos::world::Region>,
    /// Breakable objects: solid until hit (`GOW_NOBREAKABLES=1` draws them as ordinary static models without collision).
    pub breakables: Vec<Breakable>,
    /// Bounds of columns that have no collision polygon in the sheet (the innerPillar models): solid cylinders for the hero.
    pub pillars: Vec<([f32; 3], [f32; 3])>,
    /// Mean centre of the models of ordinary size (not scenery, not sky): where the playable part of the level is, for choosing a start.
    pub focus: [f32; 3],
}

/// Collision world from the level's sheet: the polygons the hero's movement queries do not skip, as oriented triangles.
fn sheet_world(s: &sheet::Sheet) -> (CollisionWorld, Vec<([[f32; 3]; 3], CollisionKind)>, String) {
    use gow2_kratos::world::{sheet_triangles, Class};
    let tris = sheet_triangles(s);
    let overlay = tris
        .iter()
        .map(|&(t, c)| (t, match c { Class::Floor => CollisionKind::Floor, Class::Ceiling => CollisionKind::Ceiling, Class::Wall => CollisionKind::Wall, Class::Skipped => CollisionKind::Skipped }))
        .collect();
    let world = gow2_kratos::world::sheet_world(s);
    let (fl, ce, wa) = world.counts();
    let info = format!("collision sheet: {} polygons, {} triangles used ({fl} floor, {ce} ceiling, {wa} wall)", s.polys.len(), world.triangle_count());
    (world, overlay, info)
}
#[derive(Default)]
struct Batch {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

fn rgba_image(t: &gow2_formats::texture::Texture) -> Image {
    crate::gpu::texture_image(t, true)
}

/// An unlit material for a MAT record of the level: its texture, and the blend mode from the top byte of `+0x38` (0x48 additive, 0x42 alpha).
fn level_material(first: &BTreeMap<&str, &[u8]>, store: &TextureStore, images: &mut Assets<Image>, name: &str) -> StandardMaterial {
    let mut mat = StandardMaterial { unlit: true, cull_mode: None, perceptual_roughness: 1.0, ..default() };
    if let Some(mb) = first.get(name).filter(|m| m.len() == 120) {
        let w38 = u32::from_le_bytes([mb[0x38], mb[0x39], mb[0x3a], mb[0x3b]]);
        match w38 >> 24 {
            0x48 => mat.alpha_mode = AlphaMode::Add,
            0x42 => mat.alpha_mode = AlphaMode::Blend,
            _ => {}
        }
    }
    // a material without a texture is a flat colour: the three floats at +8 of the MAT record (MAT_groundPlainDesign 0.4 grey, MAT_genericTan a tan)
    if store.material_texture(name).is_none() {
        if let Some(mb) = first.get(name).filter(|m| m.len() == 120) {
            let f = |o: usize| f32::from_le_bytes([mb[o], mb[o + 1], mb[o + 2], mb[o + 3]]);
            let c = [f(8), f(12), f(16)];
            if c.iter().all(|v| v.is_finite() && (0.0..=2.0).contains(v)) {
                mat.base_color = Color::linear_rgb(c[0], c[1], c[2]);
            }
        }
    }
    if let Some(t) = store.material_texture(name) {
        if matches!(mat.alpha_mode, AlphaMode::Opaque) && t.rgba.chunks(4).any(|p| p[3] < 255) && std::env::var_os("GOW_NOMASK").is_none() {
            mat.alpha_mode = AlphaMode::Mask(0.5);
        }
        mat.base_color_texture = Some(images.add(rgba_image(&t)));
    }
    mat
}

/// A breakable object (crate, pot, basket, barrel): drawn as its own entities so it can disappear, solid until it is hit.
pub struct Breakable {
    pub name: String,
    pub entities: Vec<Entity>,
    /// Axis-aligned bounds of the object in world space.
    pub lo: [f32; 3],
    pub hi: [f32; 3],
}

fn is_breakable(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    m.starts_with("break") && !m.contains("wall")
}
/// Spawns the static models of the level WAD at `path`.
pub fn spawn_level(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    path: &str,
) -> LoadedLevel {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let pl = level::parse(&recs);
    let first: BTreeMap<&str, &[u8]> = recs.iter().filter(|r| r.tag == wad::Tag::Object && !r.data.is_empty()).fold(
        BTreeMap::new(),
        |mut m, r| {
            m.entry(r.name.as_str()).or_insert(r.data);
            m
        },
    );
    let store = TextureStore::new(recs.iter().filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)));

    // skeletons of the rigged models: their vertices are joint-local and are placed by the bind-pose joint matrices
    let skels: HashMap<String, Skeleton> = skin::find_rigs(&recs)
        .into_iter()
        .filter(|r| pl.rigged.contains(&r.model))
        .map(|r| (r.model.clone(), Skeleton::from_rig(&skin::parse_rig(r.rig))))
        .collect();
    // decode every drawable model once
    let mut decoded: HashMap<&str, (skin::SkinMesh, Vec<String>)> = HashMap::new();
    for m in &pl.models {
        if let Some(blob) = first.get(wad::mesh_record_name(m).as_str()) {
            let sm = skin::mesh_joints(blob, 4096.0);
            decoded.insert(m.as_str(), (sm, mdl::model_materials(&recs, m)));
        }
    }

    let mut batches: BTreeMap<String, Batch> = BTreeMap::new();
    let mut breaks: Vec<(String, [f32; 3], [f32; 3], BTreeMap<String, Batch>)> = Vec::new();
    let mut pillars: Vec<([f32; 3], [f32; 3])> = Vec::new();
    let mut centres: Vec<[f32; 3]> = Vec::new();
    let mut floor_tris: Vec<[[f32; 3]; 3]> = Vec::new();
    let mut tri_material: Vec<String> = Vec::new();
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut drawn = 0usize;
    // Models the game shows only at certain moments of a scripted sequence. Drawn all at once they overlap (the three RHOD10 gate models share one place and
    // have no collision, so a closed gate would be walked through): they are left out. `GOW_ALLMODELS=1` draws everything.
    let all_models = std::env::var_os("GOW_ALLMODELS").is_some();
    let scripted = |m: &str| m.to_ascii_lowercase().contains("placeholder") || matches!(m, "ZeroDoor" | "FirstDoor" | "SecondDoor" | "DoorSparkle" | "sparkle" | "fallingDebris" | "godRays");
    let mut add = |model: &str, place: &dyn Fn([f32; 3]) -> [f32; 3]| {
        if !all_models && scripted(model) {
            return;
        }
        let Some((sm, mats)) = decoded.get(model) else { return };
        drawn += 1;
        let skel = skels.get(model);
        let world: Vec<[f32; 3]> = sm
            .verts
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let mut p = [v[0] as f32 / 16.0, v[1] as f32 / 16.0, v[2] as f32 / 16.0];
                // rigged model: the vertex is joint-local, so the bind-pose joint matrix (which carries the model's own offset
                // and scale) comes first
                if let Some(m) = skel.and_then(|s| s.bind_world.get(sm.joints[i] as usize)) {
                    p = transform_point(m, p);
                }
                place(p)
            })
            .collect();
        let (mut mlo, mut mhi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in &world {
            lo = lo.min(Vec3::from(*p));
            hi = hi.max(Vec3::from(*p));
            mlo = mlo.min(Vec3::from(*p));
            mhi = mhi.max(Vec3::from(*p));
        }
        if std::env::var_os("GOW_MODELLIST").is_some() {
            println!("model {model}: {} tris, x {:.0}..{:.0} y {:.0}..{:.0} z {:.0}..{:.0}", sm.tris.len(), mlo.x, mhi.x, mlo.y, mhi.y, mlo.z, mhi.z);
        }
        if (mhi - mlo).max_element() < 1500.0 && !model.to_ascii_lowercase().contains("sky") && mhi.x > -500.0 {
            centres.push(((mlo + mhi) / 2.0).to_array());
        }
        if model.starts_with("innerPillar") {
            pillars.push((mlo.to_array(), mhi.to_array()));
        }
        // a breakable object keeps its own batches so it can be hidden when it breaks
        if is_breakable(model) && std::env::var_os("GOW_NOBREAKABLES").is_none() {
            let mut local: BTreeMap<String, Batch> = BTreeMap::new();
            for (t, slot) in &sm.tris {
                let b = local.entry(mats.get(*slot as usize).cloned().unwrap_or_else(|| "MAT_none".into())).or_default();
                let base = b.pos.len() as u32;
                for &vi in t {
                    let vi = vi as usize;
                    b.pos.push(world[vi]);
                    b.uv.push([sm.uvs[vi][0] as f32, sm.uvs[vi][1] as f32]);
                    let c = sm.cols[vi];
                    b.col.push([c[0].min(128) as f32 / 128.0, c[1].min(128) as f32 / 128.0, c[2].min(128) as f32 / 128.0, c[3].min(128) as f32 / 128.0]);
                }
                b.idx.extend([base, base + 1, base + 2]);
            }
            breaks.push((model.to_string(), mlo.to_array(), mhi.to_array(), local));
            return;
        }
        // the sky domes surround the origin in model space: they are drawn apart and follow the camera (`sky_follow`)
        let sky = model.to_ascii_lowercase().contains("skydome");
        for (t, slot) in &sm.tris {
            let name = mats.get(*slot as usize).cloned().unwrap_or_else(|| "MAT_none".into());
            let b = batches.entry(if sky { format!("SKY|{name}") } else { name }).or_default();
            let base = b.pos.len() as u32;
            for &vi in t {
                let vi = vi as usize;
                b.pos.push(world[vi]);
                b.uv.push([sm.uvs[vi][0] as f32, sm.uvs[vi][1] as f32]);
                let c = sm.cols[vi];
                b.col.push([c[0].min(128) as f32 / 128.0, c[1].min(128) as f32 / 128.0, c[2].min(128) as f32 / 128.0, c[3].min(128) as f32 / 128.0]);
            }
            b.idx.extend([base, base + 1, base + 2]);
            if sky {
                continue;
            }
            floor_tris.push([world[t[0] as usize], world[t[1] as usize], world[t[2] as usize]]);
            tri_material.push(mats.get(*slot as usize).cloned().unwrap_or_else(|| "MAT_none".into()));
        }
    };
    let identity = level::ModelXf { offset: [0.0; 3], scale: 1.0 };
    for inst in &pl.instances {
        // a rigged model's own offset and scale are in its root joint, so only the instance rotation and translation follow
        let xf = if pl.rigged.contains(&inst.model) { identity } else { pl.xf(&inst.model) };
        add(&inst.model, &|p| level::place_instance(p, xf, inst));
    }
    // rigged models that no ref instance names are placed by their go node
    let instanced: std::collections::HashSet<&String> = pl.instances.iter().map(|i| &i.model).collect();
    for (model, (rot, pos)) in &pl.go_xf {
        if pl.rigged.contains(model) && !instanced.contains(model) {
            let inst = level::Instance { name: format!("go {model}"), model: model.clone(), rot: *rot, pos: *pos };
            add(model, &|p| level::place_instance(p, identity, &inst));
        }
    }
    for m in pl.plain_models() {
        let xf = pl.xf(m);
        add(m, &|p| level::place_plain(p, xf));
    }

    let triangles = floor_tris.len();
    let mut solid_materials: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (key, b) in batches {
        let (is_sky, name) = match key.strip_prefix("SKY|") {
            Some(n) => (true, n.to_string()),
            None => (false, key.clone()),
        };
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, b.pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, b.uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, b.col);
        mesh.insert_indices(Indices::U32(b.idx));
        if std::env::var_os("GOW_MATLIST").is_some() {
            let w38 = first.get(name.as_str()).filter(|m| m.len() == 120).map(|mb| u32::from_le_bytes([mb[0x38], mb[0x39], mb[0x3a], mb[0x3b]]));
            println!("material {name}: {} verts, w38 {:?}", mesh.count_vertices(), w38.map(|w| format!("{w:08x}")));
        }
        let mat = level_material(&first, &store, images, &name);
        if std::env::var_os("GOW_MATLIST").is_some() && mat.base_color_texture.is_none() {
            println!("  untextured: {name} ({} verts)", mesh.count_vertices());
        }
        if is_sky {
            commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(materials.add(mat)), Transform::from_scale(Vec3::splat(SKY_SCALE)), SkyDome, bevy::camera::visibility::NoFrustumCulling));
            continue;
        }
        if matches!(mat.alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_)) {
            solid_materials.insert(name.clone());
        }
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(materials.add(mat)), Transform::default()));
    }
    let mut breakables = Vec::new();
    for (name, lo, hi, local) in breaks {
        let mut entities = Vec::new();
        for (mname, b) in local {
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, b.pos);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, b.uv);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, b.col);
            mesh.insert_indices(Indices::U32(b.idx));
            let mat = level_material(&first, &store, images, &mname);
            entities.push(commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(materials.add(mat)), Transform::default())).id());
        }
        breakables.push(Breakable { name, entities, lo, hi });
    }
    let camera = CollisionWorld::new(floor_tris.iter().zip(&tri_material).filter(|(_, m)| solid_materials.contains(*m)).map(|(t, _)| *t), 64.0);
    let (world, overlay, collision, regions): (Box<dyn World + Send + Sync>, _, _, _) = match sheet::find(&recs) {
        Some(s) => {
            let (w, overlay, info) = sheet_world(&s);
            let regions = gow2_kratos::world::walkable_regions(&w, 8.0);
            (Box::new(w), overlay, info, regions)
        }
        None => {
            let info = "no collision sheet: floor from render triangles, no walls".to_string();
            (Box::new(TriWorld::new(floor_tris, 32.0)), Vec::new(), info, Vec::new())
        }
    };
    // the median centre, so a few strays (a rigged model left at the origin) do not pull it
    let focus = if centres.is_empty() {
        [0.0; 3]
    } else {
        [0, 1, 2].map(|k| {
            let mut v: Vec<f32> = centres.iter().map(|c| c[k]).collect();
            v.sort_by(|a, b| a.total_cmp(b));
            v[v.len() / 2]
        })
    };
    LoadedLevel { world, triangles, models: drawn, bounds: (lo, hi), collision, overlay, camera, regions, breakables, pillars, focus }
}















