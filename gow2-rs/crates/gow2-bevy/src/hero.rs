//! Loads Kratos from the user's WAD into Bevy: textured pieces, skeleton and the clips the controller needs.
//!
//! Everything is decoded at start-up; no game data is bundled. The skinning follows `docs/animation.md`: two-joint blending
//! with the weight in the vertex w field, and joint-local parts (the armour plates) placed by their joint's world matrix.

use std::collections::{BTreeMap, HashMap};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use gow2_formats::{anm, mdl, skin, texture::TextureStore, wad};
use gow2_kratos::anim::ClipInfo;
use gow2_skel::{blend_poses, local_parts, skin_matrices, skin_positions_blend, world_matrices, Clip, Skeleton, Trs};

/// One drawable piece: the vertices of one material slot, remapped from the model's vertex list.
pub struct Piece {
    pub mesh: Handle<Mesh>,
    pub src: Vec<u32>,
    /// The piece's material and its colour as loaded (a hurt flash or a stone tint multiplies this).
    pub material: Handle<StandardMaterial>,
    pub base: Color,
}

/// Kratos's model, skeleton and the clips loaded for him.
#[derive(Resource)]
pub struct Hero {
    pub skel: Skeleton,
    pub bind_pos: Vec<[f32; 3]>,
    pub joints: Vec<i64>,
    pub joints2: Vec<i64>,
    pub weight: Vec<f32>,
    pub local: Vec<bool>,
    pub pieces: Vec<Piece>,
    pub clips: HashMap<String, Clip>,
    pub infos: HashMap<String, ClipInfo>,
    pub root: Entity,
    /// Dark ellipsoids inside the limbs and torso that close the model's open shell (see `FILLER`).
    pub filler: Vec<Filler>,
    /// Aim groups: a move's clip name that has no data of its own and names three partial aim clips (members ...00, ...01, ...02: aimed low, level, high).
    pub groups: HashMap<String, [String; 3]>,
}

/// One filler ellipsoid between two joints.
pub struct Filler {
    pub entity: Entity,
    pub a: usize,
    pub b: usize,
    /// Half widths across the segment (x and z of the joint frame's model space).
    pub rx: f32,
    pub rz: f32,
}

/// The armoured model is a thin shell: it is open at the neck, under the arms and where the blades sit, and the game never lets the camera see inside. From other angles
/// the holes showed straight through to the far wall of the shell. These ellipsoids (joint, joint, half width, half depth; units of the model) sit inside the armour so
/// a hole shows a dark solid instead (visual choice; the sizes were picked by eye against the bind pose).
const FILLER: [(&str, &str, f32, f32); 12] = [
    ("pelvis", "vertebrae4", 3.0, 2.0),
    ("vertebrae3", "neck", 3.2, 2.2),
    ("neck", "head", 1.0, 1.0),
    ("lHumerus", "lRadius", 1.15, 1.15),
    ("lRadius", "lWrist", 1.0, 1.0),
    ("rHumerus", "rRadius", 1.15, 1.15),
    ("rRadius", "rWrist", 1.0, 1.0),
    ("lFemur", "lTibia", 1.7, 1.7),
    ("lTibia", "lMetatarsal", 1.3, 1.3),
    ("rFemur", "rTibia", 1.7, 1.7),
    ("rTibia", "rMetatarsal", 1.3, 1.3),
    ("pelvis", "pelvis", 2.8, 1.9),
];

/// WADs beside the hero WAD that hold more clips for the hero's rig (the magic animations).
const EXTRA_CLIP_WADS: [&str; 5] = ["R_M_LGHTN2.WAD", "R_M_ELCTRC0.WAD", "R_S_BONE0.WAD", "R_S_HAMMER0.WAD", "R_S_OLYMPUS0.WAD"];

/// The model is a thin shell (the PS2 game never shows its inside), and the baked vertex shading of the inner faces is near black, so a hole shows as a black pit.
/// A floor on the vertex colour keeps the inside of the armour readable instead (visual choice).
const INNER_LIGHT: f32 = 0.42;

/// The enemies' models are characters like Kratos (baked vertex colour clamped to the GS's 128), not effect models; the Rhodes soldier is the first.
fn is_enemy(model: &str) -> bool {
    model == "rhsold00"
}

fn rgba_image(t: &gow2_formats::texture::Texture) -> Image {
    crate::gpu::texture_image(t, true)
}

/// Loads `model` (usually "hero") from the WAD at `path`, spawns its pieces under a root entity and decodes the named clips.
pub fn spawn_hero(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    path: &str,
    model: &str,
    clip_names: &[&str],
) -> Hero {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let rr = skin::find_rigs(&recs)
        .into_iter()
        .find(|r| r.model == model)
        .unwrap_or_else(|| panic!("no rig for model {model} in {path}"));
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let first: BTreeMap<&str, &[u8]> = recs.iter().filter(|r| r.tag == wad::Tag::Object && !r.data.is_empty()).fold(
        BTreeMap::new(),
        |mut m, r| {
            m.entry(r.name.as_str()).or_insert(r.data);
            m
        },
    );
    // (record names are cut to their last 19 characters: `MDL_WindChargeGust_0` is stored as `DL_WindChargeGust_0`)
    let blob = first[wad::mesh_record_name(model).as_str()];
    let sm = skin::mesh_joints(blob, 4096.0);
    // Kratos's own model is in model space and skinned; every other rigged object (weapons, the magic's models) has joint-local vertices, placed by the joint matrices
    let nparts = sm.part.iter().copied().max().map_or(0, |m| m as usize + 1);
    // GOW_LOCALPARTS=0 treats the parts of the other models as model-space skinned meshes (like Kratos) instead of joint-local ones, to look at the Medusa head
    // Without a flag for it, the other models are decided by their bounds: a model whose vertices are joint-local looks scattered when taken as model-space, and the other way
    // round (the Medusa head, 24 joints, is a skinned mesh in model space; its snakes came out as long spikes when placed by their joints, the Olympus is joint-local with a 1/32 root)
    let model_space = |sm: &skin::SkinMesh| -> bool {
        let diag = |pts: &mut dyn Iterator<Item = [f32; 3]>| {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in pts {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt()
        };
        let raw = diag(&mut sm.verts.iter().map(|v| [v[0] as f32 / 16.0, v[1] as f32 / 16.0, v[2] as f32 / 16.0]));
        let placed = diag(&mut sm.verts.iter().enumerate().map(|(i, v)| {
            let p = [v[0] as f32 / 16.0, v[1] as f32 / 16.0, v[2] as f32 / 16.0];
            skel.bind_world.get(sm.joints[i] as usize).map_or(p, |m| gow2_skel::transform_point(m, p))
        }));
        skel.len() > 2 && raw <= placed * 1.05
    };
    // (the Rhodes soldier is built of rigid pieces, one joint each, like the weapons: its parts are joint-local, found by looking at it)
    let local_default = if model == "hero" || is_enemy(model) { true } else { !model_space(&sm) };
    let part_local: Vec<bool> = if model == "hero" { local_parts(&sm, &skel) } else { vec![std::env::var("GOW_LOCALPARTS").map_or(local_default, |v| v != "0"); nparts] };
    if std::env::var_os("GOW_LOGFX").is_some() && model != "hero" {
        println!("  {model}: parts are {}", if local_default { "joint-local" } else { "model-space skinned" });
    }
    let local: Vec<bool> = sm.part.iter().map(|&p| part_local[p as usize]).collect();
    let mat_names = mdl::model_materials(&recs, model);
    let store = TextureStore::new(recs.iter().filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)));
    let bind_pos: Vec<[f32; 3]> = sm.verts.iter().map(|v| [v[0] as f32 / 16.0, v[1] as f32 / 16.0, v[2] as f32 / 16.0]).collect();

    let root = commands.spawn((Transform::default(), Visibility::default())).id();
    let mut slots: BTreeMap<u16, Vec<[u32; 3]>> = BTreeMap::new();
    for (t, s) in &sm.tris {
        slots.entry(*s).or_default().push(*t);
    }
    let mut pieces = Vec::new();
    for (slot, tris) in slots {
        let mut remap: BTreeMap<u32, u32> = BTreeMap::new();
        let (mut src, mut idx) = (Vec::new(), Vec::new());
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
                // GOW_PARTCOLOR=1 paints every model part in its own colour (debugging the part frames and the holes between parts)
                if std::env::var_os("GOW_PARTCOLOR").is_some() {
                    let k = sm.part[v as usize] as usize;
                    let c3 = Color::hsl((k as f32 * 137.5) % 360.0, 1.0, 0.55).to_srgba();
                    let p = [c3.red, c3.green, c3.blue];
                    let shade = 0.55 + 0.45 * (c[0].min(128) as f32 / 128.0);
                    return [p[0] * shade, p[1] * shade, p[2] * shade, 1.0];
                }
                if model != "hero" && !is_enemy(model) {
                    // the effect and weapon models: the colour goes to the GS as it is (128 = 1.0, up to 255 = 2.0 after modulation), no floor
                    return [c[0] as f32 / 128.0, c[1] as f32 / 128.0, c[2] as f32 / 128.0, c[3].min(128) as f32 / 128.0];
                }
                if is_enemy(model) {
                    return [c[0].min(128) as f32 / 128.0, c[1].min(128) as f32 / 128.0, c[2].min(128) as f32 / 128.0, c[3].min(128) as f32 / 128.0];
                }
                [(c[0].min(128) as f32 / 128.0).max(INNER_LIGHT), (c[1].min(128) as f32 / 128.0).max(INNER_LIGHT), (c[2].min(128) as f32 / 128.0).max(INNER_LIGHT), c[3].min(128) as f32 / 128.0]
            })
            .collect();
        let pos: Vec<[f32; 3]> = src.iter().map(|&v| bind_pos[v as usize]).collect();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        mesh.insert_indices(Indices::U32(idx));
        let mut mat = StandardMaterial { unlit: true, cull_mode: if std::env::var_os("GOW_CULL").is_some() { Some(bevy::render::render_resource::Face::Back) } else { None }, perceptual_roughness: 1.0, ..default() };
        // effect objects (the magic's models) use the blend mode of their MAT record (top byte of +0x38: 0x48 additive, 0x42 alpha); Kratos himself does not
        if model != "hero" && !is_enemy(model) {
            if let Some(mb) = mat_names.get(slot as usize).and_then(|m| first.get(m.as_str())).filter(|m| m.len() == 120) {
                match u32::from_le_bytes([mb[0x38], mb[0x39], mb[0x3a], mb[0x3b]]) >> 24 {
                    0x48 => mat.alpha_mode = AlphaMode::Add,
                    0x42 => mat.alpha_mode = AlphaMode::Blend,
                    _ => {}
                }
            }
        }
        if let Some(t) = mat_names.get(slot as usize).and_then(|m| store.material_texture(m)).filter(|_| std::env::var_os("GOW_PARTCOLOR").is_none()) {
            if matches!(mat.alpha_mode, AlphaMode::Opaque) && t.rgba.chunks(4).any(|p| p[3] < 255) && std::env::var_os("GOW_NOMASK").is_none() {
                mat.alpha_mode = AlphaMode::Mask(0.5);
            }
            mat.base_color_texture = Some(images.add(rgba_image(&t)));
        }
        if model != "hero" && std::env::var_os("GOW_LOGFX").is_some() {
            println!("  {model} part {slot}: material {:?} blend {:?} texture {}", mat_names.get(slot as usize), mat.alpha_mode, mat.base_color_texture.is_some());
        }
        // the material's RGBA tint (MAT +0x60..+0x6c) multiplies the vertex colour in the game's shader (the per-object colour constant of the VU1 programs): the Medusa beam is
        // red, the tornado blue-grey, the Bone staff's crystal purple, the bare Kratos's skin 1.5
        if let Some(mb) = mat_names.get(slot as usize).and_then(|m| first.get(m.as_str())).filter(|m| m.len() == 120) {
            let f = |o: usize| f32::from_le_bytes([mb[o], mb[o + 1], mb[o + 2], mb[o + 3]]);
            // (not for the additive materials, blend byte 0x48: see `blades::material`)
            if mb[0x3b] != 0x48 {
                mat.base_color = Color::linear_rgba(f(0x60), f(0x64), f(0x68), 1.0);
            }
        }
        let handle = meshes.add(mesh);
        // the vertices are rewritten every frame, so the bounding box the engine computed from the first positions is wrong: no frustum culling for the pieces
        let base = mat.base_color;
        let material = materials.add(mat);
        let child = commands.spawn((Mesh3d(handle.clone()), MeshMaterial3d(material.clone()), Transform::default(), bevy::camera::visibility::NoFrustumCulling)).id();
        commands.entity(root).add_child(child);
        pieces.push(Piece { mesh: handle, src, material, base });
    }

    let mut clips: HashMap<String, Clip> = HashMap::new();
    let mut infos: HashMap<String, ClipInfo> = HashMap::new();
    let mut groups: HashMap<String, [String; 3]> = HashMap::new();
    if let Some(a) = rr.anm.as_deref().and_then(|n| first.get(n)) {
        let locomotion = gow2_kratos::anim::ALL_CLIPS;
        let mut missing = 0;
        for name in clip_names {
            // * stands for the first clip of the bank (the effect objects have one clip each)
            let wanted = if *name == "*" { None } else { Some(*name) };
            if let Some(cc) = skin::clip_channels(a, skel.len() as u32, wanted) {
                let clip = Clip::bake(&cc, &skel);
                // only the locomotion clips need their foot speed
                let ground_speed = if locomotion.contains(name) { gow2_kratos::anim::ground_speed(&skel, &clip) } else { 0.0 };
                let info = ClipInfo { duration: clip.duration, ground_speed };
                if locomotion.contains(name) {
                    println!("clip {name}: {:.2} s, ground speed {:.1} units/s", info.duration, info.ground_speed);
                }
                infos.insert(name.to_string(), info);
                clips.insert(name.to_string(), clip);
            } else {
                // A move may name a group of aim clips (`magMedusaBeam` has `magMedusaBeam00`, `01`, `02`; `magWindIdleBlend` has `magWindIdle00` to `02`): the group
                // itself holds no data. Until the blend between the members is decoded, the middle member stands in (LOW).
                let stem = name.trim_end_matches("Blend");
                for s in [name.to_string(), stem.to_string(), name.replace("magMedusa", "magMedusaAir")] {
                    let members = ["00", "01", "02"].map(|k| format!("{s}{k}"));
                    let baked: Vec<Option<Clip>> = members.iter().map(|m| skin::clip_channels(a, skel.len() as u32, Some(m.as_str())).map(|cc| Clip::bake(&cc, &skel))).collect();
                    if baked.iter().all(|c| c.is_some()) {
                        for (m, c) in members.iter().zip(baked) {
                            let c = c.unwrap();
                            infos.insert(m.clone(), ClipInfo { duration: c.duration, ground_speed: 0.0 });
                            clips.insert(m.clone(), c);
                        }
                        groups.insert(name.to_string(), members);
                        break;
                    }
                }
                let m = std::env::var("GOW_AIM").unwrap_or_else(|_| "01".into());
                let alt = [format!("{name}{m}"), format!("{stem}{m}"), format!("{}{m}", name.replace("magMedusa", "magMedusaAir"))];
                let found = alt.iter().find_map(|cand| skin::clip_channels(a, skel.len() as u32, Some(cand.as_str())).map(|cc| (cand.clone(), cc)));
                if let Some((used, cc)) = found {
                    let clip = Clip::bake(&cc, &skel);
                    println!("clip {name}: stands in as {used}");
                    infos.insert(name.to_string(), ClipInfo { duration: clip.duration, ground_speed: 0.0 });
                    clips.insert(name.to_string(), clip);
                    continue;
                }
                missing += 1;
                if std::env::var_os("GOW_LOGMISSING").is_some() {
                    println!("clip not in the hero bank: {name}");
                }
            }
        }
        println!("{} clips decoded, {missing} not found", clips.len());
    }
    // more clip banks for the hero's rig: the magic WADs next to the hero WAD carry `ANM_Hero_Magic*` (Lightning in `R_M_LGHTN2`, Electric in `R_M_ELCTRC0`);
    // their clip names are the move data's `mag*` names. A clip already found keeps its first source.
    if let Some(dir) = std::path::Path::new(path).parent().filter(|_| model == "hero") {
        for wad_name in EXTRA_CLIP_WADS {
            let Ok(extra) = std::fs::read(dir.join(wad_name)) else { continue };
            let erecs: Vec<wad::Record> = wad::records(&extra).collect();
            for r in erecs.iter().filter(|r| r.tag == wad::Tag::Object && r.name.starts_with("ANM_Hero") && !r.data.is_empty()) {
                let mut added = 0;
                for name in clip_names {
                    if clips.contains_key(*name) {
                        continue;
                    }
                    if let Some(cc) = skin::clip_channels(r.data, skel.len() as u32, Some(name)) {
                        let clip = Clip::bake(&cc, &skel);
                        infos.insert(name.to_string(), ClipInfo { duration: clip.duration, ground_speed: 0.0 });
                        clips.insert(name.to_string(), clip);
                        added += 1;
                    }
                }
                println!("{} clips from {wad_name} {}", added, r.name);
            }
        }
    }
    let _ = anm::clips; // the decoder module is used through skin::clip_channels
    let core = materials.add(StandardMaterial { base_color: Color::srgb(0.36, 0.24, 0.18), unlit: true, ..default() });
    let ball = meshes.add(Sphere::new(1.0).mesh().uv(14, 9));
    let mut filler = Vec::new();
    for (an, bn, rx, rz) in FILLER {
        // the filler ellipsoids are not in the game's model (the retail model has no inside geometry either); they are off unless GOW_FILL=1
        if std::env::var_os("GOW_FILL").is_none() {
            break;
        }
        let (Some(a), Some(b)) = (skel.names.iter().position(|n| n == an), skel.names.iter().position(|n| n == bn)) else { continue };
        let entity = commands.spawn((Mesh3d(ball.clone()), MeshMaterial3d(core.clone()), Transform::default())).id();
        commands.entity(root).add_child(entity);
        filler.push(Filler { entity, a, b, rx, rz });
    }
    Hero { skel, bind_pos, joints: sm.joints, joints2: sm.joints2, weight: sm.weight, local, pieces, clips, infos, root, filler, groups }
}

impl Hero {
    /// Local pose from weighted clip layers `(clip name, time in seconds, weight)`.
    pub fn pose(&self, layers: &[(&str, f32, f32)]) -> Vec<Trs> {
        // clips with no pelvis channel are partial (aim overlays): they go over the full-body layers instead of blending with them
        let pelvis = self.skel.names.iter().position(|n| n == "pelvis");
        let partial = |n: &str| self.clips.get(n).is_some_and(|c| pelvis.is_some_and(|p| c.joints[p].rot.is_none() && c.joints[p].trans.is_none()));
        if layers.iter().any(|(n, _, _)| partial(n)) {
            let (over, base): (Vec<_>, Vec<_>) = layers.iter().copied().partition(|(n, _, _)| partial(n));
            let total: f32 = base.iter().map(|l| l.2).sum();
            let base: Vec<(&str, f32, f32)> = if total > 0.0 { base.iter().map(|&(n, t, w)| (n, t, w / total)).collect() } else { base };
            return self.pose_over(&base, &over);
        }
        let samples: Vec<(Vec<Trs>, f32)> = layers
            .iter()
            .filter_map(|(n, t, w)| self.clips.get(*n).map(|c| (c.sample(&self.skel, *t), *w)))
            .collect();
        if samples.is_empty() {
            return self.skel.bind.clone();
        }
        // weights above one in total would add the joint scales up (two layers at 1.0 gave scale 2.0), so they are scaled back to one
        let total: f32 = samples.iter().map(|(_, w)| *w).sum();
        let k = if total > 1.0 { 1.0 / total } else { 1.0 };
        let refs: Vec<(&[Trs], f32)> = samples.iter().map(|(p, w)| (p.as_slice(), *w * k)).collect();
        blend_poses(&refs)
    }

    /// A pose with partial clips laid over a base: the aim clips of the magic (`magMedusaIdle00`...) animate only the spine, head and arms (no pelvis, no legs; found
    /// with `gow2-kratos` example `aim_probe`), so each joint they cover is blended from the base pose to the blended overlay pose by the overlays' total weight (at most 1)
    /// and every other joint keeps the base pose. `over` is `(clip, time, weight)`.
    pub fn pose_over(&self, base: &[(&str, f32, f32)], over: &[(&str, f32, f32)]) -> Vec<Trs> {
        let mut out = self.pose(base);
        // (`pose` of the base layers: they are full-body clips, so it does not come back here)
        let layers: Vec<(&Clip, Vec<Trs>, f32)> = over.iter().filter_map(|(n, t, w)| self.clips.get(*n).map(|c| (c, c.sample(&self.skel, *t), *w))).filter(|(_, _, w)| *w > 0.0).collect();
        for j in 0..out.len() {
            let covering: Vec<(&[Trs], f32)> = layers.iter().filter(|(c, _, _)| c.joints[j].rot.is_some() || c.joints[j].trans.is_some() || c.joints[j].scale.is_some()).map(|(_, p, w)| (std::slice::from_ref(&p[j]), *w)).collect();
            let total: f32 = covering.iter().map(|(_, w)| *w).sum();
            if total <= 0.0 {
                continue;
            }
            let normalised: Vec<(&[Trs], f32)> = covering.iter().map(|(p, w)| (*p, w / total)).collect();
            let blended = blend_poses(&normalised)[0];
            let m = total.min(1.0);
            out[j] = blend_poses(&[(std::slice::from_ref(&out[j]), 1.0 - m), (std::slice::from_ref(&blended), m)])[0];
        }
        out
    }

    /// Transforms of the filler ellipsoids for a local pose (relative to the hero root).
    pub fn filler_transforms(&self, local: &[Trs]) -> Vec<(Entity, Transform)> {
        let world = world_matrices(&self.skel, local);
        let p = |j: usize| Vec3::new(world[j][12], world[j][13], world[j][14]);
        self.filler
            .iter()
            .map(|f| {
                let (a, b) = (p(f.a), p(f.b));
                let d = b - a;
                let len = d.length();
                let up = if len > 1e-4 { d / len } else { Vec3::Y };
                let rot = Quat::from_rotation_arc(Vec3::Y, up);
                // half length: the segment plus a little at both ends so neighbours overlap
                (f.entity, Transform { translation: (a + b) * 0.5, rotation: rot, scale: Vec3::new(f.rx, len * 0.5 + f.rx.min(f.rz) * 0.5, f.rz) })
            })
            .collect()
    }

    /// Skins the model with a local pose and writes the positions into every piece's mesh.
    /// The skinned vertex positions of a local pose (every vertex of the model, indexed like ind_pos).
    pub fn posed(&self, local: &[Trs]) -> Vec<[f32; 3]> {
        let world = world_matrices(&self.skel, local);
        let sk = skin_matrices(&self.skel, &world);
        skin_positions_blend(&self.bind_pos, &self.joints, &self.joints2, &self.weight, &self.local, &sk, &world)
    }

    pub fn apply(&self, local: &[Trs], meshes: &mut Assets<Mesh>) {
        let out = self.posed(local);
        for p in &self.pieces {
            if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
                let pos: Vec<[f32; 3]> = p.src.iter().map(|&v| out[v as usize]).collect();
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            }
        }
    }
}












