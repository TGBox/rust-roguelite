//! Spieler: Eingabe lesen, bewegen, schießen.
//!
//! Steuerung wie in *Isaac*: WASD bewegt, Pfeiltasten schießen in vier
//! Richtungen. Hält man mehrere Pfeiltasten, gewinnt die zuletzt gedrückte.

use bevy::prelude::*;
use dungeon_gen::GridPos;

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    physics::{Body, BodyKind, Position, Velocity, physics_body},
    projectile::tear_bundle,
    room::tile_center,
    schedule::GameSet,
};

/// Radius der Grafik (in Kacheln).
pub const PLAYER_RADIUS_TILES: f32 = 0.4;
/// Hitbox bewusst kleiner als die Grafik: fühlt sich fairer an.
const PLAYER_HALF_TILES: f32 = 0.3;
/// Anteil der Spielergeschwindigkeit, den ein Schuss mitnimmt.
const SHOT_INHERIT_VELOCITY: f32 = 0.3;

const SHOOT_KEYS: [(KeyCode, Vec2); 4] = [
    (KeyCode::ArrowUp, Vec2::Y),
    (KeyCode::ArrowDown, Vec2::NEG_Y),
    (KeyCode::ArrowLeft, Vec2::NEG_X),
    (KeyCode::ArrowRight, Vec2::X),
];

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInput>()
            .add_systems(Startup, spawn_player)
            // Eingabe einmal pro Bild lesen, direkt BEVOR die festen Ticks laufen.
            .add_systems(
                RunFixedMainLoop,
                read_input.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
            )
            .add_systems(
                FixedUpdate,
                (player_movement, player_shoot)
                    .chain()
                    .in_set(GameSet::Control),
            );
    }
}

#[derive(Component)]
pub struct Player;

/// Werte, die später Items verändern (M6). Einheiten in Kacheln bzw. Sekunden.
#[derive(Component, Debug, Clone)]
pub struct PlayerStats {
    /// Höchstgeschwindigkeit (Kacheln/s).
    pub move_speed: f32,
    /// Wie schnell die Höchstgeschwindigkeit erreicht wird (1/s). Höher = direkter.
    pub acceleration: f32,
    /// Pause zwischen zwei Schüssen (s).
    pub fire_delay: f32,
    /// Schussgeschwindigkeit (Kacheln/s).
    pub shot_speed: f32,
    /// Reichweite (Kacheln).
    pub range: f32,
}

impl Default for PlayerStats {
    fn default() -> Self {
        Self {
            move_speed: 5.0,
            acceleration: 14.0,
            fire_delay: 0.35,
            shot_speed: 9.0,
            range: 6.5,
        }
    }
}

/// Verbleibende Zeit bis zum nächsten möglichen Schuss (s).
#[derive(Component, Debug, Default)]
pub struct ShootCooldown(pub f32);

/// Aufbereitete Eingabe. Entkoppelt Tastatur von Logik:
/// Gamepad-Support in M9 ändert nur `read_input`, nichts sonst.
#[derive(Resource, Debug, Default)]
pub struct PlayerInput {
    /// Normalisiert oder null.
    pub move_dir: Vec2,
    pub shoot_dir: Option<Vec2>,
}

fn spawn_player(mut commands: Commands, assets: Res<GameAssets>) {
    let start = tile_center(GridPos::new(7, 4));
    commands.spawn((
        Name::new("Player"),
        Player,
        PlayerStats::default(),
        ShootCooldown::default(),
        physics_body(
            start,
            Vec2::ZERO,
            Body {
                half_size: Vec2::splat(PLAYER_HALF_TILES * TILE_SIZE),
                kind: BodyKind::Walker,
            },
            10.0,
        ),
        Mesh2d(assets.player_mesh.clone()),
        MeshMaterial2d(assets.player_material.clone()),
    ));
}

/// `Local<T>` ist Zustand, der nur diesem System gehört und zwischen den
/// Aufrufen erhalten bleibt – hier der Stapel gedrückter Pfeiltasten.
fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut input: ResMut<PlayerInput>,
    mut shoot_stack: Local<Vec<KeyCode>>,
) {
    let mut dir = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        dir.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        dir.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        dir.x += 1.0;
    }
    // Diagonal nicht schneller als gerade.
    input.move_dir = dir.normalize_or_zero();

    for (key, _) in SHOOT_KEYS {
        if keys.just_pressed(key) {
            shoot_stack.push(key);
        }
    }
    shoot_stack.retain(|k| keys.pressed(*k));
    // Fallback, falls ein Tastendruck verpasst wurde (z. B. Fensterfokus).
    if shoot_stack.is_empty()
        && let Some((key, _)) = SHOOT_KEYS.iter().find(|(k, _)| keys.pressed(*k))
    {
        shoot_stack.push(*key);
    }

    input.shoot_dir = shoot_stack
        .last()
        .and_then(|last| SHOOT_KEYS.iter().find(|(k, _)| k == last))
        .map(|(_, d)| *d);
}

fn player_movement(
    time: Res<Time>,
    input: Res<PlayerInput>,
    mut query: Query<(&PlayerStats, &mut Velocity), With<Player>>,
) {
    let dt = time.delta_secs();
    for (stats, mut vel) in &mut query {
        let target = input.move_dir * stats.move_speed * TILE_SIZE;
        // Exponentielle Annäherung: framerate-unabhängig und ohne Überschwingen.
        // Loslassen = Ziel null = sanftes Abbremsen.
        let t = 1.0 - (-stats.acceleration * dt).exp();
        vel.0 = vel.0.lerp(target, t);
    }
}

fn player_shoot(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    assets: Res<GameAssets>,
    mut query: Query<(&Position, &Velocity, &PlayerStats, &mut ShootCooldown), With<Player>>,
) {
    let dt = time.delta_secs();
    for (pos, vel, stats, mut cooldown) in &mut query {
        cooldown.0 = (cooldown.0 - dt).max(0.0);

        // let-else: früh aussteigen, wenn nicht geschossen wird.
        let Some(dir) = input.shoot_dir else {
            continue;
        };
        if cooldown.0 > 0.0 {
            continue;
        }
        cooldown.0 = stats.fire_delay;

        let speed = stats.shot_speed * TILE_SIZE;
        let velocity = dir * speed + vel.0 * SHOT_INHERIT_VELOCITY;
        let lifetime = stats.range * TILE_SIZE / speed;
        commands.spawn(tear_bundle(pos.0, velocity, lifetime, &assets));
    }
}
