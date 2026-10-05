//! Kratos's own collision balls (`CDV_gohero`): fists, feet, body and the throw ball, each at its joint. They are the attack volumes of
//! the moves that use them (`docs/combat.md` 2.1); the blades' balls come from `crate::blades`.

use gow2_formats::{cdv, wad};
use gow2_kratos::blades::Mat;
use gow2_skel::{transform_point, Skeleton};

/// One ball: its volume id, the joint it hangs on, its centre in that joint's frame and its radius.
struct Ball {
    id: u32,
    joint: usize,
    centre: [f32; 3],
    radius: f32,
    prev: Option<[f32; 3]>,
}

pub struct BodyBalls {
    balls: Vec<Ball>,
}

impl BodyBalls {
    /// Reads `CDV_gohero` from the hero WAD; the joint indices must exist in `skel`.
    pub fn load(hero_wad: &str, skel: &Skeleton) -> Option<BodyBalls> {
        let data = std::fs::read(hero_wad).ok()?;
        let recs: Vec<wad::Record> = wad::records(&data).collect();
        let r = recs.iter().find(|r| r.tag == wad::Tag::Object && r.name == "CDV_gohero" && !r.data.is_empty())?;
        let h = cdv::parse(r.data)?;
        let balls: Vec<Ball> = h
            .balls
            .iter()
            .filter(|b| (b.joint as usize) < skel.len())
            .map(|b| Ball { id: h.id_of(b), joint: b.joint as usize, centre: b.centre, radius: b.radius, prev: None })
            .collect();
        println!("body balls: {} from CDV_gohero", balls.len());
        (!balls.is_empty()).then_some(BodyBalls { balls })
    }

    /// The balls in model space for these world matrices: `(volume id, centre, centre one step ago, radius)`.
    pub fn step(&mut self, world: &[Mat]) -> Vec<(u32, [f32; 3], [f32; 3], f32)> {
        self.balls
            .iter_mut()
            .map(|b| {
                let c = transform_point(&world[b.joint], b.centre);
                let prev = b.prev.unwrap_or(c);
                b.prev = Some(c);
                (b.id, c, prev, b.radius)
            })
            .collect()
    }
}
