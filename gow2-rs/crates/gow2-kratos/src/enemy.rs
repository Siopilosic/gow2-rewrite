//! A stand-in enemy brain for the training dummies: walk up to Kratos, wind up, strike, recover.
//!
//! The game's enemy behaviour (`BHV_*` behaviour records, the creature scripts) is not decoded, so this is a simple, fixed melee loop that gives the
//! player's defence (blocking, evading, being hit) something to act on. It is a test aid, not a port of any creature (LOW).

use crate::combat::Victim;

/// Seconds a struck enemy is staggered and does nothing.
const STAGGER: f32 = 0.45;
/// Reach of the strike, in units from centre to centre.
pub const STRIKE_REACH: f32 = 34.0;
const WALK_SPEED: f32 = 56.0;
const WIND_UP: f32 = 0.55;
const RECOVER: f32 = 0.7;
const COOLDOWN: f32 = 0.9;
/// Enemies further than this from Kratos stay where they are.
const SIGHT: f32 = 700.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Idle,
    Approach,
    WindUp,
    Recover,
}

/// A blow that landed (or was thrown): how hard and from where.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strike {
    pub damage: f32,
    /// Unit vector (x, z) from the enemy toward its target.
    pub dir: [f32; 2],
    /// Every third blow is a heavy one.
    pub heavy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Brain {
    pub phase: Phase,
    timer: f32,
    cooldown: f32,
    blows: u32,
}

impl Default for Brain {
    fn default() -> Self {
        Brain { phase: Phase::Idle, timer: 0.0, cooldown: 0.5, blows: 0 }
    }
}

impl Brain {
    /// One tick. Moves `me` toward `target` and returns a blow at the end of a wind-up if the target is still in reach.
    pub fn update(&mut self, dt: f32, me: &mut Victim, target: [f32; 3], enabled: bool) -> Option<Strike> {
        self.cooldown = (self.cooldown - dt).max(0.0);
        let (dx, dz) = (target[0] - me.pos[0], target[2] - me.pos[2]);
        let dist = (dx * dx + dz * dz).sqrt();
        let dir = if dist > 1e-3 { [dx / dist, dz / dist] } else { [0.0, 1.0] };
        if !enabled || !me.alive() || me.since_hit < STAGGER || me.airborne || (target[1] - me.pos[1]).abs() > 60.0 {
            if self.phase != Phase::Idle {
                self.phase = Phase::Idle;
            }
            return None;
        }
        match self.phase {
            Phase::Idle => {
                if dist < SIGHT && self.cooldown <= 0.0 {
                    self.phase = Phase::Approach;
                }
            }
            Phase::Approach => {
                if dist <= STRIKE_REACH - 6.0 {
                    self.phase = Phase::WindUp;
                    self.timer = WIND_UP;
                } else if dist > SIGHT {
                    self.phase = Phase::Idle;
                } else {
                    me.vel = [dir[0] * WALK_SPEED, dir[1] * WALK_SPEED];
                }
            }
            Phase::WindUp => {
                self.timer -= dt;
                if self.timer <= 0.0 {
                    self.phase = Phase::Recover;
                    self.timer = RECOVER;
                    self.blows += 1;
                    if dist <= STRIKE_REACH + 6.0 {
                        let heavy = self.blows % 3 == 0;
                        return Some(Strike { damage: if heavy { 18.0 } else { 8.0 }, dir, heavy });
                    }
                }
            }
            Phase::Recover => {
                self.timer -= dt;
                if self.timer <= 0.0 {
                    self.phase = Phase::Idle;
                    self.cooldown = COOLDOWN;
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(brain: &mut Brain, me: &mut Victim, target: [f32; 3], secs: f32) -> Vec<Strike> {
        let mut out = Vec::new();
        let dt = 1.0 / 60.0;
        for _ in 0..(secs / dt) as usize {
            if let Some(s) = brain.update(dt, me, target, true) {
                out.push(s);
            }
            me.pos[0] += me.vel[0] * dt;
            me.pos[2] += me.vel[1] * dt;
            me.vel = [0.0; 2];
            me.since_hit += dt;
        }
        out
    }

    #[test]
    fn it_walks_up_and_strikes_with_a_rhythm() {
        let mut me = Victim::new(1, [0.0, 0.0, 0.0], 100.0, 1.0);
        let mut b = Brain::default();
        let hits = run(&mut b, &mut me, [0.0, 0.0, 200.0], 12.0);
        assert!(hits.len() >= 3, "{} blows in 12 s", hits.len());
        // the third blow is heavy, the others light
        assert!(hits[2].heavy && !hits[0].heavy);
        // it ended up within reach of the target
        assert!((200.0 - me.pos[2]).abs() <= STRIKE_REACH);
    }

    #[test]
    fn a_disabled_or_staggered_enemy_does_nothing() {
        let mut me = Victim::new(1, [0.0; 3], 100.0, 1.0);
        let mut b = Brain::default();
        assert!(b.update(0.1, &mut me, [0.0, 0.0, 10.0], false).is_none());
        me.since_hit = 0.0;
        assert!(b.update(0.1, &mut me, [0.0, 0.0, 10.0], true).is_none());
        assert_eq!(b.phase, Phase::Idle);
    }
}

