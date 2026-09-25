//! Gegner. In M4 nur **Übungsgegner**: stehen still, sterben nach ein paar
//! Treffern. Sie existieren, damit sich Türen schließen und wieder öffnen.
//! In M5 bekommen sie KI, Schaden und eigene Typen.

use bevy::prelude::*;
use dungeon_gen::{RoomKind, collision::aabb_overlap, spawns};

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    physics::{Body, BodyKind, Position, physics_body},
    projectile::Projectile,
    room::{CurrentRoom, RoomScoped},
    run::Run,
    schedule::GameSet,
    states::AppState,
};

pub const DUMMY_HALF_TILES: f32 = 0.4;
pub const BOSS_HALF_TILES: f32 = 0.8;
const DUMMY_HEALTH: i32 = 3;
const BOSS_HEALTH: i32 = 15;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            projectiles_hit_enemies.in_set(GameSet::Cleanup),
        );
    }
}

#[derive(Component)]
pub struct Enemy;

/// Vorläufig hier; wandert in M5 in ein `combat`-Modul.
#[derive(Component, Debug)]
pub struct Health(pub i32);

/// Spawnt die Gegner für den aktuellen Raum und gibt ihre Anzahl zurück.
/// Die Positionen kommen deterministisch aus `dungeon_gen::spawns`.
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
    let points = spawns::plan_spawns(info.kind, &room.layout, &mut rng);

    let is_boss = info.kind == RoomKind::Boss;
    let (half, health, mesh, material) = if is_boss {
        (
            BOSS_HALF_TILES,
            BOSS_HEALTH,
            &assets.boss_mesh,
            &assets.boss_material,
        )
    } else {
        (
            DUMMY_HALF_TILES,
            DUMMY_HEALTH,
            &assets.enemy_mesh,
            &assets.enemy_material,
        )
    };

    for &p in &points {
        commands.spawn((
            Name::new(if is_boss { "BossDummy" } else { "Dummy" }),
            Enemy,
            Health(health),
            RoomScoped,
            DespawnOnExit(AppState::InGame),
            physics_body(
                room.tile_center(p),
                Vec2::ZERO,
                Body {
                    half_size: Vec2::splat(half * TILE_SIZE),
                    kind: BodyKind::Walker,
                },
                8.0,
            ),
            Mesh2d(mesh.clone()),
            MeshMaterial2d(material.clone()),
        ));
    }
    points.len()
}

/// Einfacher Treffer-Check: jedes Projektil gegen jeden Gegner (O(n·m)).
/// Bei ein paar Dutzend Objekten völlig ausreichend.
fn projectiles_hit_enemies(
    mut commands: Commands,
    projectiles: Query<(Entity, &Position, &Body), With<Projectile>>,
    mut enemies: Query<(Entity, &Position, &Body, &mut Health), With<Enemy>>,
) {
    for (shot, shot_pos, shot_body) in &projectiles {
        for (enemy, enemy_pos, enemy_body, mut health) in &mut enemies {
            // Schon tot, aber noch nicht entfernt (Commands wirken erst später).
            if health.0 <= 0 {
                continue;
            }
            let hit = aabb_overlap(
                shot_pos.0.to_array(),
                shot_body.half_size.to_array(),
                enemy_pos.0.to_array(),
                enemy_body.half_size.to_array(),
            );
            if hit {
                health.0 -= 1;
                commands.entity(shot).try_despawn();
                if health.0 <= 0 {
                    commands.entity(enemy).try_despawn();
                }
                // Ein Projektil trifft nur einen Gegner.
                break;
            }
        }
    }
}
