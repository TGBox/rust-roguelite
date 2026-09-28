//! Gegner: Werte, Spawnen und KI.
//!
//! Jeder Gegnertyp hat eine eigene **Verhaltens-Komponente** (`Chaser`,
//! `Shooter`, `Charger`, `Boss`) und ein eigenes System. Ein System fragt
//! nur „seine“ Gegner ab – neue Typen kommen hinzu, ohne bestehende zu ändern.
//!
//! Alle Gegner bewegen sich über ein gemeinsames **Flowfield** zum Spieler
//! (siehe `dungeon_gen::pathing`), das nur neu berechnet wird, wenn der
//! Spieler die Kachel wechselt.

use std::f32::consts::TAU;

use bevy::prelude::*;
use dungeon_gen::{EnemyKind, GridPos, pathing::FlowField, spawns};

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    combat::{BaseMaterial, ContactDamage, Faction, Health},
    physics::{Body, BodyKind, Position, Velocity, physics_body},
    player::Player,
    projectile::{Shot, shot_bundle},
    room::{CurrentRoom, RoomScoped},
    run::Run,
    schedule::GameSet,
    states::AppState,
};

pub const ENEMY_HALF_TILES: f32 = 0.38;
pub const BOSS_HALF_TILES: f32 = 0.8;
/// Gegner „schlafen“ nach dem Betreten eines Raums kurz – Zeit zum Orientieren.
const WAKE_UP_SECS: f32 = 0.6;
/// Wie stark sich überlappende Gegner auseinanderschieben (1/s).
const SEPARATION_STRENGTH: f32 = 6.0;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                update_flow_field,
                wake_up,
                (chaser_ai, shooter_ai, charger_ai, boss_ai),
                separate_enemies,
            )
                .chain()
                .in_set(GameSet::Control),
        )
        .add_systems(OnExit(AppState::InGame), |mut commands: Commands| {
            commands.remove_resource::<PlayerFlow>();
        });
    }
}

// --- Komponenten -------------------------------------------------------------

#[derive(Component)]
pub struct Enemy;

/// Noch nicht aktiv (verbleibende Sekunden). Keine KI, kein Kontaktschaden.
#[derive(Component)]
pub struct Asleep(pub f32);

/// Bewegungswerte in Kacheln/s bzw. 1/s.
#[derive(Component, Debug, Clone, Copy)]
pub struct Mobility {
    pub speed: f32,
    pub acceleration: f32,
}

#[derive(Component)]
pub struct Chaser;

#[derive(Component)]
pub struct Shooter {
    cooldown: f32,
}

#[derive(Component)]
pub struct Charger {
    state: ChargeState,
}

/// Zustandsmaschine des Chargers. `Copy`, damit wir den alten Zustand lesen
/// und den neuen zuweisen können, ohne mit dem Borrow-Checker zu ringen.
#[derive(Debug, Clone, Copy)]
enum ChargeState {
    /// Schleicht Richtung Spieler und wartet auf eine freie Linie.
    Stalking,
    /// Kurzes Ausholen – Warnung für den Spieler.
    Windup { remaining: f32, dir: Vec2 },
    /// Sprint in fester Richtung bis zum Aufprall.
    Charging { dir: Vec2 },
    /// Nach dem Aufprall benommen.
    Stunned { remaining: f32 },
}

#[derive(Component)]
pub struct Boss {
    volley_cooldown: f32,
}

// --- Werte je Typ -------------------------------------------------------------

struct EnemyStats {
    health: f32,
    contact_damage: f32,
    half_tiles: f32,
    mobility: Mobility,
}

fn stats(kind: EnemyKind) -> EnemyStats {
    let m = |speed, acceleration| Mobility {
        speed,
        acceleration,
    };
    match kind {
        EnemyKind::Chaser => EnemyStats {
            health: 6.0,
            contact_damage: 1.0,
            half_tiles: ENEMY_HALF_TILES,
            mobility: m(2.6, 8.0),
        },
        EnemyKind::Shooter => EnemyStats {
            health: 5.0,
            contact_damage: 1.0,
            half_tiles: ENEMY_HALF_TILES,
            mobility: m(2.0, 6.0),
        },
        EnemyKind::Charger => EnemyStats {
            health: 8.0,
            contact_damage: 1.0,
            half_tiles: ENEMY_HALF_TILES,
            mobility: m(1.4, 6.0),
        },
        EnemyKind::Boss => EnemyStats {
            health: 50.0,
            contact_damage: 2.0,
            half_tiles: BOSS_HALF_TILES,
            mobility: m(1.3, 3.0),
        },
    }
}

// --- Spawnen ------------------------------------------------------------------

/// Spawnt die Gegner für den aktuellen Raum und gibt ihre Anzahl zurück.
/// Typen und Positionen kommen deterministisch aus `dungeon_gen::spawns`.
pub fn spawn_room_enemies(
    commands: &mut Commands,
    run: &Run,
    room: &CurrentRoom,
    assets: &GameAssets,
) -> usize {
    let Some(info) = run.floor.get(room.pos) else {
        return 0;
    };
    let mut rng = spawns::spawn_rng(run.seed, run.floor.depth, room.pos);
    let plan = spawns::plan_spawns(info.kind, &room.layout, &mut rng);

    for spawn in &plan {
        let s = stats(spawn.kind);
        let (mesh, material) = match spawn.kind {
            EnemyKind::Chaser => (&assets.enemy_mesh, &assets.chaser_material),
            EnemyKind::Shooter => (&assets.enemy_mesh, &assets.shooter_material),
            EnemyKind::Charger => (&assets.enemy_mesh, &assets.charger_material),
            EnemyKind::Boss => (&assets.boss_mesh, &assets.boss_material),
        };
        let mut entity = commands.spawn((
            Name::new(format!("{:?}", spawn.kind)),
            Enemy,
            Faction::Enemy,
            Health::full(s.health),
            ContactDamage(s.contact_damage),
            s.mobility,
            Asleep(WAKE_UP_SECS),
            RoomScoped,
            DespawnOnExit(AppState::InGame),
            physics_body(
                room.tile_center(spawn.pos),
                Vec2::ZERO,
                Body {
                    half_size: Vec2::splat(s.half_tiles * TILE_SIZE),
                    kind: BodyKind::Walker,
                },
                8.0,
            ),
            Mesh2d(mesh.clone()),
            MeshMaterial2d(material.clone()),
            BaseMaterial(material.clone()),
        ));
        // Das Verhalten hängt vom Typ ab: eine Komponente pro Verhalten.
        match spawn.kind {
            EnemyKind::Chaser => entity.insert(Chaser),
            EnemyKind::Shooter => entity.insert(Shooter { cooldown: 1.0 }),
            EnemyKind::Charger => entity.insert(Charger {
                state: ChargeState::Stalking,
            }),
            EnemyKind::Boss => entity.insert(Boss {
                volley_cooldown: 2.0,
            }),
        };
    }
    plan.len()
}

// --- Gemeinsame Hilfen --------------------------------------------------------

/// Distanzkarte zum Spieler für den aktuellen Raum.
#[derive(Resource)]
pub struct PlayerFlow {
    room: GridPos,
    /// Layout-Stand, für den das Feld berechnet wurde (Bomben ändern Räume).
    revision: u32,
    field: FlowField,
}

fn update_flow_field(
    mut commands: Commands,
    flow: Option<Res<PlayerFlow>>,
    room: Res<CurrentRoom>,
    player: Single<&Position, With<Player>>,
) {
    let target = room.tile_at(player.0);
    let up_to_date = flow.is_some_and(|f| {
        f.room == room.pos && f.revision == room.revision && f.field.target() == target
    });
    if up_to_date {
        return;
    }
    let field = FlowField::compute(target, |p| !room.blocks(p, BodyKind::Walker));
    commands.insert_resource(PlayerFlow {
        room: room.pos,
        revision: room.revision,
        field,
    });
}

/// Richtung zum Spieler: direkt, wenn freie Bahn, sonst entlang des Flowfields.
fn chase_direction(
    flow: Option<&PlayerFlow>,
    room: &CurrentRoom,
    from: Vec2,
    player: Vec2,
) -> Vec2 {
    let direct = (player - from).normalize_or_zero();
    if room.line_of_sight(from, player, BodyKind::Walker) {
        return direct;
    }
    let tile = room.tile_at(from);
    flow.filter(|f| f.room == room.pos)
        .and_then(|f| f.field.next_step(tile))
        .map(|next| (room.tile_center(next) - from).normalize_or_zero())
        .unwrap_or(direct)
}

/// Sanft auf eine Wunschgeschwindigkeit zusteuern (wie beim Spieler).
fn steer(velocity: &mut Velocity, desired: Vec2, acceleration: f32, dt: f32) {
    let t = 1.0 - (-acceleration * dt).exp();
    velocity.0 = velocity.0.lerp(desired, t);
}

fn enemy_shot(from: Vec2, dir: Vec2, speed_tiles: f32, assets: &GameAssets) -> impl Bundle {
    let speed = speed_tiles * TILE_SIZE;
    shot_bundle(
        Shot {
            position: from,
            velocity: dir * speed,
            lifetime: 9.0 * TILE_SIZE / speed,
            damage: 1.0,
            faction: Faction::Enemy,
            piercing: false,
            homing: false,
        },
        assets,
    )
}

fn wake_up(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Asleep, &mut Velocity)>,
) {
    for (entity, mut asleep, mut velocity) in &mut query {
        asleep.0 -= time.delta_secs();
        // Rückstoß im Schlaf abklingen lassen.
        velocity.0 *= 0.85;
        if asleep.0 <= 0.0 {
            commands.entity(entity).remove::<Asleep>();
        }
    }
}

// --- Verhalten ------------------------------------------------------------------

fn chaser_ai(
    time: Res<Time>,
    room: Res<CurrentRoom>,
    flow: Option<Res<PlayerFlow>>,
    player: Single<&Position, With<Player>>,
    mut query: Query<(&Position, &mut Velocity, &Mobility), (With<Chaser>, Without<Asleep>)>,
) {
    let dt = time.delta_secs();
    for (pos, mut vel, mob) in &mut query {
        let dir = chase_direction(flow.as_deref(), &room, pos.0, player.0);
        steer(&mut vel, dir * mob.speed * TILE_SIZE, mob.acceleration, dt);
    }
}

/// Hält Abstand (zu nah: weg, zu weit: hin) und schießt bei freier Sicht.
fn shooter_ai(
    mut commands: Commands,
    time: Res<Time>,
    room: Res<CurrentRoom>,
    flow: Option<Res<PlayerFlow>>,
    assets: Res<GameAssets>,
    player: Single<&Position, With<Player>>,
    mut query: Query<(&Position, &mut Velocity, &Mobility, &mut Shooter), Without<Asleep>>,
) {
    const TOO_CLOSE: f32 = 3.5;
    const TOO_FAR: f32 = 6.0;
    const FIRE_DELAY: f32 = 1.6;
    const SHOT_SPEED: f32 = 5.0;

    let dt = time.delta_secs();
    for (pos, mut vel, mob, mut shooter) in &mut query {
        let to_player = player.0 - pos.0;
        let dist_tiles = to_player.length() / TILE_SIZE;

        let dir = if dist_tiles < TOO_CLOSE {
            -to_player.normalize_or_zero()
        } else if dist_tiles > TOO_FAR {
            chase_direction(flow.as_deref(), &room, pos.0, player.0)
        } else {
            Vec2::ZERO
        };
        steer(&mut vel, dir * mob.speed * TILE_SIZE, mob.acceleration, dt);

        shooter.cooldown -= dt;
        if shooter.cooldown <= 0.0 && room.line_of_sight(pos.0, player.0, BodyKind::Projectile) {
            shooter.cooldown = FIRE_DELAY;
            commands.spawn(enemy_shot(
                pos.0,
                to_player.normalize_or_zero(),
                SHOT_SPEED,
                &assets,
            ));
        }
    }
}

/// Schleichen → ausholen → sprinten → benommen → schleichen.
fn charger_ai(
    time: Res<Time>,
    room: Res<CurrentRoom>,
    flow: Option<Res<PlayerFlow>>,
    player: Single<&Position, With<Player>>,
    mut query: Query<(&Position, &mut Velocity, &Mobility, &mut Charger), Without<Asleep>>,
) {
    const WINDUP_SECS: f32 = 0.4;
    const STUN_SECS: f32 = 0.8;
    const CHARGE_SPEED_TILES: f32 = 9.0;
    const ALIGN_TOLERANCE_TILES: f32 = 0.45;
    const MAX_RANGE_TILES: f32 = 9.0;

    let dt = time.delta_secs();
    let charge_speed = CHARGE_SPEED_TILES * TILE_SIZE;

    for (pos, mut vel, mob, mut charger) in &mut query {
        let to_player = player.0 - pos.0;

        let current = charger.state;
        charger.state = match current {
            ChargeState::Stalking => {
                let dir = chase_direction(flow.as_deref(), &room, pos.0, player.0);
                steer(&mut vel, dir * mob.speed * TILE_SIZE, mob.acceleration, dt);

                let tol = ALIGN_TOLERANCE_TILES * TILE_SIZE;
                let aligned = to_player.x.abs() < tol || to_player.y.abs() < tol;
                let in_range = to_player.length() < MAX_RANGE_TILES * TILE_SIZE;
                if aligned && in_range && room.line_of_sight(pos.0, player.0, BodyKind::Walker) {
                    // Auf die Hauptachse runden: Charger sprinten nur gerade.
                    let dir = if to_player.x.abs() > to_player.y.abs() {
                        Vec2::new(to_player.x.signum(), 0.0)
                    } else {
                        Vec2::new(0.0, to_player.y.signum())
                    };
                    ChargeState::Windup {
                        remaining: WINDUP_SECS,
                        dir,
                    }
                } else {
                    ChargeState::Stalking
                }
            }
            ChargeState::Windup { remaining, dir } => {
                steer(&mut vel, Vec2::ZERO, 12.0, dt);
                if remaining - dt <= 0.0 {
                    vel.0 = dir * charge_speed;
                    ChargeState::Charging { dir }
                } else {
                    ChargeState::Windup {
                        remaining: remaining - dt,
                        dir,
                    }
                }
            }
            ChargeState::Charging { dir } => {
                // Die Physik setzt die Geschwindigkeit bei einem Aufprall auf 0.
                // Ist davon in Sprintrichtung nicht mehr viel übrig, sind wir gegen
                // etwas gelaufen (Wand, Fels – oder ein Treffer hat uns gebremst).
                if vel.0.dot(dir) < charge_speed * 0.5 {
                    ChargeState::Stunned {
                        remaining: STUN_SECS,
                    }
                } else {
                    vel.0 = dir * charge_speed;
                    ChargeState::Charging { dir }
                }
            }
            ChargeState::Stunned { remaining } => {
                steer(&mut vel, Vec2::ZERO, 10.0, dt);
                if remaining - dt <= 0.0 {
                    ChargeState::Stalking
                } else {
                    ChargeState::Stunned {
                        remaining: remaining - dt,
                    }
                }
            }
        };
    }
}

/// Verfolgt langsam und feuert regelmäßig einen Ring aus 8 Schüssen.
fn boss_ai(
    mut commands: Commands,
    time: Res<Time>,
    room: Res<CurrentRoom>,
    flow: Option<Res<PlayerFlow>>,
    assets: Res<GameAssets>,
    player: Single<&Position, With<Player>>,
    mut query: Query<(&Position, &mut Velocity, &Mobility, &mut Boss), Without<Asleep>>,
) {
    const VOLLEY_DELAY: f32 = 2.8;
    const VOLLEY_SHOTS: u32 = 8;
    const SHOT_SPEED: f32 = 4.5;

    let dt = time.delta_secs();
    for (pos, mut vel, mob, mut boss) in &mut query {
        let dir = chase_direction(flow.as_deref(), &room, pos.0, player.0);
        steer(&mut vel, dir * mob.speed * TILE_SIZE, mob.acceleration, dt);

        boss.volley_cooldown -= dt;
        if boss.volley_cooldown <= 0.0 {
            boss.volley_cooldown = VOLLEY_DELAY;
            // Ring leicht zum Spieler hin gedreht, damit ein Schuss direkt zielt.
            let base = (player.0 - pos.0).to_angle();
            for i in 0..VOLLEY_SHOTS {
                let angle = base + i as f32 * TAU / VOLLEY_SHOTS as f32;
                commands.spawn(enemy_shot(
                    pos.0,
                    Vec2::from_angle(angle),
                    SHOT_SPEED,
                    &assets,
                ));
            }
        }
    }
}

/// Überlappende Gegner auseinanderschieben, damit sie nicht zu einem Klumpen werden.
///
/// `iter_combinations_mut` liefert jedes Paar genau einmal – mit zwei
/// gleichzeitigen `&mut`-Zugriffen, die der Borrow-Checker sonst nie erlauben würde.
fn separate_enemies(mut query: Query<(&Position, &mut Velocity, &Body), With<Enemy>>) {
    let mut pairs = query.iter_combinations_mut();
    while let Some([(pa, mut va, ba), (pb, mut vb, bb)]) = pairs.fetch_next() {
        let delta = pa.0 - pb.0;
        let min_dist = ba.half_size.x + bb.half_size.x;
        let dist = delta.length();
        if dist < min_dist && dist > 0.01 {
            // Überlappung (Pixel) × Stärke (1/s) = Ausweichgeschwindigkeit (Pixel/s).
            let push = delta / dist * (min_dist - dist) * SEPARATION_STRENGTH;
            va.0 += push;
            vb.0 -= push;
        }
    }
}
