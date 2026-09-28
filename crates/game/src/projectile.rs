//! Projektile („Tears“): fliegen geradeaus, verschwinden nach Ablauf
//! ihrer Lebenszeit oder beim Aufprall auf Wand/Fels. Treffer auf
//! Spieler/Gegner behandelt `combat.rs`.

use bevy::prelude::*;

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    combat::{Faction, Health},
    juice::Fx,
    physics::{Body, BodyKind, Position, TileHit, Velocity, physics_body},
    room::RoomScoped,
    schedule::GameSet,
    states::AppState,
};

pub const TEAR_RADIUS_TILES: f32 = 0.15;

pub struct ProjectilePlugin;

impl Plugin for ProjectilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (tick_lifetime, despawn_on_tile_hit).in_set(GameSet::Cleanup),
        )
        .add_systems(FixedUpdate, steer_homing_shots.in_set(GameSet::Control));
    }
}

#[derive(Component, Debug)]
pub struct Projectile {
    /// Verbleibende Flugzeit (s).
    pub remaining: f32,
    pub damage: f32,
    /// Wer geschossen hat. Trifft nur die jeweils andere Seite.
    pub faction: Faction,
    /// Fliegt durch Ziele hindurch.
    pub piercing: bool,
    /// Lenkt zum nächsten Ziel.
    pub homing: bool,
    /// Bei durchschlagenden Schüssen: schon getroffene Ziele.
    pub already_hit: Vec<Entity>,
}

/// Alle Parameter eines Schusses. Eine Struktur statt sechs Funktionsargumenten:
/// Die Aufrufer benennen jeden Wert, Verwechslungen sind ausgeschlossen.
#[derive(Debug, Clone, Copy)]
pub struct Shot {
    pub position: Vec2,
    pub velocity: Vec2,
    pub lifetime: f32,
    pub damage: f32,
    pub faction: Faction,
    pub piercing: bool,
    pub homing: bool,
}

pub fn shot_bundle(shot: Shot, assets: &GameAssets) -> impl Bundle {
    let art = match shot.faction {
        Faction::Player => &assets.actors.tear,
        Faction::Enemy => &assets.actors.enemy_shot,
    };
    (
        Name::new("Shot"),
        DespawnOnExit(AppState::InGame),
        RoomScoped,
        Projectile {
            remaining: shot.lifetime,
            damage: shot.damage,
            faction: shot.faction,
            piercing: shot.piercing,
            homing: shot.homing,
            already_hit: Vec::new(),
        },
        physics_body(
            shot.position,
            shot.velocity,
            Body {
                half_size: Vec2::splat(TEAR_RADIUS_TILES * TILE_SIZE),
                kind: BodyKind::Projectile,
            },
            5.0,
        ),
        Sprite {
            image: art.image.clone(),
            custom_size: Some(art.size),
            ..default()
        },
    )
}

fn tick_lifetime(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Projectile)>,
) {
    let dt = time.delta_secs();
    for (entity, mut projectile) in &mut query {
        projectile.remaining -= dt;
        if projectile.remaining <= 0.0 {
            // `try_despawn`: kein Warn-Log, falls dasselbe Projektil im selben
            // Tick auch durch einen Treffer entfernt wurde.
            commands.entity(entity).try_despawn();
        }
    }
}

fn despawn_on_tile_hit(
    mut commands: Commands,
    mut hits: MessageReader<TileHit>,
    projectiles: Query<(&Position, &Projectile)>,
    mut fx: MessageWriter<Fx>,
) {
    for hit in hits.read() {
        // Der Physik-Code meldet Treffer für ALLE Körper. Hier interessieren nur Projektile.
        let Ok((pos, projectile)) = projectiles.get(hit.entity) else {
            continue;
        };
        commands.entity(hit.entity).try_despawn();
        // Kleiner Spritzer an der Wand.
        fx.write(Fx::Burst {
            at: pos.0,
            color: splash_color(projectile.faction),
            count: 5,
            speed: 60.0,
        });
    }
}

fn splash_color(faction: Faction) -> Color {
    match faction {
        Faction::Player => Color::srgb(0.55, 0.78, 0.98),
        Faction::Enemy => Color::srgb(0.95, 0.35, 0.30),
    }
}

/// Zielsuchende Schüsse drehen sich langsam zum nächsten gegnerischen Ziel.
/// Die Geschwindigkeit (Betrag) bleibt gleich, nur die Richtung ändert sich.
fn steer_homing_shots(
    time: Res<Time>,
    mut shots: Query<(&Position, &mut Velocity, &Projectile)>,
    targets: Query<(&Position, &Faction), (With<Health>, Without<Projectile>)>,
) {
    const SEARCH_RADIUS_TILES: f32 = 5.0;
    const TURN_RATE: f32 = 6.0;

    let dt = time.delta_secs();
    let radius = SEARCH_RADIUS_TILES * TILE_SIZE;
    for (pos, mut vel, projectile) in &mut shots {
        if !projectile.homing {
            continue;
        }
        let nearest = targets
            .iter()
            .filter(|(_, f)| **f != projectile.faction)
            .map(|(p, _)| p.0 - pos.0)
            .filter(|d| d.length() < radius)
            .min_by(|a, b| a.length().total_cmp(&b.length()));
        let Some(to_target) = nearest else {
            continue;
        };
        let speed = vel.0.length();
        let current = vel.0.normalize_or_zero();
        let wanted = to_target.normalize_or_zero();
        let turned = current
            .lerp(wanted, (TURN_RATE * dt).min(1.0))
            .normalize_or_zero();
        vel.0 = turned * speed;
    }
}
