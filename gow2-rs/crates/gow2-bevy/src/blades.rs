//! Kratos's chained blades: the model, the chain, the trail and the collision balls, placed each tick by `gow2_kratos::blades`.
//!
//! `R_WEAPON0_<stage>.WAD` holds `MDL_MAIblade` with a one-joint rig (scale 1/64): its vertices are joint-local, so the rig's bind matrix
//! comes first (as for rigged level models), and the blade is then drawn at the matrix of whichever slot joint it follows. The same mesh
//! serves both hands.
//!
//! * **Chain.** The game builds 69 links in two strips between the chain joint (`LChain`, at the hand) and the blade (`docs/animation.md`,
//!   "Blade attachment"); how it places them is not decoded, so the links are laid along a sagging curve between the two ends, alternating
//!   between two perpendicular strips, with the game's `MAT_chainlink` texture (stand-in placement, MEDIUM).
//! * **Trail.** `docs/effects.md` 5.1: a ring of up to 160 samples, up to 10 sub-samples per frame, 0.32 s lifetime, alpha rising from the
//!   oldest sample to the newest, on while the blade is in the hand or out on the chain, with `MAT_swordtrail`. The two edge points of the
//!   ribbon are a viewer choice in the docs too: here the tip and a point 80 % along the blade.
//! * **Collision ball.** `CDV_gomaiblade`: one ball in the rig joint's frame, so at the bind scale 1/64 a sphere of radius 13.2 units around the
//!   middle of the blade.

use std::collections::{BTreeMap, VecDeque};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    camera::visibility::NoFrustumCulling,
};
use gow2_formats::{cdv, dc::Attachment, mdl, skin, texture::TextureStore, wad};
use gow2_kratos::blades::{Blade, Mat, Mode, Slots};
use gow2_skel::{transform_point, Skeleton};

use crate::hero::Hero;

/// Links in the chain (`GBL+0x98 / GBL+0x9c + 1`, RAM `+0x14c`).
const LINKS: usize = 69;
/// Samples the trail ring holds, its lifetime in seconds, and sub-samples per frame at most.
const TRAIL_SAMPLES: usize = 160;
const TRAIL_LIFE: f32 = 0.32;
const TRAIL_SUBSTEPS: usize = 4;

/// A mesh that is rewritten every frame.
struct Ribbon {
    mesh: Handle<Mesh>,
}

struct Sample {
    a: Vec3,
    b: Vec3,
    t: f32,
}

/// One blade in the world: its state and the entity that shows it.
pub struct BladeInst {
    pub blade: Blade,
    pub entity: Entity,
    /// The attack-volume id of this blade: 2 for the left (`weaponLeftMat`), 3 for the right (MEDIUM: the pair 2-3 of `docs/combat.md` 2.1).
    pub id: u32,
    /// The collision ball in blade-object space (through the rig's bind matrix) and its radius; `CDV_gomaiblade`.
    ball_local: [f32; 3],
    ball_radius: f32,
    /// The ball's centre now and one step ago, in model space.
    ball: [f32; 3],
    prev_ball: [f32; 3],
    started: bool,
    /// Where the chain starts, in model space, as of the last step.
    anchor: [f32; 3],
    chain: Option<Ribbon>,
    trail: Option<Ribbon>,
    /// The trail's two edge points in blade space.
    edge: [Vec3; 2],
    samples: VecDeque<Sample>,
    prev_edge: Option<[Vec3; 2]>,
}

#[derive(Resource)]
pub struct Blades {
    pub items: Vec<BladeInst>,
}

fn rgba_image(t: &gow2_formats::texture::Texture) -> Image {
    crate::gpu::texture_image(t, true)
}

fn joint(hero: &Hero, name: &str) -> Option<usize> {
    hero.skel.names.iter().position(|n| n.eq_ignore_ascii_case(name))
}

/// An unlit material for a MAT record of the weapon WAD: its texture, and the blend mode from the top byte of `+0x38`.
fn material(first: &BTreeMap<&str, &[u8]>, store: &TextureStore, images: &mut Assets<Image>, name: &str) -> StandardMaterial {
    let mut mat = StandardMaterial { unlit: true, cull_mode: None, perceptual_roughness: 1.0, ..default() };
    if let Some(mb) = first.get(name).filter(|m| m.len() == 120) {
        // the material's RGBA tint (+0x60) multiplies the vertex colour in the game's shader, except for the additive materials (blend byte 0x48): `MAT_blade3b` carries (0.7, 0, 0)
        // and the retail blade shows the cyan strokes of its texture untinted (PCSX2 frame, `docs/rust-port.md` "Model audit")
        let f = |o: usize| f32::from_le_bytes([mb[o], mb[o + 1], mb[o + 2], mb[o + 3]]);
        if mb[0x3b] != 0x48 {
            mat.base_color = Color::linear_rgba(f(0x60), f(0x64), f(0x68), 1.0);
        }
        match u32::from_le_bytes([mb[0x38], mb[0x39], mb[0x3a], mb[0x3b]]) >> 24 {
            0x48 => mat.alpha_mode = AlphaMode::Add,
            0x42 => mat.alpha_mode = AlphaMode::Blend,
            _ => {}
        }
    }
    if let Some(t) = store.material_texture(name) {
        if matches!(mat.alpha_mode, AlphaMode::Opaque) && t.rgba.chunks(4).any(|p| p[3] < 255) {
            mat.alpha_mode = AlphaMode::Mask(0.5);
        }
        mat.base_color_texture = Some(images.add(rgba_image(&t)));
    }
    mat
}

/// An empty dynamic mesh (one degenerate triangle) for a ribbon.
fn empty_ribbon(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; 3]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    meshes.add(mesh)
}

fn write_ribbon(meshes: &mut Assets<Mesh>, r: &Ribbon, pos: Vec<[f32; 3]>, uv: Vec<[f32; 2]>, col: Vec<[f32; 4]>, idx: Vec<u32>) {
    if let Some(mut m) = meshes.get_mut(&r.mesh) {
        if idx.is_empty() {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; 3]);
            m.insert_indices(Indices::U32(vec![0, 1, 2]));
        } else {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
            m.insert_indices(Indices::U32(idx));
        }
    }
}

/// Spawns one blade entity per attachment record under `parent` (Kratos's root). `None` when the weapon WAD or a joint is missing.
pub fn spawn_blades(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    weapon_wad: &str,
    hero: &Hero,
    atts: &[Attachment],
    parent: Entity,
) -> Option<Blades> {
    let data = std::fs::read(weapon_wad).ok()?;
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let rr = skin::find_rigs(&recs).into_iter().find(|r| r.model == "MAIblade")?;
    let skel = Skeleton::from_rig(&skin::parse_rig(rr.rig));
    let first: BTreeMap<&str, &[u8]> = recs.iter().filter(|r| r.tag == wad::Tag::Object && !r.data.is_empty()).fold(
        BTreeMap::new(),
        |mut m, r| {
            m.entry(r.name.as_str()).or_insert(r.data);
            m
        },
    );
    let sm = skin::mesh_joints(first.get("MDL_MAIblade_0")?, 4096.0);
    let mats = mdl::model_materials(&recs, "MAIblade");
    let store = TextureStore::new(recs.iter().filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)));
    // joint-local vertices go through the rig's bind matrix (the 1/64 scale)
    let pos: Vec<[f32; 3]> = sm
        .verts
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let p = [v[0] as f32 / 16.0, v[1] as f32 / 16.0, v[2] as f32 / 16.0];
            skel.bind_world.get(sm.joints[i] as usize).map_or(p, |m| transform_point(m, p))
        })
        .collect();
    // the blade's long axis (z) for the trail's edge points
    let (zmin, zmax) = pos.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[2]), hi.max(p[2])));
    let edge = [Vec3::new(0.0, 0.0, zmin), Vec3::new(0.0, 0.0, zmin + 0.8 * (zmax - zmin))];
    let mut slots: BTreeMap<u16, Vec<[u32; 3]>> = BTreeMap::new();
    for (t, s) in &sm.tris {
        slots.entry(*s).or_default().push(*t);
    }
    // one mesh and material per material slot, shared by both blades
    let mut parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>)> = Vec::new();
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
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, src.iter().map(|&v| pos[v as usize]).collect::<Vec<_>>());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, src.iter().map(|&v| [sm.uvs[v as usize][0] as f32, sm.uvs[v as usize][1] as f32]).collect::<Vec<_>>());
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_COLOR,
            src.iter()
                .map(|&v| {
                    let c = sm.cols[v as usize];
                    [c[0] as f32 / 128.0, c[1] as f32 / 128.0, c[2] as f32 / 128.0, c[3].min(128) as f32 / 128.0]
                })
                .collect::<Vec<_>>(),
        );
        mesh.insert_indices(Indices::U32(idx));
        let name = mats.get(slot as usize).cloned().unwrap_or_default();
        parts.push((meshes.add(mesh), materials.add(material(&first, &store, images, &name))));
    }
    let chain_mat = materials.add(material(&first, &store, images, "MAT_chainlink"));
    let trail_mat = {
        let mut m = material(&first, &store, images, "MAT_swordtrail");
        // the trail is a glowing ribbon: additive unless the record says otherwise
        if matches!(m.alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_)) {
            m.alpha_mode = AlphaMode::Add;
        }
        materials.add(m)
    };

    // the blade's collision ball: centre and radius are in the space of its rig joint, whose bind matrix carries the 1/64 scale
    let hull = recs.iter().find(|r| r.tag == wad::Tag::Object && r.name == "CDV_gomaiblade" && !r.data.is_empty()).and_then(|r| cdv::parse(r.data));
    let (ball_local, ball_radius) = hull
        .as_ref()
        .and_then(|h| h.balls.first())
        .map(|b| {
            let m = skel.bind_world.get(b.joint as usize).copied().unwrap_or(IDENTITY);
            let scale = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
            (transform_point(&m, b.centre), b.radius * scale)
        })
        .unwrap_or(([0.0, 0.0, -5.0], 13.2));
    let mut items = Vec::new();
    for (n, a) in atts.iter().filter(|a| a.chained).enumerate() {
        let (Some(back), Some(hand), Some(free), Some(chain)) =
            (joint(hero, &a.stowed), joint(hero, &a.hand), joint(hero, &a.free), joint(hero, &a.chain[0]))
        else {
            eprintln!("blade joints of {} not found in the rig", a.object);
            continue;
        };
        let entity = commands.spawn((Transform::default(), Visibility::default())).id();
        commands.entity(parent).add_child(entity);
        for (m, mat) in &parts {
            let piece = commands.spawn((Mesh3d(m.clone()), MeshMaterial3d(mat.clone()), Transform::default())).id();
            commands.entity(entity).add_child(piece);
        }
        // the chain and the trail live in world space, so they are not children of Kratos
        let chain_mesh = empty_ribbon(meshes);
        commands.spawn((Mesh3d(chain_mesh.clone()), MeshMaterial3d(chain_mat.clone()), Transform::default(), NoFrustumCulling));
        let trail_mesh = empty_ribbon(meshes);
        commands.spawn((Mesh3d(trail_mesh.clone()), MeshMaterial3d(trail_mat.clone()), Transform::default(), NoFrustumCulling));
        items.push(BladeInst {
            blade: Blade::new(Slots { back, hand, free, chain }, a.snap_m),
            entity,
            id: 2 + n as u32,
            ball_local,
            ball_radius,
            ball: [0.0; 3],
            prev_ball: [0.0; 3],
            started: false,
            anchor: [0.0; 3],
            chain: Some(Ribbon { mesh: chain_mesh }),
            trail: Some(Ribbon { mesh: trail_mesh }),
            edge,
            samples: VecDeque::new(),
            prev_edge: None,
        });
    }
    println!("blades: {} attached from {weapon_wad} ({} parts)", items.len(), parts.len());
    (!items.is_empty()).then_some(Blades { items })
}

const IDENTITY: Mat = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// Row-major game matrix to a Bevy transform (the game's rows are the axes, so the flat array read as columns is the same matrix).
fn to_transform(m: &Mat) -> Transform {
    Transform::from_matrix(Mat4::from_cols_array(m))
}

/// The chain's 70 points from the anchor to the blade: a sagging curve (the sag shrinks as the chain pulls tight).
fn chain_points(a: Vec3, b: Vec3) -> Vec<Vec3> {
    let sag = (b - a).length() * 0.12;
    (0..=LINKS)
        .map(|i| {
            let t = i as f32 / LINKS as f32;
            a.lerp(b, t) - Vec3::Y * (sag * 4.0 * t * (1.0 - t))
        })
        .collect()
}

/// The chain as 69 link quads, alternating between two perpendicular strips.
fn chain_mesh(pts: &[Vec3]) -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 4]>, Vec<u32>) {
    const HALF_WIDTH: f32 = 0.9;
    let (mut pos, mut uv, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for i in 0..pts.len() - 1 {
        let (p0, p1) = (pts[i], pts[i + 1]);
        let along = p1 - p0;
        if along.length_squared() < 1e-8 {
            continue;
        }
        let t = along.normalize();
        let up = if t.dot(Vec3::Y).abs() > 0.95 { Vec3::X } else { Vec3::Y };
        let n1 = t.cross(up).normalize();
        let n2 = t.cross(n1).normalize();
        let n = if i % 2 == 0 { n1 } else { n2 };
        // the links overlap their neighbours by a fifth
        let (a, b) = (p0 - along * 0.1, p1 + along * 0.1);
        let base = pos.len() as u32;
        for (p, u) in [(a, 0.0), (b, 1.0)] {
            pos.push((p - n * HALF_WIDTH).to_array());
            pos.push((p + n * HALF_WIDTH).to_array());
            uv.push([u, 0.0]);
            uv.push([u, 1.0]);
        }
        col.extend([[1.0f32; 4]; 4]);
        idx.extend([base, base + 1, base + 2, base + 1, base + 3, base + 2]);
    }
    (pos, uv, col, idx)
}

impl Blades {
    /// One simulation step: moves each blade with the skeleton's world matrices (`world`, model space) and tracks its collision ball.
    /// Puts every blade on its back slot (held there by `step` with `keep_mode`): for moves that use both hands for something else.
    pub fn stow(&mut self) {
        for it in &mut self.items {
            it.blade.mode = Mode::Back;
        }
    }

    pub fn step(&mut self, world: &[Mat], dt: f32, keep_mode: bool) {
        for it in &mut self.items {
            it.blade.update(world, dt, keep_mode);
            let c = transform_point(&it.blade.matrix, it.ball_local);
            it.prev_ball = if it.started { it.ball } else { c };
            it.ball = c;
            it.started = true;
            let a = &world[it.blade.slots.chain];
            it.anchor = [a[12], a[13], a[14]];
        }
    }

    /// The blades' collision balls in model space: `(volume id, centre, centre one step ago, radius)`.
    pub fn balls(&self) -> Vec<(u32, [f32; 3], [f32; 3], f32)> {
        self.items.iter().map(|i| (i.id, i.ball, i.prev_ball, i.ball_radius)).collect()
    }

    /// Shows the blades, their chains and their trails. `root` places model space in the world, `now` is the time in seconds, `dt` the
    /// frame time.
    pub fn draw(&mut self, root: &Transform, now: f32, meshes: &mut Assets<Mesh>, set: &mut dyn FnMut(Entity, Transform), gizmos: &mut Gizmos) {
        let _ = gizmos;
        for it in &mut self.items {
            set(it.entity, to_transform(&it.blade.matrix));
            let blade_world = Mat4::from_cols_array(&it.blade.matrix);
            let out = it.blade.mode != Mode::Back;

            // chain: hidden while the blade is stowed
            if let Some(r) = &it.chain {
                if out {
                    let pts: Vec<Vec3> = chain_points(Vec3::from(it.anchor), Vec3::from(it.blade.origin())).into_iter().map(|p| root.transform_point(p)).collect();
                    let (pos, uv, col, idx) = chain_mesh(&pts);
                    write_ribbon(meshes, r, pos, uv, col, idx);
                } else {
                    write_ribbon(meshes, r, vec![], vec![], vec![], vec![]);
                }
            }

            // trail: sub-samples between the last frame's edge points and this frame's, while the blade is out
            let edge_now = it.edge.map(|e| root.transform_point(blade_world.transform_point3(e)));
            if out {
                if let Some(prev) = it.prev_edge {
                    for k in 1..=TRAIL_SUBSTEPS {
                        let f = k as f32 / TRAIL_SUBSTEPS as f32;
                        it.samples.push_back(Sample { a: prev[0].lerp(edge_now[0], f), b: prev[1].lerp(edge_now[1], f), t: now });
                    }
                }
                it.prev_edge = Some(edge_now);
            } else {
                it.prev_edge = None;
            }
            while it.samples.front().map_or(false, |s| now - s.t > TRAIL_LIFE) || it.samples.len() > TRAIL_SAMPLES {
                it.samples.pop_front();
            }
            if let Some(r) = &it.trail {
                let n = it.samples.len();
                if n >= 2 {
                    let (mut pos, mut uv, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
                    for (i, s) in it.samples.iter().enumerate() {
                        let age = ((now - s.t) / TRAIL_LIFE).clamp(0.0, 1.0);
                        pos.push(s.a.to_array());
                        pos.push(s.b.to_array());
                        uv.push([1.0 - age, 0.0]);
                        uv.push([1.0 - age, 1.0]);
                        let alpha = (1.0 - age) * 0.9;
                        col.extend([[1.6, 1.6, 1.6, alpha]; 2]);
                        if i > 0 {
                            let b = (i * 2) as u32;
                            idx.extend([b - 2, b - 1, b, b - 1, b + 1, b]);
                        }
                    }
                    write_ribbon(meshes, r, pos, uv, col, idx);
                } else {
                    write_ribbon(meshes, r, vec![], vec![], vec![], vec![]);
                }
            }
        }
    }

    /// Modes for the title line: back, hand or free for each blade.
    pub fn modes(&self) -> String {
        self.items
            .iter()
            .map(|i| match i.blade.mode {
                Mode::Back => "back",
                Mode::Hand => "hand",
                Mode::Free => "free",
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chain_has_sixty_nine_links_between_its_ends() {
        let (a, b) = (Vec3::new(0.0, 10.0, 0.0), Vec3::new(40.0, 20.0, 0.0));
        let pts = chain_points(a, b);
        assert_eq!(pts.len(), LINKS + 1);
        assert!(pts[0].distance(a) < 1e-4 && pts[LINKS].distance(b) < 1e-4, "both ends are fixed");
        // it sags below the straight line in the middle
        let mid = pts[LINKS / 2];
        assert!(mid.y < (a.y + b.y) / 2.0 - 1.0, "mid {mid:?}");
        let (pos, uv, col, idx) = chain_mesh(&pts);
        assert_eq!((pos.len(), uv.len(), col.len(), idx.len()), (LINKS * 4, LINKS * 4, LINKS * 4, LINKS * 6));
    }

    #[test]
    fn neighbouring_links_lie_in_perpendicular_planes() {
        let pts = chain_points(Vec3::ZERO, Vec3::new(60.0, 0.0, 0.0));
        let (pos, ..) = chain_mesh(&pts);
        // the width direction of link i: from its first vertex pair
        let width = |i: usize| Vec3::from(pos[i * 4 + 1]) - Vec3::from(pos[i * 4]);
        let (w0, w1, w2) = (width(10), width(11), width(12));
        assert!(w0.normalize().dot(w1.normalize()).abs() < 0.05, "links 10 and 11 are crossed");
        assert!(w0.normalize().dot(w2.normalize()) > 0.95, "links 10 and 12 face the same way");
    }

    #[test]
    fn a_zero_length_chain_makes_no_links() {
        let pts = chain_points(Vec3::ONE, Vec3::ONE);
        let (_, _, _, idx) = chain_mesh(&pts);
        assert!(idx.is_empty());
    }
}


