//! Effects of God of War II on the CPU, without any engine types: the particle shapes, emitters and effect groups of a WAD ([`bank`]) and their playback ([`play`]).
//!
//! The game evaluates every particle on VU1 with a small program held in the shape record (`docs/particles.md`); this crate evaluates the same lists and data table
//! on the CPU and hands the front end sprites (position, size, angle, colour).

pub mod bank;
pub mod play;
