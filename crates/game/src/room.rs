//! Der aktuelle Raum: Layout und Türzustand als Ressource, Kacheln als
//! Entities, Umrechnung zwischen Welt- und Kachelkoordinaten.
//!
//! Jeder Raum der Etage hat einen festen Platz in der Welt (sein **Ursprung**),
//! passend zu seiner Position im Etagenraster. Der Startraum liegt bei (0, 0).
//! Beim Raumwechsel schwenkt die Kamera einfach zum Nachbarraum.

use bevy::prelude::*;
use dungeon_gen::{
    GridPos, RoomLayout, Tile,
    floor::START_POS,
    room::{ROOM_HEIGHT, ROOM_WIDTH},
};

use crate::{
    TILE_SIZE, assets::GameAssets, enemy::Enemy, physics::BodyKind, run::Run, schedule::GameSet,
    states::AppState,
};

/// Größe eines Raums in Weltkoordinaten.
pub const ROOM_SIZE: Vec2 = Vec2::new(
    ROOM_WIDTH as f32 * TILE_SIZE,
    ROOM_HEIGHT as f32 * TILE_SIZE,
);

pub struct RoomPlugin;

impl Plugin for RoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(AppState::InGame), leave_room)
            .add_systems(FixedUpdate, unlock_when_cleared.in_set(GameSet::Cleanup))
            // Nur wenn sich `CurrentRoom` geändert hat (z. B. Türen auf/zu).
            .add_systems(
                Update,
                update_door_visuals.run_if(resource_exists_and_changed::<CurrentRoom>),
            );
    }
}

/// Der Raum, in dem der Spieler gerade ist.
/// Die Kollision fragt hier nach – nicht bei den Kachel-Entities.
#[derive(Resource, Debug)]
pub struct CurrentRoom {
    /// Position im Etagenraster.
    pub pos: GridPos,
    pub layout: RoomLayout,
    /// Türen zu, solange Gegner im Raum sind.
    pub locked: bool,
}

impl CurrentRoom {
    /// Weltposition der Raummitte.
    pub fn origin(&self) -> Vec2 {
        room_origin(self.pos)
    }

    // Weltraum:   Pixel.
    // Kachelraum: Kachel (i, j) belegt [i, i+1) × [j, j+1). Das erwartet `dungeon_gen::collision`.

    pub fn world_to_tile_space(&self, world: Vec2) -> Vec2 {
        (world - self.origin()) / TILE_SIZE + room_half_size()
    }

    pub fn tile_space_to_world(&self, tile: Vec2) -> Vec2 {
        (tile - room_half_size()) * TILE_SIZE + self.origin()
    }

    /// Weltposition der Mitte einer Kachel.
    pub fn tile_center(&self, pos: GridPos) -> Vec2 {
        self.tile_space_to_world(Vec2::new(pos.x as f32, pos.y as f32) + 0.5)
    }

    /// Kachel unter einer Weltposition.
    pub fn tile_at(&self, world: Vec2) -> GridPos {
        let t = self.world_to_tile_space(world).floor();
        GridPos::new(t.x as i32, t.y as i32)
    }

    /// Blockiert die Kachel diese Art von Körper? Verschlossene Türen
    /// sind für Läufer wie Wände.
    pub fn blocks(&self, pos: GridPos, kind: BodyKind) -> bool {
        match kind {
            BodyKind::Walker => {
                self.layout.blocks_movement(pos)
                    || (self.locked && self.layout.get(pos) == Some(Tile::Door))
            }
            BodyKind::Projectile => self.layout.blocks_projectiles(pos),
        }
    }
}

fn room_half_size() -> Vec2 {
    Vec2::new(ROOM_WIDTH as f32, ROOM_HEIGHT as f32) / 2.0
}

/// Weltposition der Mitte des Raums an `pos` im Etagenraster.
pub fn room_origin(pos: GridPos) -> Vec2 {
    let rel = pos - START_POS;
    Vec2::new(rel.x as f32, rel.y as f32) * ROOM_SIZE
}

/// Markiert gerenderte Kacheln und merkt sich, zu welchem Raum sie gehören.
#[derive(Component, Debug)]
pub struct RoomTile {
    pub room: GridPos,
}

#[derive(Component)]
pub struct DoorTile;

/// Gehört zum aktuellen Raum und verschwindet beim Raumwechsel
/// (Projektile, Gegner). Kacheln werden separat über `RoomTile` verwaltet.
#[derive(Component)]
pub struct RoomScoped;

/// Hilfsfunktion statt System: wird beim Start und bei jedem Raumwechsel
/// aufgerufen. `&mut Commands` reicht, weil `Commands` Befehle nur sammelt.
pub fn spawn_room_tiles(commands: &mut Commands, room: &CurrentRoom, assets: &GameAssets) {
    for (pos, tile) in room.layout.iter() {
        let material = match tile {
            Tile::Floor => &assets.floor,
            Tile::Wall => &assets.wall,
            Tile::Rock => &assets.rock,
            Tile::Pit => &assets.pit,
            Tile::Door if room.locked => &assets.door_closed,
            Tile::Door => &assets.door_open,
        };
        let mut entity = commands.spawn((
            RoomTile { room: room.pos },
            DespawnOnExit(AppState::InGame),
            Mesh2d(assets.tile_mesh.clone()),
            MeshMaterial2d(material.clone()),
            // z = 0: Kacheln liegen unter allem anderen.
            Transform::from_translation(room.tile_center(pos).extend(0.0)),
        ));
        if tile == Tile::Door {
            entity.insert(DoorTile);
        }
    }
}

/// Erster Raum eines Runs. Läuft in der Kette aus `run.rs` nach `start_run`.
pub fn enter_first_room(mut commands: Commands, mut run: ResMut<Run>, assets: Res<GameAssets>) {
    let pos = run.floor.start();
    let layout = run.floor.room_layout(pos).expect("Startraum existiert");
    let room = CurrentRoom {
        pos,
        layout,
        locked: false,
    };
    run.visited.insert(pos);
    run.cleared.insert(pos);
    spawn_room_tiles(&mut commands, &room, &assets);
    commands.insert_resource(room);
}

fn leave_room(mut commands: Commands) {
    commands.remove_resource::<CurrentRoom>();
}

/// Kein Gegner mehr da? Türen öffnen und Raum als geräumt merken.
fn unlock_when_cleared(
    mut room: ResMut<CurrentRoom>,
    mut run: ResMut<Run>,
    enemies: Query<(), With<Enemy>>,
) {
    // Nur lesen löst keine Change Detection aus – erst das Schreiben unten.
    if room.locked && enemies.is_empty() {
        room.locked = false;
        run.cleared.insert(room.pos);
        info!("Raum {:?} geräumt – Türen offen", room.pos);
    }
}

fn update_door_visuals(
    room: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    mut doors: Query<(&RoomTile, &mut MeshMaterial2d<ColorMaterial>), With<DoorTile>>,
) {
    let material = if room.locked {
        &assets.door_closed
    } else {
        &assets.door_open
    };
    for (tile, mut mat) in &mut doors {
        if tile.room == room.pos {
            mat.0 = material.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room_at(pos: GridPos) -> CurrentRoom {
        CurrentRoom {
            pos,
            layout: RoomLayout::empty(),
            locked: false,
        }
    }

    #[test]
    fn start_room_center_is_world_origin() {
        let room = room_at(START_POS);
        assert_eq!(room.tile_center(dungeon_gen::room::CENTER), Vec2::ZERO);
    }

    #[test]
    fn neighbor_room_is_offset_by_room_size() {
        let east = room_at(START_POS + GridPos::new(1, 0));
        assert_eq!(east.origin(), Vec2::new(ROOM_SIZE.x, 0.0));
    }

    #[test]
    fn conversions_are_inverse_in_any_room() {
        let room = room_at(GridPos::new(2, 9));
        let world = Vec2::new(-1234.5, 842.25);
        let back = room.tile_space_to_world(room.world_to_tile_space(world));
        assert!((back - world).length() < 1e-3);
    }

    #[test]
    fn locked_doors_block_walkers_only() {
        use dungeon_gen::{Direction, room::door_pos};
        let mut room = room_at(START_POS);
        room.layout = RoomLayout::empty().with_doors([Direction::North]);
        let door = door_pos(Direction::North);
        assert!(!room.blocks(door, BodyKind::Walker));
        room.locked = true;
        assert!(room.blocks(door, BodyKind::Walker));
        assert!(room.blocks(door, BodyKind::Projectile));
    }
}
