//! The Rhodes soldier as an enemy: the game's own model and animation bank (`R_RHSOLD00.WAD`, model `rhsold00`, 18 joints, clips `navIdle`, `navWalkF`, `attChargeslice`,
//! `hitFront`, `death00` ...) standing in for the training dummies' capsules.
//!
//! The brain is the same simple melee loop the dummies use (`gow2_kratos::enemy`); the soldier's own behaviour records (`BHV_`) are not decoded (LOW). What is the game's
//! here is the look and the clips; which clip plays when is picked by name from the state of the victim and its brain.

use bevy::prelude::*;
use gow2_kratos::{combat::Victim, enemy::Phase};

use crate::hero::{spawn_hero, Hero};

/// The clips the soldier uses, all in the model's own `ANM_rhsold00`.
pub const CLIPS: [&str; 14] = [
    "navIdle", "navWalkF", "attChargeslice", "attCharge", "hitFront", "hitBack", "hitStagger", "hitAir", "hitLand", "hitGetUp", "hitLaunch", "death00", "death04", "death06",
];

/// The three death clips used (`death01` is two seconds long, longer than a dead dummy lies there).
const DEATHS: [&str; 3] = ["death00", "death04", "death06"];

/// Seconds a clip change is cross-faded over.
const FADE: f32 = 0.15;

/// What the animation needs to know about the soldier's state this tick.
pub struct Input<'a> {
    pub victim: &'a Victim,
    pub phase: Phase,
    /// Where Kratos stands (the soldier turns to face him).
    pub target: [f32; 3],
}

/// One soldier on screen: the model, and the animation state.
pub struct Soldier {
    pub hero: Hero,
    /// Heading in the sense of `Body::heading` (the facing is (-sin h, -cos h) in x, z); the soldier model looks along -z like Kratos.
    pub heading: f32,
    clip: &'static str,
    time: f32,
    rate: f32,
    looping: bool,
    prev: Option<(&'static str, f32)>,
    fade: f32,
    last_since: f32,
    dead: bool,
    was_airborne: bool,
}

/// The soldiers in play, one per victim (same order as `Sim::victims`).
#[derive(Resource)]
pub struct Soldiers(pub Vec<Soldier>);

/// Loads one soldier from the WAD at `path`; `None` if it is not there.
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, path: &str, heading: f32) -> Option<Soldier> {
    if !std::path::Path::new(path).exists() {
        return None;
    }
    let hero = spawn_hero(commands, meshes, materials, images, path, "rhsold00", &CLIPS);
    Some(Soldier { hero, heading, clip: "navIdle", time: 0.0, rate: 1.0, looping: true, prev: None, fade: 0.0, last_since: 9.0, dead: false, was_airborne: false })
}

impl Soldier {
    fn play(&mut self, clip: &'static str, looping: bool, rate: f32) {
        if self.clip == clip && self.looping == looping {
            return;
        }
        // (an empty name stands for "restart": nothing to fade from)
        if !self.clip.is_empty() {
            self.prev = Some((self.clip, self.time));
            self.fade = FADE;
        }
        self.clip = clip;
        self.time = 0.0;
        self.rate = rate;
        self.looping = looping;
    }

    fn duration(&self, clip: &str) -> f32 {
        self.hero.infos.get(clip).map_or(1.0, |i| i.duration.max(0.05))
    }

    /// Advances the animation state by `dt` and turns the soldier toward its target.
    pub fn step(&mut self, dt: f32, inp: &Input) {
        let v = inp.victim;
        // time of the current clip (a petrified soldier stands still)
        let frozen = v.petrify > 0.0;
        if !frozen {
            self.time += dt * self.rate;
            if let Some(p) = self.prev.as_mut() {
                p.1 += dt;
            }
            self.fade = (self.fade - dt).max(0.0);
        }
        let dur = self.duration(self.clip);
        let finished = !self.looping && self.time >= dur;
        if self.looping && self.time >= dur {
            self.time %= dur;
        }
        let hit_now = v.since_hit < self.last_since - 1e-4 && v.since_hit < 0.2;
        self.last_since = v.since_hit;

        if !v.alive() {
            if !self.dead {
                self.dead = true;
                self.play(DEATHS[v.id as usize % DEATHS.len()], false, 1.0);
            }
            return;
        }
        if self.dead {
            // the dummy got up again (respawned): back to standing
            self.dead = false;
            self.play("navIdle", true, 1.0);
        }
        if frozen {
            return;
        }
        // turn toward Kratos (not while thrown, down or staggered)
        let hurt = matches!(self.clip, "hitFront" | "hitBack" | "hitStagger" | "hitAir" | "hitLand" | "hitGetUp" | "hitLaunch") && !finished;
        if !hurt && !v.airborne {
            let want = (-(inp.target[0] - v.pos[0])).atan2(-(inp.target[2] - v.pos[2]));
            if (inp.target[0] - v.pos[0]).abs() + (inp.target[2] - v.pos[2]).abs() > 1.0 {
                let mut d = want - self.heading;
                while d > std::f32::consts::PI {
                    d -= std::f32::consts::TAU;
                }
                while d < -std::f32::consts::PI {
                    d += std::f32::consts::TAU;
                }
                self.heading += d * (1.0 - (-10.0 * dt).exp());
            }
        }
        // reactions first: thrown, landing, getting up, struck
        if v.airborne {
            self.was_airborne = true;
            if self.clip != "hitAir" && self.clip != "hitLaunch" {
                self.play("hitLaunch", false, 1.0);
            } else if self.clip == "hitLaunch" && finished {
                self.play("hitAir", true, 1.0);
            }
            return;
        }
        if self.was_airborne {
            self.was_airborne = false;
            self.play("hitLand", false, 1.0);
            return;
        }
        if hit_now {
            // a hit from the front (the soldier faces Kratos); a hard one staggers
            let clip = if v.vel[0].hypot(v.vel[1]) > 60.0 { "hitStagger" } else { "hitFront" };
            // restart even if it is the same clip: a second blow flinches again
            self.prev = Some((self.clip, self.time));
            self.fade = FADE;
            self.clip = "";
            self.play(clip, false, 1.0);
            return;
        }
        if self.clip == "hitLand" && finished {
            self.play("hitGetUp", false, 1.0);
            return;
        }
        if hurt {
            return;
        }
        match inp.phase {
            Phase::WindUp | Phase::Recover => {
                // the whole blow (wind-up and recovery) spans one `attChargeslice`
                let span = 0.55 + 0.7;
                let rate = self.duration("attChargeslice") / span;
                self.play("attChargeslice", false, rate);
            }
            Phase::Approach => self.play("navWalkF", true, 1.0),
            Phase::Idle => {
                if self.clip == "attChargeslice" && !finished {
                    return;
                }
                self.play("navIdle", true, 1.0);
            }
        }
    }

    /// The layers to pose the model with: the current clip and, while a change fades, the one before.
    pub fn layers(&self) -> Vec<(&str, f32, f32)> {
        let t = |clip: &str, time: f32| time.min(self.duration(clip) - 0.001).max(0.0);
        let mut out = Vec::new();
        // (the weights must add up to one over clips that exist: `Hero::pose` does not normalise them, and weights under one shrink the joint scales)
        if let (Some((p, pt)), true) = (self.prev.filter(|(p, _)| self.hero.clips.contains_key(*p)), self.fade > 0.0) {
            let w = self.fade / FADE;
            out.push((p, t(p, pt), w));
            out.push((self.clip, t(self.clip, self.time), 1.0 - w));
        } else {
            out.push((self.clip, t(self.clip, self.time), 1.0));
        }
        out
    }

    /// Writes the pose into the meshes, tints the pieces (a hurt flash, a stone grey) and returns the root entity with the transform to place it at.
    pub fn show(&self, v: &Victim, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> (Entity, Transform) {
        let local = self.hero.pose(&self.layers());
        self.hero.apply(&local, meshes);
        let hurt = (1.0 - v.since_hit / 0.18).clamp(0.0, 1.0);
        for p in &self.hero.pieces {
            if let Some(mut m) = materials.get_mut(&p.material) {
                let base = p.base.to_linear();
                m.base_color = if v.petrify > 0.0 {
                    Color::linear_rgba(base.red * 0.55, base.green * 0.55, base.blue * 0.55, base.alpha)
                } else {
                    Color::linear_rgba(base.red * (1.0 + 1.5 * hurt), base.green * (1.0 - 0.6 * hurt), base.blue * (1.0 - 0.6 * hurt), base.alpha)
                };
            }
        }
        (self.hero.root, Transform { translation: Vec3::from(v.pos), rotation: Quat::from_rotation_y(self.heading), scale: Vec3::ONE })
    }
}
