//! Gegner: Werte, Spawnen und KI.
//!
//! Jeder Gegnertyp hat eine eigene **Verhaltens-Komponente** (`Chaser`,
//! `Shooter`, `Charger`, …) und ein eigenes System. Ein System fragt
//! nur „seine“ Gegner ab – neue Typen kommen hinzu, ohne bestehende zu ändern.
//!
//! Aufteilung:
//! - hier: Komponenten, Werte, Spawnen, einfache Gegner
//! - `enemy/advanced.rs`: Springer, Teiler, Beschwörer
//! - `enemy/bosses.rs`: die drei Bosse mit Angriffsphasen
//!
//! Alle Gegner bewegen sich über ein gemeinsames **Flowfield** zum Spieler
//! (siehe `dungeon_gen::pathing`), das nur neu berechnet wird, wenn der
//! Spieler die Kachel wechselt.

use bevy::prelude::*;
use dungeon_gen::{EnemyKind, GridPos, meta, pathing::FlowField, spawns};

mod advanced;
pub use bosses::BroodAi;
mod bosses;

use crate::{
    TILE_SIZE,
    assets::{ActorArt, GameAssets},
    audio::{Effect, Sfx},
    combat::{ContactDamage, Faction, FlashArt, Health},
    juice::{FxColor, Wobble},
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
                (
                    chaser_ai,
                    shooter_ai,
                    charger_ai,
                    advanced::hopper_ai,
                    advanced::summoner_ai,
                    bosses::king_ai,
                    bosses::brood_ai,
                    bosses::warden_ai,
                ),
                separate_enemies,
            )
                .chain()
                .in_set(GameSet::Control),
        )
        .add_systems(
            FixedUpdate,
            advanced::split_on_death
                .in_set(GameSet::Cleanup)
                .before(crate::room::unlock_when_cleared),
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

/// Dauer des Ausholens vor dem Sprint (s).
const CHARGE_WINDUP_SECS: f32 = 0.4;

/// Nur lesende Abfragen für die Optik (`juice.rs`). Der Zustand selbst bleibt
/// privat: Von außen kann niemand die Zustandsmaschine durcheinanderbringen.
impl Charger {
    /// 0.0 (fängt an) … 1.0 (sprintet gleich), sonst `None`.
    pub fn windup_progress(&self) -> Option<f32> {
        match self.state {
            ChargeState::Windup { remaining, .. } => Some(1.0 - remaining / CHARGE_WINDUP_SECS),
            _ => None,
        }
    }

    pub fn is_stunned(&self) -> bool {
        matches!(self.state, ChargeState::Stunned { .. })
    }
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

/// Markiert alle Bosse (für HUD-Lebensbalken, Frost-Immunität, Effekte).
#[derive(Component)]
pub struct Boss;

/// Welcher Typ dieser Gegner ist (für Tod-Effekte wie das Teilen).
#[derive(Component, Debug, Clone, Copy)]
pub struct EnemyType(pub EnemyKind);

/// Champion: doppeltes Leben, größer, goldener Schimmer, lässt Beute fallen.
#[derive(Component)]
pub struct Champion;

// --- Werte je Typ -------------------------------------------------------------

struct EnemyStats {
    health: f32,
    contact_damage: f32,
    half_tiles: f32,
    mobility: Mobility,
}

fn stats(kind: EnemyKind) -> EnemyStats {
    let e = |health, contact_damage, half_tiles, speed, acceleration| EnemyStats {
        health,
        contact_damage,
        half_tiles,
        mobility: Mobility {
            speed,
            acceleration,
        },
    };
    match kind {
        //                        Leben  Kontakt  Hitbox           Tempo  Beschl.
        EnemyKind::Chaser => e(6.0, 1.0, ENEMY_HALF_TILES, 2.6, 8.0),
        EnemyKind::Shooter => e(5.0, 1.0, ENEMY_HALF_TILES, 2.0, 6.0),
        EnemyKind::Charger => e(8.0, 1.0, ENEMY_HALF_TILES, 1.4, 6.0),
        EnemyKind::Hopper => e(7.0, 1.0, 0.36, 0.0, 10.0),
        EnemyKind::Splitter => e(10.0, 1.0, 0.45, 1.6, 5.0),
        EnemyKind::Splitling => e(2.5, 1.0, 0.24, 3.4, 10.0),
        EnemyKind::Summoner => e(8.0, 1.0, 0.36, 1.8, 6.0),
        EnemyKind::Boss => e(50.0, 2.0, BOSS_HALF_TILES, 1.3, 3.0),
        EnemyKind::BroodMother => e(75.0, 2.0, 0.85, 1.1, 3.0),
        EnemyKind::Warden => e(100.0, 2.0, BOSS_HALF_TILES, 0.8, 2.0),
    }
}

/// Grafik und Partikelfarbe je Typ.
fn look(kind: EnemyKind, assets: &GameAssets) -> (&ActorArt, Color) {
    let a = &assets.actors;
    match kind {
        EnemyKind::Chaser => (&a.chaser, Color::srgb(0.80, 0.25, 0.22)),
        EnemyKind::Shooter => (&a.shooter, Color::srgb(0.95, 0.60, 0.20)),
        EnemyKind::Charger => (&a.charger, Color::srgb(0.35, 0.45, 0.90)),
        EnemyKind::Hopper => (&a.hopper, Color::srgb(0.40, 0.75, 0.30)),
        EnemyKind::Splitter | EnemyKind::Splitling => (
            if kind == EnemyKind::Splitter {
                &a.splitter
            } else {
                &a.splitling
            },
            Color::srgb(0.65, 0.40, 0.85),
        ),
        EnemyKind::Summoner => (&a.summoner, Color::srgb(0.90, 0.85, 0.75)),
        EnemyKind::Boss => (&a.boss, Color::srgb(0.60, 0.20, 0.55)),
        EnemyKind::BroodMother => (&a.brood_mother, Color::srgb(0.45, 0.70, 0.30)),
        EnemyKind::Warden => (&a.warden, Color::srgb(0.55, 0.85, 1.0)),
    }
}

/// Beschreibt einen zu spawnenden Gegner. Struktur statt langer Argumentliste.
#[derive(Debug, Clone, Copy)]
pub struct EnemySpawn {
    pub kind: EnemyKind,
    /// Weltposition.
    pub pos: Vec2,
    pub depth: u32,
    pub champion: bool,
    /// Wie lange er nach dem Erscheinen noch stillhält (s).
    pub asleep: f32,
}

/// Spawnt einen Gegner – für Raumbelegung, Teilen und Beschwören.
pub fn spawn_enemy(commands: &mut Commands, spawn: EnemySpawn, assets: &GameAssets) -> Entity {
    let mut s = stats(spawn.kind);
    // Tiefere Etagen: zähere und etwas schnellere Gegner.
    s.health *= meta::enemy_health_multiplier(spawn.depth);
    s.mobility.speed *= meta::enemy_speed_multiplier(spawn.depth);
    if spawn.champion {
        s.health *= 2.0;
    }
    let (art, blood) = look(spawn.kind, assets);
    let size = art.size * if spawn.champion { 1.25 } else { 1.0 };

    let mut entity = commands.spawn((
        Name::new(format!("{:?}", spawn.kind)),
        Enemy,
        EnemyType(spawn.kind),
        Faction::Enemy,
        Health::full(s.health),
        ContactDamage(s.contact_damage),
        s.mobility,
        Asleep(spawn.asleep),
        RoomScoped,
        DespawnOnExit(AppState::InGame),
        physics_body(
            spawn.pos,
            Vec2::ZERO,
            Body {
                half_size: Vec2::splat(s.half_tiles * TILE_SIZE),
                kind: BodyKind::Walker,
            },
            8.0,
        ),
        Sprite {
            image: art.image.clone(),
            custom_size: Some(size),
            ..default()
        },
        FlashArt::from(art),
        Wobble::new(size, spawn.pos),
        FxColor(blood),
    ));
    if spawn.champion {
        entity.insert(Champion);
    }
    if spawn.kind.is_boss() {
        entity.insert(Boss);
    }
    // Das Verhalten hängt vom Typ ab: eine Komponente pro Verhalten.
    // Teiler und Splitlinge verfolgen einfach – ihr Besonderes passiert beim Tod.
    match spawn.kind {
        EnemyKind::Chaser | EnemyKind::Splitter | EnemyKind::Splitling => {
            entity.insert(Chaser);
        }
        EnemyKind::Shooter => {
            entity.insert(Shooter { cooldown: 1.0 });
        }
        EnemyKind::Charger => {
            entity.insert(Charger {
                state: ChargeState::Stalking,
            });
        }
        EnemyKind::Hopper => {
            entity.insert(advanced::Hopper::new(spawn.depth));
        }
        EnemyKind::Summoner => {
            entity.insert(advanced::Summoner::default());
        }
        EnemyKind::Boss => {
            entity.insert(bosses::KingAi::default());
        }
        EnemyKind::BroodMother => {
            entity.insert(bosses::BroodAi::default());
        }
        EnemyKind::Warden => {
            entity.insert(bosses::WardenAi::default());
        }
    }
    entity.id()
}

/// Spawnt die aktuelle Welle (`CurrentRoom::wave`) des Raums und gibt die
/// Anzahl zurück. Typen und Positionen kommen deterministisch aus `dungeon_gen::spawns`.
pub fn spawn_room_enemies(
    commands: &mut Commands,
    run: &Run,
    room: &CurrentRoom,
    assets: &GameAssets,
) -> usize {
    let Some(info) = run.floor.get(room.pos) else {
        return 0;
    };
    let depth = run.floor.depth;
    let mut rng = spawns::wave_rng(run.seed, depth, room.pos, room.wave);
    let plan = spawns::plan_wave(info.kind, &room.layout, depth, room.wave, &mut rng);
    for spawn in &plan {
        spawn_enemy(
            commands,
            EnemySpawn {
                kind: spawn.kind,
                pos: room.tile_center(spawn.pos),
                depth,
                champion: spawn.champion,
                asleep: WAKE_UP_SECS,
            },
            assets,
        );
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
        Shot::plain(
            from,
            dir * speed,
            9.0 * TILE_SIZE / speed,
            1.0,
            Faction::Enemy,
        ),
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
    mut sfx: MessageWriter<Sfx>,
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
            sfx.write(Sfx(Effect::EnemyShoot));
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
                        remaining: CHARGE_WINDUP_SECS,
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
