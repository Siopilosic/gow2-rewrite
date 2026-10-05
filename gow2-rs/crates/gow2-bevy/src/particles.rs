//! Drawing the particle effects: `gow2_fx::play::Player` evaluates the particles of an effect, this module turns its sprites into camera-facing quads.
//!
//! One dynamic mesh per particle shape (all its live particles), with the shape's `MAT_` texture and the blend its flags select (`docs/particles.md` 3.4). The render
//! routines 3 (rotated billboard), 1 and 2 are drawn as camera-facing quads, 4 as a soft disc, 7 and 0 as small quads, like the viewer of `analysis/levels/particles.js`.

use std::collections::HashMap;
use std::path::Path;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use gow2_formats::{texture::TextureStore, wad};
use gow2_fx::bank::{Bank, Blend};
use gow2_fx::play::{Player, SIZE_UNIT};
use gow2_skel::Mat4;

const MAX_PER_SHAPE: usize = 1500;

struct Pool {
    mesh: Handle<Mesh>,
    entity: Entity,
}

struct Group {
    player: Player,
    pools: Vec<Pool>,
}

/// The magic WADs' effect banks and the meshes that draw them.
#[derive(Resource)]
pub struct Particles {
    groups: Vec<Group>,
    /// Effects that follow something: key -> (group, instance serial).
    live: HashMap<u64, (usize, u64)>,
}

/// Loads the effect banks of the WADs beside the hero WAD that have effects (`files`).
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, dir: &Path, files: &[&str]) -> Particles {
    let mut groups = Vec::new();
    for f in files {
        let Ok(data) = std::fs::read(dir.join(f)) else { continue };
        let bank = Bank::load(&data);
        if bank.shapes.is_empty() {
            continue;
        }
        let recs: Vec<wad::Record> = wad::records(&data).collect();
        let store = TextureStore::new(recs.iter().filter(|r| r.tag == wad::Tag::Object).map(|r| (r.name.clone(), r.data)));
        let mut pools = Vec::new();
        for sh in &bank.shapes {
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
            mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
            let handle = meshes.add(mesh);
            let mut mat = StandardMaterial { unlit: true, cull_mode: None, perceptual_roughness: 1.0, alpha_mode: AlphaMode::Blend, ..default() };
            mat.alpha_mode = match sh.blend() {
                Blend::Normal => AlphaMode::Blend,
                Blend::Additive => AlphaMode::Add,
                // subtractive: not drawn (see `update`)
                Blend::Subtractive => AlphaMode::Blend,
            };
            // the material's RGBA tint (MAT +0x60, `MAT_pticleMat` has 2.0) multiplies the colour as in the models
            if let Some(mb) = sh.material.as_deref().and_then(|m| recs.iter().find(|r| r.name == m && r.data.len() == 120)) {
                let fl = |o: usize| f32::from_le_bytes([mb.data[o], mb.data[o + 1], mb.data[o + 2], mb.data[o + 3]]);
                mat.base_color = Color::linear_rgba(fl(0x60), fl(0x64), fl(0x68), 1.0);
            }
            if sh.render == 4 {
                // a disc is a gouraud fan without a texture (`update` builds its vertices)
            } else if let Some(t) = sh.material.as_deref().and_then(|m| store.material_texture(m)) {
                mat.base_color_texture = Some(images.add(crate::gpu::texture_image(&t, true)));
            }
            if std::env::var_os("GOW_PFXNOTEX").is_some() {
                mat.base_color_texture = None;
            }
            if std::env::var_os("GOW_PFXBLEND").is_some() {
                mat.alpha_mode = AlphaMode::Blend;
            }
            if std::env::var_os("GOW_PFXDEBUG").is_some() {
                // every sprite an opaque magenta quad: shows where the particles are and how big
                mat = StandardMaterial { unlit: true, cull_mode: None, base_color: Color::srgb(1.0, 0.0, 1.0), ..default() };
            }
            let entity = commands.spawn((Mesh3d(handle.clone()), MeshMaterial3d(materials.add(mat)), Transform::default(), Visibility::Inherited, bevy::camera::visibility::NoFrustumCulling)).id();
            pools.push(Pool { mesh: handle, entity });
        }
        let mut player = Player::new(bank);
        player.attach_rigs(&data);
        println!("particles {f}: {} shapes, {} effects", player.bank.shapes.len(), player.bank.effects.len());
        groups.push(Group { player, pools });
    }
    Particles { groups, live: HashMap::new() }
}

impl Particles {
    /// Starts an effect by name at `root` (row-vector placement); returns false if no bank has it.
    pub fn start(&mut self, name: &str, root: Mat4, duration: Option<f32>) -> bool {
        for g in &mut self.groups {
            if g.player.start(name, root, duration) {
                return true;
            }
        }
        false
    }

    /// Starts an effect that keeps following something under `key`; later calls with the same key move it. `end` stops its emission.
    pub fn follow(&mut self, key: u64, name: &str, root: Mat4, end: bool) {
        self.follow_filtered(key, name, root, end, &[]);
    }

    /// [`Particles::follow`] with only the emitters whose names start with one of `prefixes` (all of them when the list is empty).
    pub fn follow_filtered(&mut self, key: u64, name: &str, root: Mat4, end: bool, prefixes: &[&str]) {
        if let Some(&(gi, serial)) = self.live.get(&key) {
            let g = &mut self.groups[gi];
            if let Some(i) = g.player.instances.iter().position(|i| i.serial == serial) {
                if end {
                    g.player.stop(i);
                    self.live.remove(&key);
                } else {
                    g.player.set_root(i, root);
                }
                return;
            }
            self.live.remove(&key);
        }
        if end {
            return;
        }
        for (gi, g) in self.groups.iter_mut().enumerate() {
            if let Some(serial) = g.player.start_filtered(name, root, Some(1e9), prefixes) {
                self.live.insert(key, (gi, serial));
                return;
            }
        }
    }

    /// Keeps the following effects in step with the things they follow: every item is started or moved, every effect whose key is missing is ended.
    pub fn sync_follow(&mut self, items: &[(u64, &str, Mat4)]) {
        let with: Vec<(u64, &str, Mat4, &[&str])> = items.iter().map(|&(k, n, m)| (k, n, m, &[] as &[&str])).collect();
        self.sync_follow_filtered(&with);
    }

    /// [`Particles::sync_follow`] where each item also names the emitter prefixes that may emit.
    pub fn sync_follow_filtered(&mut self, items: &[(u64, &str, Mat4, &[&str])]) {
        let keys: Vec<u64> = self.live.keys().copied().collect();
        for k in keys {
            if !items.iter().any(|i| i.0 == k) {
                self.follow(k, "", [0.0; 16], true);
            }
        }
        for (k, n, m, p) in items {
            self.follow_filtered(*k, n, *m, false, p);
        }
    }

    pub fn live_particles(&self) -> usize {
        self.groups.iter().map(|g| g.player.live()).sum()
    }
}

/// The placement for a magic effect: the ones that shoot along their own -z axis (the Medusa flash, the wind gust, the bomb) look along `dir`, the others stand upright.
pub fn place(name: &str, pos: Vec3, dir: Vec3, scale: f32) -> Mat4 {
    if matches!(name, "gomedusaflash" | "gowindgust" | "gomedusabomb") {
        root_look(pos, dir, scale)
    } else {
        root_up(pos, Vec3::Y, scale)
    }
}

/// A row-vector placement matrix: scale, a turn about the y axis and a position.
pub fn root(pos: Vec3, yaw: f32, scale: f32) -> Mat4 {
    let (s, c) = yaw.sin_cos();
    [c * scale, 0.0, -s * scale, 0.0, 0.0, scale, 0.0, 0.0, s * scale, 0.0, c * scale, 0.0, pos.x, pos.y, pos.z, 1.0]
}

/// A row-vector placement whose local +y axis points along `up` (for effects that shoot along their own y), with a position and a scale.
pub fn root_up(pos: Vec3, up: Vec3, scale: f32) -> Mat4 {
    let rot = Quat::from_rotation_arc(Vec3::Y, up.normalize_or_zero());
    let (x, y, z) = (rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z);
    [x.x * scale, x.y * scale, x.z * scale, 0.0, y.x * scale, y.y * scale, y.z * scale, 0.0, z.x * scale, z.y * scale, z.z * scale, 0.0, pos.x, pos.y, pos.z, 1.0]
}

/// A row-vector placement whose local -z axis points along `dir` (the forward of the game's effect models and of the effects that shoot along it, like the Medusa flash).
pub fn root_look(pos: Vec3, dir: Vec3, scale: f32) -> Mat4 {
    let rot = Quat::from_rotation_arc(Vec3::NEG_Z, dir.normalize_or_zero());
    let (x, y, z) = (rot * Vec3::X, rot * Vec3::Y, rot * Vec3::Z);
    [x.x * scale, x.y * scale, x.z * scale, 0.0, y.x * scale, y.y * scale, y.z * scale, 0.0, z.x * scale, z.y * scale, z.z * scale, 0.0, pos.x, pos.y, pos.z, 1.0]
}

/// Steps the effects and rewrites the quads of every shape (a system: runs after the camera is placed).
pub fn update(time: Res<Time>, mut p: Option<ResMut<Particles>>, camera: Query<(&Transform, &Projection), With<Camera3d>>, mut meshes: ResMut<Assets<Mesh>>) {
    let Some(p) = p.as_deref_mut() else { return };
    let Ok((cam, proj)) = camera.single() else { return };
    let (right, up) = (cam.rotation * Vec3::X, cam.rotation * Vec3::Y);
    // the half extent of a size-1 particle is SIZE_UNIT * tan(fov_x / 2) of the game's own camera (0.5143), so effects keep their size against Kratos whatever our field of view is
    let tan_x = 0.5143f32;
    let _ = proj;
    let unit = SIZE_UNIT * tan_x;
    let dt = time.delta_secs();
    for g in &mut p.groups {
        g.player.update(dt);
        let sprites = g.player.sprites();
        if std::env::var_os("GOW_LOGFX2").is_some() && !sprites.is_empty() {
            let s = &sprites[0];
            println!("particles: {} sprites, first shape {} at {:.0?} size {:.1} rgba {:.2?}", sprites.len(), g.player.shape(s.shape).name, s.pos, s.size * unit, s.rgba);
        }
        let mut per: Vec<Vec<usize>> = vec![Vec::new(); g.pools.len()];
        for (i, s) in sprites.iter().enumerate() {
            // subtractive shapes (`Cd - Cs * As`, the dark specks of the wind hit) have no blend mode in the standard material, and multiplying draws them as black blocks: not drawn
            if g.player.blend(s.shape) == Blend::Subtractive {
                continue;
            }
            per[s.shape].push(i);
        }
        for (si, idxs) in per.iter().enumerate() {
            let Some(mut mesh) = meshes.get_mut(&g.pools[si].mesh) else { continue };
            if idxs.is_empty() {
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
                mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
                mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
                continue;
            }
            let n = idxs.len().min(MAX_PER_SHAPE);
            let (mut pos, mut uv, mut col, mut ind) = (Vec::with_capacity(n * 4), Vec::with_capacity(n * 4), Vec::with_capacity(n * 4), Vec::with_capacity(n * 6));
            for (k, &i) in idxs.iter().take(n).enumerate() {
                let s = &sprites[i];
                let size = s.size * unit;
                if g.player.shape(s.shape).render == 4 {
                    // a fan: the centre in the sprite's colour, eight rim points (an octagon, as the game's) in the rim colour, facing the camera
                    let c = Vec3::from(s.pos);
                    let base = pos.len() as u32;
                    pos.push(c.to_array());
                    uv.push([0.5, 0.5]);
                    col.push(s.rgba);
                    for j in 0..8 {
                        let a = j as f32 * std::f32::consts::TAU / 8.0;
                        let q = c + (right * a.cos() + up * a.sin()) * size;
                        pos.push(q.to_array());
                        uv.push([0.5, 0.5]);
                        col.push(s.rim);
                    }
                    for j in 0..8u32 {
                        ind.extend_from_slice(&[base, base + 1 + j, base + 1 + (j + 1) % 8]);
                    }
                    continue;
                }
                let (ca, sa) = (s.angle.cos() * size, s.angle.sin() * size);
                let corners = [[ca, sa], [-sa, ca], [sa, -ca], [-ca, -sa]];
                let c = Vec3::from(s.pos);
                for (j, cn) in corners.iter().enumerate() {
                    let (a, b) = (cn[0] - cn[1], cn[0] + cn[1]);
                    let q = c + right * a + up * b;
                    pos.push([q.x, q.y, q.z]);
                    // strip order (u1, v0) (u0, v0) (u1, v1) (u0, v1), docs/particles.md 3.3
                    uv.push([[1.0, 0.0], [0.0, 0.0], [1.0, 1.0], [0.0, 1.0]][j]);
                    col.push(s.rgba);
                }
                let b = (k * 4) as u32;
                ind.extend_from_slice(&[b, b + 1, b + 2, b + 2, b + 1, b + 3]);
            }
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
            mesh.insert_indices(Indices::U32(ind));
        }
    }
}
