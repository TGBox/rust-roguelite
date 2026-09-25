//! Der aktuelle Raum: Layout als Ressource, Kacheln als Entities,
//! und die Umrechnung zwischen Welt- und Kachelkoordinaten.
//!
//! In M4 wird der feste Testraum durch Räume aus dem Etagen-Generator ersetzt.

use bevy::prelude::*;
use dungeon_gen::{
    GridPos, RoomLayout, Tile,
    room::{ROOM_HEIGHT, ROOM_WIDTH},
};

use crate::{TILE_SIZE, assets::GameAssets};

const TEST_ROOM: &str = "
    ###############
    #.............#
    #..oo.....__..#
    #..o......__..#
    #.............#
    #..o.......o..#
    #..oo.....ooo.#
    #.............#
    ###############
";

pub struct RoomPlugin;

impl Plugin for RoomPlugin {
    fn build(&self, app: &mut App) {
        let layout = RoomLayout::from_ascii(TEST_ROOM).expect("Testraum muss gültig sein");
        app.insert_resource(CurrentRoom(layout))
            .add_systems(Startup, spawn_room_tiles);
    }
}

/// Das Layout des Raums, in dem der Spieler gerade ist.
/// Die Kollision fragt hier nach – nicht bei den Kachel-Entities.
#[derive(Resource)]
pub struct CurrentRoom(pub RoomLayout);

/// Markiert die gerenderten Kacheln (zum späteren Aufräumen beim Raumwechsel).
#[derive(Component)]
pub struct RoomTile;

// --- Koordinaten -----------------------------------------------------------
//
// Weltraum:  Pixel, Raum zentriert um (0, 0).
// Kachelraum: Kachel (i, j) belegt [i, i+1) × [j, j+1). Das erwartet `dungeon_gen::collision`.

fn room_half_size() -> Vec2 {
    Vec2::new(ROOM_WIDTH as f32, ROOM_HEIGHT as f32) / 2.0
}

pub fn world_to_tile_space(world: Vec2) -> Vec2 {
    world / TILE_SIZE + room_half_size()
}

pub fn tile_space_to_world(tile: Vec2) -> Vec2 {
    (tile - room_half_size()) * TILE_SIZE
}

/// Weltposition der Mitte einer Kachel.
pub fn tile_center(pos: GridPos) -> Vec2 {
    tile_space_to_world(Vec2::new(pos.x as f32, pos.y as f32) + 0.5)
}

fn spawn_room_tiles(mut commands: Commands, room: Res<CurrentRoom>, assets: Res<GameAssets>) {
    for (pos, tile) in room.0.iter() {
        let material = match tile {
            Tile::Floor => &assets.floor,
            Tile::Wall => &assets.wall,
            Tile::Rock => &assets.rock,
            Tile::Pit => &assets.pit,
        };
        commands.spawn((
            RoomTile,
            Mesh2d(assets.tile_mesh.clone()),
            MeshMaterial2d(material.clone()),
            // z = 0: Kacheln liegen unter allem anderen.
            Transform::from_translation(tile_center(pos).extend(0.0)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_center_tile_is_world_origin() {
        assert_eq!(tile_center(GridPos::new(7, 4)), Vec2::ZERO);
    }

    #[test]
    fn conversions_are_inverse() {
        let world = Vec2::new(-123.5, 42.25);
        let back = tile_space_to_world(world_to_tile_space(world));
        assert!((back - world).length() < 1e-3);
    }
}
