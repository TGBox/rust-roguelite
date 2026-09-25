//! Raumwechsel.
//!
//! 1. `Playing`: Steht der Spieler mit seiner Mitte auf einer offenen Tür,
//!    merken wir uns die Richtung und wechseln nach `RoomTransition`.
//! 2. `OnEnter(RoomTransition)`: Neuen Raum spawnen, `CurrentRoom` umstellen,
//!    Spieler an den gegenüberliegenden Eingang setzen.
//! 3. `RoomTransition` (jedes Bild): Kamera schwenkt weich zum neuen Raum.
//!    Am Ende: alten Raum entfernen, Gegner spawnen, Türen ggf. schließen.

use bevy::prelude::*;
use dungeon_gen::{
    Direction, Tile,
    room::{door_direction, inside_door},
};

use crate::{
    assets::GameAssets,
    camera::MainCamera,
    enemy::spawn_room_enemies,
    physics::{Position, PreviousPosition, Velocity},
    player::Player,
    room::{CurrentRoom, RoomScoped, RoomTile, spawn_room_tiles},
    run::Run,
    schedule::GameSet,
    states::InGameState,
};

/// Dauer des Kameraschwenks (Echtzeit – die Spielzeit steht still).
const PAN_SECS: f32 = 0.35;

pub struct TransitionPlugin;

impl Plugin for TransitionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, detect_door_exit.in_set(GameSet::Cleanup))
            .add_systems(OnEnter(InGameState::RoomTransition), begin_transition)
            .add_systems(
                Update,
                animate_transition.run_if(in_state(InGameState::RoomTransition)),
            );
    }
}

/// In welche Richtung der Spieler den Raum verlässt.
#[derive(Resource)]
struct PendingExit(Direction);

#[derive(Resource)]
struct Pan {
    from: Vec2,
    to: Vec2,
    elapsed: f32,
}

fn detect_door_exit(
    mut commands: Commands,
    room: Res<CurrentRoom>,
    run: Res<Run>,
    // `Single`: genau ein Spieler. Gibt es keinen, läuft das System einfach nicht.
    player: Single<&Position, With<Player>>,
    mut next: ResMut<NextState<InGameState>>,
) {
    if room.locked {
        return;
    }
    let tile = room.tile_at(player.0);
    if room.layout.get(tile) != Some(Tile::Door) {
        return;
    }
    let Some(dir) = door_direction(tile) else {
        return;
    };
    if run.floor.get(room.pos.neighbor(dir)).is_none() {
        // Sollte dank der Tests in `dungeon_gen` nie passieren.
        warn!("Tür {dir:?} in Raum {:?} führt ins Nichts", room.pos);
        return;
    }
    commands.insert_resource(PendingExit(dir));
    next.set(InGameState::RoomTransition);
}

fn begin_transition(
    mut commands: Commands,
    exit: Res<PendingExit>,
    mut room: ResMut<CurrentRoom>,
    mut run: ResMut<Run>,
    assets: Res<GameAssets>,
    player: Single<(&mut Position, &mut PreviousPosition, &mut Velocity), With<Player>>,
    leftovers: Query<Entity, With<RoomScoped>>,
    camera: Single<&Transform, With<MainCamera>>,
) {
    let dir = exit.0;
    let new_pos = room.pos.neighbor(dir);
    let layout = run
        .floor
        .room_layout(new_pos)
        .expect("Nachbarraum existiert (geprüft in detect_door_exit)");

    // Türen bleiben offen, bis der Schwenk vorbei ist und Gegner da sind.
    *room = CurrentRoom {
        pos: new_pos,
        layout,
        locked: false,
    };
    run.visited.insert(new_pos);
    spawn_room_tiles(&mut commands, &room, &assets);

    // Projektile (und evtl. Reste) aus dem alten Raum entfernen.
    for entity in &leftovers {
        commands.entity(entity).despawn();
    }

    // Spieler an den Eingang gegenüber der benutzten Tür setzen.
    // Beide Positionen gleich setzen, sonst würde er durchs Bild „fliegen“.
    let entry = room.tile_center(inside_door(dir.opposite()));
    let (mut pos, mut prev, mut vel) = player.into_inner();
    pos.0 = entry;
    prev.0 = entry;
    vel.0 = Vec2::ZERO;

    commands.insert_resource(Pan {
        from: camera.translation.truncate(),
        to: room.origin(),
        elapsed: 0.0,
    });
    commands.remove_resource::<PendingExit>();
}

fn animate_transition(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut pan: ResMut<Pan>,
    mut camera: Single<&mut Transform, With<MainCamera>>,
    mut room: ResMut<CurrentRoom>,
    mut run: ResMut<Run>,
    assets: Res<GameAssets>,
    old_tiles: Query<(Entity, &RoomTile)>,
    mut next: ResMut<NextState<InGameState>>,
) {
    pan.elapsed += time.delta_secs();
    let t = (pan.elapsed / PAN_SECS).min(1.0);
    // Smoothstep: sanft anfahren und abbremsen.
    let eased = t * t * (3.0 - 2.0 * t);
    let p = pan.from.lerp(pan.to, eased);
    camera.translation.x = p.x;
    camera.translation.y = p.y;

    if t < 1.0 {
        return;
    }

    for (entity, tile) in &old_tiles {
        if tile.room != room.pos {
            commands.entity(entity).despawn();
        }
    }
    if !run.cleared.contains(&room.pos) {
        let count = spawn_room_enemies(&mut commands, &run, &room, &assets);
        if count > 0 {
            room.locked = true;
        } else {
            // Räume ohne Gegner (Schatz, Shop) gelten sofort als geräumt.
            run.cleared.insert(room.pos);
        }
    }
    commands.remove_resource::<Pan>();
    next.set(InGameState::Playing);
}
