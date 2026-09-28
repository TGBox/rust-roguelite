//! Die drei Bosse. Jeder hat zwei **Phasen**: Unter 50 % Leben wird er
//! aggressiver. Die Phase wird nicht gespeichert, sondern jedes Mal aus dem
//! Leben berechnet – so kann sie nie „falsch“ sein.

use std::f32::consts::TAU;

use bevy::prelude::*;
use dungeon_gen::EnemyKind;

use super::{
    Asleep, Enemy, EnemySpawn, Mobility, PlayerFlow, advanced::MAX_ENEMIES_FOR_SUMMON,
    chase_direction, enemy_shot, spawn_enemy, steer,
};
use crate::{
    TILE_SIZE,
    assets::GameAssets,
    audio::{Effect, Sfx},
    combat::Health,
    juice::Fx,
    physics::{Position, Velocity},
    player::Player,
    room::CurrentRoom,
    run::Run,
};

/// Zweite Phase ab diesem Lebensanteil.
const PHASE_TWO_BELOW: f32 = 0.5;

fn in_phase_two(health: &Health) -> bool {
    health.current < health.max * PHASE_TWO_BELOW
}

/// Ring aus `count` Schüssen, leicht zum Spieler gedreht (ein Schuss zielt direkt).
fn ring(
    commands: &mut Commands,
    from: Vec2,
    toward: Vec2,
    count: u32,
    speed: f32,
    assets: &GameAssets,
) {
    let base = (toward - from).to_angle();
    for i in 0..count {
        let angle = base + i as f32 * TAU / count as f32;
        commands.spawn(enemy_shot(from, Vec2::from_angle(angle), speed, assets));
    }
}

// --- Klumpenkönig (Etage 1) ---------------------------------------------------------

#[derive(Component, Debug)]
pub struct KingAi {
    volley_cooldown: f32,
    /// Jede zweite Salve ist um einen halben Schusswinkel versetzt.
    offset: bool,
}

impl Default for KingAi {
    fn default() -> Self {
        Self {
            volley_cooldown: 2.0,
            offset: false,
        }
    }
}

/// Phase 1: verfolgt langsam, alle 2,8 s ein Ring aus 8 Schüssen.
/// Phase 2: schneller, alle 1,8 s ein Ring aus 12 Schüssen, abwechselnd versetzt.
pub fn king_ai(
    mut commands: Commands,
    time: Res<Time>,
    room: Res<CurrentRoom>,
    flow: Option<Res<PlayerFlow>>,
    assets: Res<GameAssets>,
    player: Single<&Position, With<Player>>,
    mut query: Query<(&Position, &mut Velocity, &Mobility, &Health, &mut KingAi), Without<Asleep>>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    for (pos, mut vel, mob, health, mut king) in &mut query {
        let phase_two = in_phase_two(health);
        let speed = mob.speed * if phase_two { 1.4 } else { 1.0 };
        let dir = chase_direction(flow.as_deref(), &room, pos.0, player.0);
        steer(&mut vel, dir * speed * TILE_SIZE, mob.acceleration, dt);

        king.volley_cooldown -= dt;
        if king.volley_cooldown > 0.0 {
            continue;
        }
        let (delay, count) = if phase_two { (1.8, 12) } else { (2.8, 8) };
        king.volley_cooldown = delay;
        king.offset = !king.offset;
        // Versatz: Zielpunkt um einen halben Schusswinkel um den Boss drehen.
        let half_step = if king.offset && phase_two {
            TAU / count as f32 / 2.0
        } else {
            0.0
        };
        let aim = pos.0 + Vec2::from_angle(half_step).rotate(player.0 - pos.0);
        ring(&mut commands, pos.0, aim, count, 4.5, &assets);
        sfx.write(Sfx(Effect::EnemyShoot));
    }
}

// --- Brutmutter (Etage 2) -----------------------------------------------------------

#[derive(Component, Debug)]
pub struct BroodAi {
    summon_cooldown: f32,
    dash: DashState,
}

#[derive(Debug, Clone, Copy)]
enum DashState {
    /// Wartet bis zum nächsten Sturmangriff (nur Phase 2).
    Idle(f32),
    /// Holt aus – der Spieler sieht es kommen.
    Windup {
        remaining: f32,
    },
    Dashing {
        remaining: f32,
        dir: Vec2,
    },
}

impl Default for BroodAi {
    fn default() -> Self {
        Self {
            summon_cooldown: 1.5,
            dash: DashState::Idle(3.0),
        }
    }
}

impl BroodAi {
    /// Für die Optik: holt gerade zum Sturm aus?
    pub fn is_winding_up(&self) -> bool {
        matches!(self.dash, DashState::Windup { .. })
    }
}

/// Phase 1: kriecht heran und ruft alle 3,2 s zwei Splitlinge.
/// Phase 2: zusätzlich Sturmangriffe in Richtung Spieler (mit Vorwarnung).
pub fn brood_ai(
    mut commands: Commands,
    time: Res<Time>,
    room: Res<CurrentRoom>,
    run: Res<Run>,
    flow: Option<Res<PlayerFlow>>,
    assets: Res<GameAssets>,
    player: Single<&Position, With<Player>>,
    enemies: Query<(), With<Enemy>>,
    mut query: Query<(&Position, &mut Velocity, &Mobility, &Health, &mut BroodAi), Without<Asleep>>,
    mut fx: MessageWriter<Fx>,
    mut sfx: MessageWriter<Sfx>,
) {
    const SUMMON_DELAY: f32 = 3.2;
    const DASH_DELAY: f32 = 3.5;
    const WINDUP_SECS: f32 = 0.6;
    const DASH_SECS: f32 = 0.5;
    const DASH_SPEED_TILES: f32 = 7.5;

    let dt = time.delta_secs();
    let mut alive = enemies.iter().count();
    for (pos, mut vel, mob, health, mut brood) in &mut query {
        let phase_two = in_phase_two(health);

        // Bewegung: Sturmangriff hat Vorrang vor dem Kriechen.
        let current = brood.dash;
        brood.dash = match current {
            DashState::Idle(t) => {
                let dir = chase_direction(flow.as_deref(), &room, pos.0, player.0);
                steer(&mut vel, dir * mob.speed * TILE_SIZE, mob.acceleration, dt);
                if phase_two && t - dt <= 0.0 {
                    DashState::Windup {
                        remaining: WINDUP_SECS,
                    }
                } else {
                    DashState::Idle(t - dt)
                }
            }
            DashState::Windup { remaining } => {
                steer(&mut vel, Vec2::ZERO, 12.0, dt);
                if remaining - dt <= 0.0 {
                    // Richtung erst jetzt festlegen: So lohnt sich Ausweichen in letzter Sekunde.
                    let dir = (player.0 - pos.0).normalize_or_zero();
                    DashState::Dashing {
                        remaining: DASH_SECS,
                        dir,
                    }
                } else {
                    DashState::Windup {
                        remaining: remaining - dt,
                    }
                }
            }
            DashState::Dashing { remaining, dir } => {
                vel.0 = dir * DASH_SPEED_TILES * TILE_SIZE;
                if remaining - dt <= 0.0 {
                    DashState::Idle(DASH_DELAY)
                } else {
                    DashState::Dashing {
                        remaining: remaining - dt,
                        dir,
                    }
                }
            }
        };

        brood.summon_cooldown -= dt;
        if brood.summon_cooldown > 0.0 || alive >= MAX_ENEMIES_FOR_SUMMON {
            continue;
        }
        brood.summon_cooldown = SUMMON_DELAY;
        for side in [-1.0, 1.0] {
            let spawn_pos = pos.0 + Vec2::new(side * TILE_SIZE, -0.5 * TILE_SIZE);
            let spawn_pos =
                if room.blocks(room.tile_at(spawn_pos), crate::physics::BodyKind::Walker) {
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
                color: Color::srgb(0.45, 0.75, 0.30),
                count: 8,
                speed: 90.0,
            });
        }
        sfx.write(Sfx(Effect::Summon));
    }
}

// --- Wächter (Etage 3) ----------------------------------------------------------------

#[derive(Component, Debug)]
pub struct WardenAi {
    spiral_cooldown: f32,
    spiral_angle: f32,
    burst_cooldown: f32,
}

impl Default for WardenAi {
    fn default() -> Self {
        Self {
            spiral_cooldown: 1.0,
            spiral_angle: 0.0,
            burst_cooldown: 2.0,
        }
    }
}

/// Phase 1: treibt langsam zur Raummitte und feuert eine rotierende Spirale.
/// Phase 2: doppelte Spirale plus gezielte Fächer aus fünf Schüssen.
pub fn warden_ai(
    mut commands: Commands,
    time: Res<Time>,
    room: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    player: Single<&Position, With<Player>>,
    mut query: Query<
        (&Position, &mut Velocity, &Mobility, &Health, &mut WardenAi),
        Without<Asleep>,
    >,
    mut sfx: MessageWriter<Sfx>,
) {
    const SPIRAL_STEP_SECS: f32 = 0.14;
    const SPIRAL_TURN: f32 = 0.45;
    const BURST_DELAY: f32 = 2.2;
    const FAN: f32 = 0.22;

    let dt = time.delta_secs();
    let center = room.origin();
    for (pos, mut vel, mob, health, mut warden) in &mut query {
        let phase_two = in_phase_two(health);
        // Mitte ansteuern; nah dran → kaum noch Bewegung.
        let to_center = center - pos.0;
        let desired = if to_center.length() > TILE_SIZE {
            to_center.normalize() * mob.speed * TILE_SIZE
        } else {
            Vec2::ZERO
        };
        steer(&mut vel, desired, mob.acceleration, dt);

        warden.spiral_cooldown -= dt;
        if warden.spiral_cooldown <= 0.0 {
            warden.spiral_cooldown = SPIRAL_STEP_SECS;
            warden.spiral_angle = (warden.spiral_angle + SPIRAL_TURN) % TAU;
            // Phase 2: zweiter Arm, genau gegenüber.
            let arms = if phase_two { 2 } else { 1 };
            for arm in 0..arms {
                let angle = warden.spiral_angle + arm as f32 * TAU / 2.0;
                commands.spawn(enemy_shot(pos.0, Vec2::from_angle(angle), 3.8, &assets));
            }
        }

        if !phase_two {
            continue;
        }
        warden.burst_cooldown -= dt;
        if warden.burst_cooldown <= 0.0 {
            warden.burst_cooldown = BURST_DELAY;
            let aim = (player.0 - pos.0).normalize_or_zero();
            for i in -2..=2 {
                let dir = Vec2::from_angle(i as f32 * FAN).rotate(aim);
                commands.spawn(enemy_shot(pos.0, dir, 6.0, &assets));
            }
            sfx.write(Sfx(Effect::EnemyShoot));
        }
    }
}
