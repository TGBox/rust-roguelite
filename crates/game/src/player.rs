//! Spieler: Eingabe lesen, bewegen, schießen.
//!
//! Steuerung wie in *Isaac*: WASD bewegt, Pfeiltasten schießen in vier
//! Richtungen. Hält man mehrere Pfeiltasten, gewinnt die zuletzt gedrückte.

use bevy::prelude::*;
use dungeon_gen::{
    items::{ShotPattern, compute_stats, shot_pattern},
    room::CENTER,
    stats::Stats,
};

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    audio::{Effect, Sfx},
    combat::{Faction, FlashArt, Health, Invulnerable},
    item_db::ItemDatabase,
    juice::Wobble,
    physics::{Body, BodyKind, Position, Velocity, physics_body},
    projectile::{Shot, shot_bundle},
    room::CurrentRoom,
    run::{ResumeInfo, Run},
    schedule::GameSet,
    states::{AppState, InGameState},
};

/// Hitbox bewusst kleiner als die Grafik: fühlt sich fairer an.
const PLAYER_HALF_TILES: f32 = 0.3;
/// Leben in halben Herzen.
pub const PLAYER_MAX_HEALTH: f32 = 6.0;
/// Wie schnell die Höchstgeschwindigkeit erreicht wird (1/s). Höher = direkter.
const ACCELERATION: f32 = 14.0;
/// Winkel zwischen zwei Schüssen einer Salve (Bogenmaß, ≈ 10°).
const SPREAD: f32 = 0.18;
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
            .add_systems(OnEnter(InGameState::Dying), show_dead_player)
            // Eingabe einmal pro Bild lesen, direkt BEVOR die festen Ticks laufen.
            .add_systems(
                RunFixedMainLoop,
                read_input
                    .in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop)
                    .run_if(in_state(InGameState::Playing)),
            )
            .add_systems(
                FixedUpdate,
                (refresh_player_stats, player_movement, player_shoot)
                    .chain()
                    .in_set(GameSet::Control),
            );
    }
}

#[derive(Component)]
pub struct Player;

/// Aktuelle Werte des Spielers – aus Grundwerten und Items berechnet
/// (siehe `dungeon_gen::items::compute_stats`). Nie direkt verändern.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct PlayerStats {
    pub stats: Stats,
    pub pattern: ShotPattern,
}

impl Default for PlayerStats {
    fn default() -> Self {
        Self {
            stats: Stats::BASE,
            pattern: shot_pattern(&[]),
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
    /// Bombe legen (Taste E). Bleibt `true`, bis ein fester Tick sie verbraucht:
    /// Ein kurzer Tastendruck kann in ein Bild ohne festen Tick fallen – ohne
    /// dieses „Einrasten“ ginge er verloren.
    pub place_bomb: bool,
}

/// Läuft in der Kette aus `run.rs`, nachdem der erste Raum existiert.
pub fn spawn_player(
    mut commands: Commands,
    assets: Res<GameAssets>,
    room: Res<CurrentRoom>,
    resume: Option<Res<ResumeInfo>>,
) {
    let start = room.tile_center(CENTER);
    let art = &assets.actors.player;
    // Beim Fortsetzen: gespeichertes Leben statt voller Herzen.
    let health = resume.map_or(Health::full(PLAYER_MAX_HEALTH), |r| Health {
        current: r.health,
        max: r.max_health,
    });
    commands.spawn((
        Name::new("Player"),
        DespawnOnExit(AppState::InGame),
        Player,
        Faction::Player,
        // Lebenspunkte in halben Herzen: 6 = drei volle Herzen.
        health,
        Invulnerable::default(),
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
        Sprite {
            image: art.image.clone(),
            custom_size: Some(art.size),
            ..default()
        },
        FlashArt::from(art),
        Wobble::new(art.size, start),
    ));
}

/// Berechnet die Werte jeden Tick neu. Das ist billig und immer korrekt –
/// auch für Effekte, die von Münzen oder fehlendem Leben abhängen.
/// `set_if_neq` löst Change Detection nur bei echten Änderungen aus.
fn refresh_player_stats(
    run: Res<Run>,
    db: Res<ItemDatabase>,
    mut query: Query<(&mut PlayerStats, &Health), With<Player>>,
) {
    let inv = &run.inventory;
    let items = db.0.resolve(&inv.items);
    for (mut stats, health) in &mut query {
        let missing = (health.max - health.current).max(0.0) as u32;
        stats.set_if_neq(PlayerStats {
            stats: compute_stats(&items, inv.coins, missing),
            pattern: shot_pattern(&items),
        });
    }
}

/// Der Spieler kippt um: auf den Kopf gestellt und dunkelrot eingefärbt.
/// Die Wackel-Animation stoppt von selbst, weil die virtuelle Zeit steht.
fn show_dead_player(mut query: Query<(&mut Sprite, &FlashArt, &mut Visibility), With<Player>>) {
    for (mut sprite, art, mut visibility) in &mut query {
        // Falls der Tod mitten im Aufblitzen passiert.
        sprite.image = art.normal.clone();
        sprite.flip_y = true;
        sprite.color = Color::srgb(0.75, 0.3, 0.3);
        // Falls der Tod mitten im Blinken passiert.
        *visibility = Visibility::Inherited;
    }
}

/// `Local<T>` ist Zustand, der nur diesem System gehört und zwischen den
/// Aufrufen erhalten bleibt – hier der Stapel gedrückter Pfeiltasten.
fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut input: ResMut<PlayerInput>,
    mut shoot_stack: Local<Vec<KeyCode>>,
) {
    if keys.just_pressed(KeyCode::KeyE) {
        input.place_bomb = true;
    }

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
        let target = input.move_dir * stats.stats.move_speed * TILE_SIZE;
        // Exponentielle Annäherung: framerate-unabhängig und ohne Überschwingen.
        // Loslassen = Ziel null = sanftes Abbremsen.
        let t = 1.0 - (-ACCELERATION * dt).exp();
        vel.0 = vel.0.lerp(target, t);
    }
}

fn player_shoot(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    assets: Res<GameAssets>,
    mut query: Query<(&Position, &Velocity, &PlayerStats, &mut ShootCooldown), With<Player>>,
    mut sfx: MessageWriter<Sfx>,
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
        let PlayerStats { stats, pattern } = stats;
        cooldown.0 = stats.fire_delay();
        // Ein Sound pro Salve, nicht pro Schuss.
        sfx.write(Sfx(Effect::Shoot));

        let speed = stats.shot_speed * TILE_SIZE;
        let lifetime = stats.range * TILE_SIZE / speed;
        // Salve gleichmäßig um die Schussrichtung fächern: bei 3 Schüssen
        // Winkel -1, 0, +1 mal SPREAD.
        let n = pattern.count;
        for i in 0..n {
            let offset = (i as f32 - (n - 1) as f32 / 2.0) * SPREAD;
            let shot_dir = Vec2::from_angle(offset).rotate(dir);
            commands.spawn(shot_bundle(
                Shot {
                    position: pos.0,
                    velocity: shot_dir * speed + vel.0 * SHOT_INHERIT_VELOCITY,
                    lifetime,
                    damage: stats.damage,
                    faction: Faction::Player,
                    piercing: pattern.piercing,
                    homing: pattern.homing,
                },
                &assets,
            ));
        }
    }
}
