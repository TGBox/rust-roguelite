//! Der aktuelle Raum: Layout und Türzustand als Ressource, Kacheln als
//! Entities, Umrechnung zwischen Welt- und Kachelkoordinaten.
//!
//! Jeder Raum der Etage hat einen festen Platz in der Welt (sein **Ursprung**),
//! passend zu seiner Position im Etagenraster. Der Startraum liegt bei (0, 0).
//! Beim Raumwechsel schwenkt die Kamera einfach zum Nachbarraum.

use bevy::prelude::*;
use dungeon_gen::{
    Direction, GridPos, RoomLayout, Tile,
    floor::START_POS,
    loot::requires_key,
    room::{ROOM_HEIGHT, ROOM_WIDTH, door_direction, door_pos},
};

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    enemy::{Enemy, spawn_room_enemies},
    inventory::{self, Toast},
    item_db::ItemDatabase,
    physics::{BodyKind, Position},
    player::Player,
    progress,
    run::{ResumeInfo, Run},
    schedule::GameSet,
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
        app.add_message::<RoomCleared>()
            .add_systems(OnExit(AppState::InGame), leave_room)
            .add_systems(
                FixedUpdate,
                (unlock_when_cleared, open_key_doors).in_set(GameSet::Cleanup),
            )
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
    /// Türen, hinter denen ein verschlossener Raum liegt (Schlüssel nötig).
    pub key_locked: Vec<Direction>,
    /// Zählt Änderungen am Layout (z. B. gesprengte Felsen), damit abgeleitete
    /// Daten wie das Flowfield wissen, dass sie neu berechnet werden müssen.
    pub revision: u32,
}

impl CurrentRoom {
    /// Raum `pos` betreten: Layout (inkl. gesprengter Felsen) und Schlüsseltüren
    /// aus dem Run übernehmen.
    pub fn enter(run: &Run, pos: GridPos) -> Self {
        let layout = run
            .layout_overrides
            .get(&pos)
            .cloned()
            .or_else(|| run.floor.room_layout(pos))
            .expect("Raum existiert auf der Etage");
        let key_locked = run
            .floor
            .doors(pos)
            .filter(|&dir| {
                let neighbor = pos.neighbor(dir);
                let needs_key = run
                    .floor
                    .get(neighbor)
                    .is_some_and(|r| requires_key(r.kind, run.floor.depth));
                needs_key && !run.unlocked.contains(&neighbor)
            })
            .collect();
        Self {
            pos,
            layout,
            locked: false,
            key_locked,
            revision: 0,
        }
    }

    /// Ist diese Kachel eine Tür, die einen Schlüssel braucht?
    pub fn is_key_locked(&self, pos: GridPos) -> bool {
        door_direction(pos).is_some_and(|d| self.key_locked.contains(&d))
    }

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

    /// Freie Linie zwischen zwei Weltpunkten für diese Art von Körper?
    pub fn line_of_sight(&self, from: Vec2, to: Vec2, kind: BodyKind) -> bool {
        dungeon_gen::pathing::line_of_sight(
            self.world_to_tile_space(from).to_array(),
            self.world_to_tile_space(to).to_array(),
            |p| self.blocks(p, kind),
        )
    }

    /// Blockiert die Kachel diese Art von Körper? Verschlossene Türen
    /// sind für Läufer wie Wände.
    pub fn blocks(&self, pos: GridPos, kind: BodyKind) -> bool {
        match kind {
            BodyKind::Walker => {
                let is_door = self.layout.get(pos) == Some(Tile::Door);
                self.layout.blocks_movement(pos)
                    || (is_door && (self.locked || self.is_key_locked(pos)))
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
    pub tile: GridPos,
}

/// Markiert Tür-Kacheln. Die Richtung braucht es nicht: `door_image`
/// erkennt Schlüsseltüren über die Kachelposition (`RoomTile::tile`).
#[derive(Component)]
pub struct DoorTile;

/// Gehört zum aktuellen Raum und verschwindet beim Raumwechsel
/// (Projektile, Gegner). Kacheln werden separat über `RoomTile` verwaltet.
#[derive(Component)]
pub struct RoomScoped;

/// Hilfsfunktion statt System: wird beim Start und bei jedem Raumwechsel
/// aufgerufen. `&mut Commands` reicht, weil `Commands` Befehle nur sammelt.
pub fn spawn_room_tiles(commands: &mut Commands, room: &CurrentRoom, assets: &GameAssets) {
    let art = &assets.tiles;
    for (pos, tile) in room.layout.iter() {
        let image = match tile {
            Tile::Floor => art.floor_at(room.pos, pos),
            Tile::Wall => art.wall.clone(),
            Tile::Rock => art.rock.clone(),
            Tile::Pit => art.pit.clone(),
            Tile::Door => door_image(room, pos, assets),
        };
        let mut entity = commands.spawn((
            RoomTile {
                room: room.pos,
                tile: pos,
            },
            DespawnOnExit(AppState::InGame),
            Sprite {
                image,
                custom_size: Some(Vec2::splat(TILE_SIZE)),
                ..default()
            },
            // z = 0: Kacheln liegen unter allem anderen.
            Transform::from_translation(room.tile_center(pos).extend(0.0)),
        ));
        if tile == Tile::Door {
            entity.insert(DoorTile);
        }
    }
}

/// Erster Raum eines Runs. Läuft in der Kette aus `run.rs` nach `start_run`.
/// Erster Raum eines Runs – der Startraum oder beim Fortsetzen der gespeicherte Raum.
/// Läuft in der Kette aus `run.rs` nach `start_run`.
pub fn enter_first_room(
    mut commands: Commands,
    mut run: ResMut<Run>,
    assets: Res<GameAssets>,
    db: Res<ItemDatabase>,
    resume: Option<Res<ResumeInfo>>,
) {
    let pos = resume.map_or(run.floor.start(), |r| r.room);
    let mut room = CurrentRoom::enter(&run, pos);
    run.visited.insert(pos);
    if pos == run.floor.start() {
        run.cleared.insert(pos);
    }
    spawn_room_tiles(&mut commands, &room, &assets);

    // Beim Fortsetzen kann der Raum Gegner, Beute oder die Falltür enthalten.
    if !run.cleared.contains(&pos) {
        if spawn_room_enemies(&mut commands, &run, &room, &assets) > 0 {
            room.locked = true;
        } else {
            run.cleared.insert(pos);
        }
    }
    inventory::prepare_room_loot(&mut run, &room, &db.0);
    inventory::spawn_room_loot(&mut commands, &run, &room, &assets);
    progress::spawn_trapdoor(&mut commands, &run, &room, &assets);

    commands.insert_resource(room);
}

fn leave_room(mut commands: Commands) {
    commands.remove_resource::<CurrentRoom>();
}

/// Ein Raum mit Gegnern wurde gerade geräumt (für Belohnungen).
#[derive(Message, Debug, Clone, Copy)]
pub struct RoomCleared {
    pub room: GridPos,
}

/// Kein Gegner mehr da? Türen öffnen und Raum als geräumt merken.
pub fn unlock_when_cleared(
    mut room: ResMut<CurrentRoom>,
    mut run: ResMut<Run>,
    enemies: Query<(), With<Enemy>>,
    mut cleared: MessageWriter<RoomCleared>,
) {
    // Nur lesen löst keine Change Detection aus – erst das Schreiben unten.
    if room.locked && enemies.is_empty() {
        room.locked = false;
        run.cleared.insert(room.pos);
        cleared.write(RoomCleared { room: room.pos });
        info!("Raum {:?} geräumt – Türen offen", room.pos);
    }
}

/// Bild einer Tür: zu (Gegner), Schlüsseltür oder offen.
fn door_image(room: &CurrentRoom, pos: GridPos, assets: &GameAssets) -> Handle<Image> {
    let art = &assets.tiles;
    if room.locked {
        art.door_closed.clone()
    } else if room.is_key_locked(pos) {
        art.door_keyed.clone()
    } else {
        art.door_open.clone()
    }
}

fn update_door_visuals(
    room: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    mut doors: Query<(&RoomTile, &mut Sprite), With<DoorTile>>,
) {
    for (tile, mut sprite) in &mut doors {
        if tile.room == room.pos {
            sprite.image = door_image(&room, tile.tile, &assets);
        }
    }
}

/// Steht der Spieler an einer Schlüsseltür, wird sie mit einem Schlüssel geöffnet.
/// Ohne Schlüssel erscheint einmal ein Hinweis (nicht jeden Tick erneut).
fn open_key_doors(
    mut room: ResMut<CurrentRoom>,
    mut run: ResMut<Run>,
    mut toast: ResMut<Toast>,
    player: Single<&Position, With<Player>>,
    mut hinted: Local<bool>,
) {
    if room.key_locked.is_empty() {
        return;
    }
    // Spielermitte liegt an der Tür an, wenn sie weniger als ~1 Kachel entfernt ist.
    let touching = room.key_locked.iter().copied().find(|&dir| {
        let door = room.tile_center(door_pos(dir));
        player.0.distance(door) < 0.95 * TILE_SIZE
    });
    let Some(dir) = touching else {
        *hinted = false;
        return;
    };
    if run.inventory.keys == 0 {
        if !*hinted {
            toast.show("Verschlossen – du brauchst einen Schlüssel".to_string());
            *hinted = true;
        }
        return;
    }
    run.inventory.keys -= 1;
    let neighbor = room.pos.neighbor(dir);
    run.unlocked.insert(neighbor);
    room.key_locked.retain(|&d| d != dir);
    toast.show("Tür aufgeschlossen".to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room_at(pos: GridPos) -> CurrentRoom {
        CurrentRoom {
            pos,
            layout: RoomLayout::empty(),
            locked: false,
            key_locked: Vec::new(),
            revision: 0,
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
    fn key_locked_doors_block_until_unlocked() {
        use dungeon_gen::{Direction, room::door_pos};
        let mut room = room_at(START_POS);
        room.layout = RoomLayout::empty().with_doors([Direction::East, Direction::West]);
        room.key_locked = vec![Direction::East];
        assert!(room.blocks(door_pos(Direction::East), BodyKind::Walker));
        assert!(!room.blocks(door_pos(Direction::West), BodyKind::Walker));
        room.key_locked.clear();
        assert!(!room.blocks(door_pos(Direction::East), BodyKind::Walker));
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
