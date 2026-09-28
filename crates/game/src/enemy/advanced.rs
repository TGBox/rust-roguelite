//! Fortgeschrittene Gegner: Springer, Teiler (beim Tod) und Beschwörer.
//!
//! Als Kindmodul von `enemy` sieht diese Datei auch dessen private Helfer
//! (`steer`, `chase_direction`, `enemy_shot`, …) – ohne sie `pub` zu machen.

use bevy::prelude::*;
use dungeon_gen::EnemyKind;

use super::{
    Asleep, Enemy, EnemySpawn, Mobility, PlayerFlow, chase_direction, enemy_shot, spawn_enemy,
    steer,
};
use crate::{
    TILE_SIZE,
    assets::GameAssets,
    audio::{Effect, Sfx},
    combat::EnemyKilled,
    juice::Fx,
    physics::{BodyKind, Position, Velocity},
    player::Player,
    room::CurrentRoom,
    run::Run,
};

/// Mehr Gegner lassen Beschwörer und Brutmutter nicht zu.
pub(super) const MAX_ENEMIES_FOR_SUMMON: usize = 9;

// --- Springer -----------------------------------------------------------------------

#[derive(Component, Debug)]
pub struct Hopper {
    state: HopState,
    /// Ab Etage 2 verschießt er beim Landen ein Kreuz aus vier Schüssen.
    cross_on_land: bool,
}

#[derive(Debug, Clone, Copy)]
enum HopState {
    Resting(f32),
    Jumping(f32),
}

impl Hopper {
    pub fn new(depth: u32) -> Self {
        Self {
            state: HopState::Resting(0.8),
            cross_on_land: depth >= 2,
        }
    }
}

/// Ruhen → Sprung in Richtung Spieler → Landen (ggf. Schüsse) → Ruhen.
pub fn hopper_ai(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    player: Single<&Position, With<Player>>,
    mut query: Query<(&Position, &mut Velocity, &Mobility, &mut Hopper), Without<Asleep>>,
    mut sfx: MessageWriter<Sfx>,
) {
    const REST_SECS: f32 = 0.9;
    const JUMP_SECS: f32 = 0.4;
    const JUMP_SPEED_TILES: f32 = 7.0;

    let dt = time.delta_secs();
    for (pos, mut vel, mob, mut hopper) in &mut query {
        // Erst kopieren, dann neu zuweisen (wie beim Charger).
        let current = hopper.state;
        hopper.state = match current {
            HopState::Resting(t) => {
                steer(&mut vel, Vec2::ZERO, mob.acceleration, dt);
                if t - dt > 0.0 {
                    HopState::Resting(t - dt)
                } else {
                    // Kein Pathfinding: Springer springen stur geradeaus –
                    // Felsen sind hier Deckung für den Spieler.
                    let dir = (player.0 - pos.0).normalize_or_zero();
                    vel.0 = dir * JUMP_SPEED_TILES * TILE_SIZE;
                    HopState::Jumping(JUMP_SECS)
                }
            }
            HopState::Jumping(t) => {
                if t - dt > 0.0 {
                    HopState::Jumping(t - dt)
                } else {
                    vel.0 = Vec2::ZERO;
                    if hopper.cross_on_land {
                        for dir in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
                            commands.spawn(enemy_shot(pos.0, dir, 4.5, &assets));
                        }
                        sfx.write(Sfx(Effect::EnemyShoot));
                    }
                    HopState::Resting(REST_SECS)
                }
            }
        };
    }
}

// --- Teiler ------------------------------------------------------------------------------

/// Stirbt ein Teiler, entstehen an seiner Stelle zwei Splitlinge.
///
/// Läuft vor `unlock_when_cleared`: Sonst gälte der Raum für einen Tick als
/// geräumt, bevor die Splitlinge existieren – und die Türen gingen auf.
pub fn split_on_death(
    mut commands: Commands,
    mut killed: MessageReader<EnemyKilled>,
    run: Res<Run>,
    room: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    mut fx: MessageWriter<Fx>,
) {
    for kill in killed.read() {
        if kill.kind != EnemyKind::Splitter {
            continue;
        }
        for side in [-1.0, 1.0] {
            let offset = Vec2::new(side * 0.35 * TILE_SIZE, 0.0);
            // Nicht in eine Wand setzen: sonst lieber genau auf den Todespunkt.
            let target = kill.pos + offset;
            let pos = if room.blocks(room.tile_at(target), BodyKind::Walker) {
                kill.pos
            } else {
                target
            };
            spawn_enemy(
                &mut commands,
                EnemySpawn {
                    kind: EnemyKind::Splitling,
                    pos,
                    depth: run.floor.depth,
                    champion: false,
                    asleep: 0.25,
                },
                &assets,
            );
        }
        fx.write(Fx::Burst {
            at: kill.pos,
            color: Color::srgb(0.65, 0.40, 0.85),
            count: 12,
            speed: 120.0,
        });
    }
}

// --- Beschwörer ---------------------------------------------------------------------------

#[derive(Component, Debug)]
pub struct Summoner {
    cooldown: f32,
}

impl Default for Summoner {
    fn default() -> Self {
        Self { cooldown: 2.0 }
    }
}

/// Hält Abstand wie ein Shooter und ruft regelmäßig einen Splitling herbei.
pub fn summoner_ai(
    mut commands: Commands,
    time: Res<Time>,
    room: Res<CurrentRoom>,
    run: Res<Run>,
    flow: Option<Res<PlayerFlow>>,
    assets: Res<GameAssets>,
    player: Single<&Position, With<Player>>,
    enemies: Query<(), With<Enemy>>,
    mut query: Query<(&Position, &mut Velocity, &Mobility, &mut Summoner), Without<Asleep>>,
    mut fx: MessageWriter<Fx>,
    mut sfx: MessageWriter<Sfx>,
) {
    const TOO_CLOSE: f32 = 4.0;
    const TOO_FAR: f32 = 7.0;
    const SUMMON_DELAY: f32 = 3.5;

    let dt = time.delta_secs();
    // Zählen einmal vorab; neue Splitlinge dieses Ticks mitzählen.
    let mut alive = enemies.iter().count();
    for (pos, mut vel, mob, mut summoner) in &mut query {
        let to_player = player.0 - pos.0;
        let dist = to_player.length() / TILE_SIZE;
        let dir = if dist < TOO_CLOSE {
            -to_player.normalize_or_zero()
        } else if dist > TOO_FAR {
            chase_direction(flow.as_deref(), &room, pos.0, player.0)
        } else {
            Vec2::ZERO
        };
        steer(&mut vel, dir * mob.speed * TILE_SIZE, mob.acceleration, dt);

        summoner.cooldown -= dt;
        if summoner.cooldown > 0.0 || alive >= MAX_ENEMIES_FOR_SUMMON {
            continue;
        }
        summoner.cooldown = SUMMON_DELAY;
        // Zwischen Beschwörer und Spieler erscheinen lassen.
        let spawn_pos = pos.0 + to_player.normalize_or_zero() * 0.8 * TILE_SIZE;
        let spawn_pos = if room.blocks(room.tile_at(spawn_pos), BodyKind::Walker) {
            pos.0
        } else {
            spawn_pos
        };
        spawn_enemy(
            &mut commands,
            EnemySpawn {
                kind: EnemyKind::Splitling,
                pos: spawn_pos,
                depth: run.floor.depth,
                champion: false,
                asleep: 0.4,
            },
            &assets,
        );
        alive += 1;
        fx.write(Fx::Burst {
            at: spawn_pos,
            color: Color::srgb(0.85, 0.3, 0.9),
            count: 10,
            speed: 90.0,
        });
        sfx.write(Sfx(Effect::Summon));
    }
}
