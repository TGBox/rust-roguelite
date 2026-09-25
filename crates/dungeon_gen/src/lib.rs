//! Engine-unabhängige Kernlogik des Roguelites.
//!
//! Alles hier ist reines Rust ohne Bevy. Die Spiel-Crate übersetzt diese
//! Datenmodelle in Entities; umgekehrt weiß diese Crate nichts von Rendering.

pub mod collision;
pub mod grid;
pub mod room;

pub use grid::{Direction, GridPos};
pub use room::{RoomLayout, Tile};
