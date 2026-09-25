//! Engine-unabhängige Kernlogik des Roguelites.
//!
//! Alles hier ist reines Rust ohne Bevy und ohne externe Crates. Die
//! Spiel-Crate übersetzt diese Datenmodelle in Entities; umgekehrt weiß diese
//! Crate nichts von Rendering.

pub mod collision;
pub mod floor;
pub mod grid;
pub mod rng;
pub mod room;
pub mod spawns;
pub mod templates;

pub use floor::{Floor, RoomInfo, RoomKind};
pub use grid::{Direction, GridPos};
pub use rng::{Rng, RunSeed};
pub use room::{RoomLayout, Tile};
