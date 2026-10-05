//! The three sub-weapons (`R_S_BONE0`, `R_S_HAMMER0`, `R_S_OLYMPUS0`): the weapon object in Kratos's hand and its attack volumes.
//!
//! The data names them `Bone`, `Hammer` and `Olympus` (the moves `MOV_BoneSquare01`, the attachments `ATT_Bone` ..., the object models `bone`, `hammer`, `olympus`); no retail
//! name is assigned. Each WAD holds the weapon's model and rig (`MDL_bone`: 12 joints and its own clips with Kratos's clip names, `MDL_hammer`, `MDL_olympus`: rigid), the clips
//! Kratos plays while holding it (`ANM_Hero_Weapon*`: `wpnBoneSlash01`, `wpnBoneIdle`, ... loaded into the hero by `hero::spawn_hero`) and the collision balls (`CDV_gobone`,
//! `CDV_gohammer`, `CDV_goolympus`) whose material ids (12, 11, 10) are the volume ids the moves' hit windows select. The attachment record gives the hand joint `RWeapIH` for
//! both slots with a snap distance of 0, so the weapon always sits at that joint's matrix (`docs/animation.md`, "Blade attachment").

use std::path::Path;

use bevy::prelude::*;
use gow2_formats::{cdv, wad};
use gow2_skel::{transform_point, world_matrices};

use crate::hero::{spawn_hero, Hero};

/// The data's names, in the order of `Progress::sub_weapon` (0 Bone, 1 Hammer, 2 Olympus).
pub const NAMES: [&str; 3] = ["Bone", "Hammer", "Olympus"];
const FILES: [(&str, &str, &str); 3] = [("R_S_BONE0.WAD", "bone", "CDV_gobone"), ("R_S_HAMMER0.WAD", "hammer", "CDV_gohammer"), ("R_S_OLYMPUS0.WAD", "olympus", "CDV_goolympus")];

pub struct SubWeapon {
    pub hero: Hero,
    hull: Option<cdv::BallHull>,
}

#[derive(Resource)]
pub struct SubWeapons {
    pub items: Vec<SubWeapon>,
}

/// Loads the weapons that exist next to the hero WAD (all hidden).
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, dir: &Path, clip_names: &[&str]) -> Option<SubWeapons> {
    let mut items = Vec::new();
    for (file, model, hull_name) in FILES {
        let path = dir.join(file);
        let Ok(data) = std::fs::read(&path) else { return None };
        let recs: Vec<wad::Record> = wad::records(&data).collect();
        let hull = recs.iter().find(|r| r.tag == wad::Tag::Object && r.name == hull_name && !r.data.is_empty()).and_then(|r| cdv::parse(r.data));
        let hero = spawn_hero(commands, meshes, materials, images, &path.to_string_lossy(), model, clip_names);
        commands.entity(hero.root).insert(Visibility::Hidden);
        println!("sub-weapon {}: {} clips, {} balls", NAMES[items.len()], hero.clips.len(), hull.as_ref().map_or(0, |h| h.balls.len()));
        items.push(SubWeapon { hero, hull });
    }
    Some(SubWeapons { items })
}

fn axis_len(m: &[f32; 16]) -> f32 {
    (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt()
}

impl SubWeapon {
    /// Shows the weapon at `attach` (the hand joint's matrix in the hero's model space, carried to the world by `root`) in the pose of the clips Kratos plays.
    #[allow(clippy::too_many_arguments)]
    pub fn place(&self, root: &Mat4, attach: &[f32; 16], layers: &[(&str, f32, f32)], shown: bool, meshes: &mut Assets<Mesh>, tr: &mut Query<&mut Transform>, vis: &mut Query<&mut Visibility>) {
        if let Ok(mut v) = vis.get_mut(self.hero.root) {
            *v = if shown { Visibility::Inherited } else { Visibility::Hidden };
        }
        if !shown {
            return;
        }
        let local = self.hero.pose(&self.mapped(layers));
        self.hero.apply(&local, meshes);
        if let Ok(mut t) = tr.get_mut(self.hero.root) {
            *t = Transform::from_matrix(*root * Mat4::from_cols_array(attach));
        }
    }

    /// The layers of Kratos that the weapon has a clip for (same names), with the weapon's own clip names.
    fn mapped<'a>(&self, layers: &[(&'a str, f32, f32)]) -> Vec<(&'a str, f32, f32)> {
        layers.iter().copied().filter(|(n, _, _)| self.hero.clips.contains_key(*n)).collect()
    }

    /// The attack balls in the hero's model space: `(volume id, centre, radius)`.
    pub fn balls(&self, attach: &[f32; 16], layers: &[(&str, f32, f32)]) -> Vec<(u32, [f32; 3], f32)> {
        let Some(h) = &self.hull else { return Vec::new() };
        let local = self.hero.pose(&self.mapped(layers));
        let world = world_matrices(&self.hero.skel, &local);
        let scale_attach = axis_len(attach);
        h.balls
            .iter()
            .filter_map(|b| {
                let m = world.get(b.joint as usize)?;
                let c = transform_point(attach, transform_point(m, b.centre));
                Some((h.id_of(b), c, b.radius * axis_len(m) * scale_attach))
            })
            .collect()
    }
}
