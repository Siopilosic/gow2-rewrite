//! Bevy front end for the God of War II Rust port. Core logic stays in engine-free crates
//! (`gow2-formats`, `gow2-skel`, `gow2-kratos`); this crate only loads assets into Bevy and feeds input to them.

pub mod audio;
pub mod blades;
pub mod fx;
pub mod gpu;
pub mod hero;
pub mod hud;
pub mod level;
pub mod particles;
pub mod soldier;
pub mod subweapon;
pub mod volumes;





