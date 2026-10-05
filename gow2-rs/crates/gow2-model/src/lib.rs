//! Recovered data model of the God of War II (SCUS-97481) engine.
//!
//! **This crate is not an engine implementation.** It records facts recovered from
//! the executable (tables, layouts, dispatch rules) so that tools can check them
//! against disc data. Every item cites the binary address it was taken from; the
//! evidence and confidence for each claim are in `docs/confirmed.md` and
//! `docs/hypotheses.md` at the project root.

pub mod objects;
pub mod routing;
pub mod servers;
pub mod wad_tags;
