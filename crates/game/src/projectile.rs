//! Projektile („Tears“): fliegen geradeaus, verschwinden nach Ablauf
//! ihrer Lebenszeit oder beim Aufprall auf Wand/Fels.

use bevy::prelude::*;

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    physics::{Body, BodyKind, TileHit, physics_body},
    schedule::GameSet,
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
}

pub fn tear_bundle(
    position: Vec2,
    velocity: Vec2,
    lifetime: f32,
    assets: &GameAssets,
) -> impl Bundle {
    (
        Name::new("Tear"),
        Projectile {
            remaining: lifetime,
        },
        physics_body(
            position,
            velocity,
            Body {
                half_size: Vec2::splat(TEAR_RADIUS_TILES * TILE_SIZE),
                kind: BodyKind::Projectile,
            },
            5.0,
        ),
        Mesh2d(assets.tear_mesh.clone()),
        MeshMaterial2d(assets.tear_material.clone()),
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
            // Tick auch durch einen Wandtreffer entfernt wird.
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
