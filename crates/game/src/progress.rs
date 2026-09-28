//! Fortschritt innerhalb eines Runs: Kills zählen, Falltür nach dem Boss,
//! Abstieg zur nächsten Etage und Sieg auf der letzten Etage.

use bevy::prelude::*;
use dungeon_gen::{GridPos, RoomKind, floor, meta::MAX_DEPTH, room::CENTER};

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    camera::MainCamera,
    combat::EnemyKilled,
    enemy::PlayerFlow,
    inventory::{Toast, reward_room_clear},
    physics::{Position, PreviousPosition, Velocity},
    pixel_art,
    player::Player,
    room::{CurrentRoom, RoomCleared, RoomScoped, RoomTile, spawn_room_tiles},
    run::{Run, RunEnd},
    save::Autosave,
    schedule::GameSet,
    states::AppState,
};

/// Wie nah die Spielermitte an der Falltür sein muss (Kacheln).
const TRAPDOOR_REACH_TILES: f32 = 0.45;

pub struct ProgressPlugin;

impl Plugin for ProgressPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Descend>().add_systems(
            FixedUpdate,
            (
                count_kills,
                // Nach der Boss-Belohnung, damit die Falltür nicht auf dem Item landet.
                place_trapdoor.after(reward_room_clear),
                (use_trapdoor, descend_floor).chain(),
            )
                .in_set(GameSet::Cleanup),
        );
    }
}

/// Zur nächsten Etage wechseln (Falltür oder Debug-Taste F5).
#[derive(Message, Debug, Clone, Copy)]
pub struct Descend;

#[derive(Component)]
struct Trapdoor;

fn count_kills(mut killed: MessageReader<EnemyKilled>, mut run: ResMut<Run>) {
    let n = killed.read().count() as u32;
    if n > 0 {
        run.stats.kills += n;
    }
}

pub fn place_trapdoor(
    mut commands: Commands,
    mut cleared: MessageReader<RoomCleared>,
    mut run: ResMut<Run>,
    room: Res<CurrentRoom>,
    assets: Res<GameAssets>,
) {
    for c in cleared.read() {
        let is_boss = run.floor.get(c.room).map(|r| r.kind) == Some(RoomKind::Boss);
        if c.room != room.pos || !is_boss {
            continue;
        }
        run.stats.bosses += 1;
        // Nicht dorthin, wo schon Beute liegt (das Boss-Item).
        let occupied: Vec<GridPos> = run
            .loot
            .get(&room.pos)
            .map(|v| v.iter().map(|(t, _)| *t).collect())
            .unwrap_or_default();
        if let Some(tile) = room
            .layout
            .nearest_floor(CENTER + GridPos::new(0, -2), &occupied)
        {
            run.trapdoor = Some((room.pos, tile));
            spawn_trapdoor(&mut commands, &run, &room, &assets);
        }
    }
}

/// Spawnt die Falltür, falls sie in diesem Raum liegt (auch beim Zurückkommen).
pub fn spawn_trapdoor(commands: &mut Commands, run: &Run, room: &CurrentRoom, assets: &GameAssets) {
    let Some((trap_room, tile)) = run.trapdoor else {
        return;
    };
    if trap_room != room.pos {
        return;
    }
    // Auf der letzten Etage ist es ein goldener Ausgang statt einer Falltür.
    let is_exit = run.floor.depth >= MAX_DEPTH;
    let image = if is_exit {
        &assets.sprites.exit
    } else {
        &assets.sprites.trapdoor
    };
    commands.spawn((
        Name::new(if is_exit { "Exit" } else { "Trapdoor" }),
        Trapdoor,
        RoomScoped,
        DespawnOnExit(AppState::InGame),
        Sprite {
            image: image.clone(),
            custom_size: Some(pixel_art::display_size(pixel_art::TRAPDOOR)),
            ..default()
        },
        // z = 1: direkt über dem Boden, unter der Beute.
        Transform::from_translation(room.tile_center(tile).extend(1.0)),
    ));
}

fn use_trapdoor(
    player: Single<&Position, With<Player>>,
    trapdoors: Query<&Transform, With<Trapdoor>>,
    mut descend: MessageWriter<Descend>,
) {
    let reach = TRAPDOOR_REACH_TILES * TILE_SIZE;
    if trapdoors
        .iter()
        .any(|t| t.translation.truncate().distance(player.0) < reach)
    {
        descend.write(Descend);
    }
}

fn descend_floor(
    mut commands: Commands,
    mut descend: MessageReader<Descend>,
    mut run: ResMut<Run>,
    assets: Res<GameAssets>,
    room_tiles: Query<Entity, With<RoomTile>>,
    scoped: Query<Entity, With<RoomScoped>>,
    player: Single<(&mut Position, &mut PreviousPosition, &mut Velocity), With<Player>>,
    mut camera: Single<&mut Transform, With<MainCamera>>,
    mut toast: ResMut<Toast>,
    mut next: ResMut<NextState<AppState>>,
    mut autosave: MessageWriter<Autosave>,
) {
    // Mehrere Messages im selben Tick zählen nur einmal.
    if descend.read().count() == 0 {
        return;
    }
    let run = &mut *run;

    if run.floor.depth >= MAX_DEPTH {
        commands.insert_resource(RunEnd::Victory);
        next.set(AppState::RunSummary);
        return;
    }

    // --- Neue Etage erzeugen, alles Raum-bezogene zurücksetzen ---
    let depth = run.floor.depth + 1;
    run.floor = floor::generate(run.seed, depth);
    run.visited.clear();
    run.cleared.clear();
    run.unlocked.clear();
    run.layout_overrides.clear();
    run.loot.clear();
    run.loot_prepared.clear();
    run.trapdoor = None;
    info!(
        "Etage {depth} – {} Räume\n{}",
        run.floor.len(),
        run.floor.to_ascii()
    );

    for entity in room_tiles.iter().chain(scoped.iter()) {
        commands.entity(entity).try_despawn();
    }

    let start = run.floor.start();
    let room = CurrentRoom::enter(run, start);
    run.visited.insert(start);
    run.cleared.insert(start);
    spawn_room_tiles(&mut commands, &room, &assets);

    // Spieler in die Mitte des neuen Startraums, Kamera hinterher.
    let center = room.tile_center(CENTER);
    let (mut pos, mut prev, mut vel) = player.into_inner();
    pos.0 = center;
    prev.0 = center;
    vel.0 = Vec2::ZERO;
    let origin = room.origin();
    camera.translation.x = origin.x;
    camera.translation.y = origin.y;

    commands.insert_resource(room);
    // Das Flowfield gehört zur alten Etage (gleiche Raumposition, anderes Layout).
    commands.remove_resource::<PlayerFlow>();
    toast.show(format!("Etage {depth} von {MAX_DEPTH}"));
    autosave.write(Autosave);
}
