//! Projektile („Tears“): fliegen geradeaus, verschwinden nach Ablauf
//! ihrer Lebenszeit oder beim Aufprall auf Wand/Fels. Treffer auf
//! Spieler/Gegner behandelt `combat.rs`.

use bevy::prelude::*;

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    combat::Faction,
    physics::{Body, BodyKind, TileHit, physics_body},
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
        );
    }
}

#[derive(Component, Debug)]
pub struct Projectile {
    /// Verbleibende Flugzeit (s).
    pub remaining: f32,
    pub damage: i32,
    /// Wer geschossen hat. Trifft nur die jeweils andere Seite.
    pub faction: Faction,
}

/// Alle Parameter eines Schusses. Eine Struktur statt sechs Funktionsargumenten:
/// Die Aufrufer benennen jeden Wert, Verwechslungen sind ausgeschlossen.
#[derive(Debug, Clone, Copy)]
pub struct Shot {
    pub position: Vec2,
    pub velocity: Vec2,
    pub lifetime: f32,
    pub damage: i32,
    pub faction: Faction,
}

pub fn shot_bundle(shot: Shot, assets: &GameAssets) -> impl Bundle {
    let material = match shot.faction {
        Faction::Player => &assets.tear_material,
        Faction::Enemy => &assets.enemy_shot_material,
    };
    (
        Name::new("Shot"),
        DespawnOnExit(AppState::InGame),
        RoomScoped,
        Projectile {
            remaining: shot.lifetime,
            damage: shot.damage,
            faction: shot.faction,
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
        Mesh2d(assets.tear_mesh.clone()),
        MeshMaterial2d(material.clone()),
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
    projectiles: Query<(), With<Projectile>>,
) {
    for hit in hits.read() {
        // Der Physik-Code meldet Treffer für ALLE Körper. Hier interessieren nur Projektile.
        if projectiles.contains(hit.entity) {
            commands.entity(hit.entity).try_despawn();
        }
    }
}
