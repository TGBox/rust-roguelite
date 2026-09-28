//! Gemeinsame Meshes und Materialien.
//!
//! Einmal erzeugen, überall per Handle klonen: Ein `Handle` ist nur ein
//! Referenzzähler, die eigentlichen GPU-Daten existieren genau einmal.
//! In M8 kommen hier Sprites und Sounds dazu.

use bevy::prelude::*;

use crate::{
    TILE_SIZE,
    enemy::{BOSS_HALF_TILES, ENEMY_HALF_TILES},
    inventory::{ITEM_HALF_TILES, PICKUP_HALF_TILES},
    player::PLAYER_RADIUS_TILES,
    projectile::TEAR_RADIUS_TILES,
};

pub struct GameAssetsPlugin;

impl Plugin for GameAssetsPlugin {
    fn build(&self, app: &mut App) {
        // `init_resource` ruft sofort `FromWorld::from_world` auf.
        // Das funktioniert, weil `DefaultPlugins` die Asset-Speicher vorher registriert haben.
        app.init_resource::<GameAssets>();
    }
}

#[derive(Resource)]
pub struct GameAssets {
    pub tile_mesh: Handle<Mesh>,
    pub floor: Handle<ColorMaterial>,
    pub wall: Handle<ColorMaterial>,
    pub rock: Handle<ColorMaterial>,
    pub pit: Handle<ColorMaterial>,
    pub door_open: Handle<ColorMaterial>,
    pub door_closed: Handle<ColorMaterial>,
    pub player_mesh: Handle<Mesh>,
    pub player_material: Handle<ColorMaterial>,
    pub player_dead_material: Handle<ColorMaterial>,
    pub tear_mesh: Handle<Mesh>,
    pub tear_material: Handle<ColorMaterial>,
    pub enemy_mesh: Handle<Mesh>,
    pub boss_mesh: Handle<Mesh>,
    pub chaser_material: Handle<ColorMaterial>,
    pub shooter_material: Handle<ColorMaterial>,
    pub charger_material: Handle<ColorMaterial>,
    pub boss_material: Handle<ColorMaterial>,
    pub enemy_shot_material: Handle<ColorMaterial>,
    /// Kurzes weißes Aufblitzen bei Treffern.
    pub flash_material: Handle<ColorMaterial>,
    pub pickup_mesh: Handle<Mesh>,
    pub item_mesh: Handle<Mesh>,
    pub half_heart_material: Handle<ColorMaterial>,
    pub heart_material: Handle<ColorMaterial>,
    pub coin_material: Handle<ColorMaterial>,
    pub key_material: Handle<ColorMaterial>,
    pub bomb_material: Handle<ColorMaterial>,
    pub item_material: Handle<ColorMaterial>,
}

/// `FromWorld` statt `Default`: Wir brauchen Zugriff auf andere Ressourcen
/// (`Assets<Mesh>`, `Assets<ColorMaterial>`), um die Handles zu erzeugen.
impl FromWorld for GameAssets {
    fn from_world(world: &mut World) -> Self {
        // Eigener Block, damit der mutable Borrow auf `Assets<Mesh>` endet,
        // bevor wir `Assets<ColorMaterial>` ausleihen.
        let (tile_mesh, player_mesh, tear_mesh, enemy_mesh, boss_mesh, pickup_mesh, item_mesh) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (
                // 1 px kleiner als die Kachel: ergibt ein dezentes Raster.
                meshes.add(Rectangle::from_size(Vec2::splat(TILE_SIZE - 1.0))),
                meshes.add(Circle::new(PLAYER_RADIUS_TILES * TILE_SIZE)),
                meshes.add(Circle::new(TEAR_RADIUS_TILES * TILE_SIZE)),
                meshes.add(Rectangle::from_size(Vec2::splat(
                    ENEMY_HALF_TILES * 2.0 * TILE_SIZE,
                ))),
                meshes.add(Rectangle::from_size(Vec2::splat(
                    BOSS_HALF_TILES * 2.0 * TILE_SIZE,
                ))),
                meshes.add(Circle::new(PICKUP_HALF_TILES * TILE_SIZE)),
                // Raute: ein um 45° gedrehtes Quadrat als Item-Symbol.
                meshes.add(Rhombus::new(
                    ITEM_HALF_TILES * 2.0 * TILE_SIZE,
                    ITEM_HALF_TILES * 2.0 * TILE_SIZE,
                )),
            )
        };

        let mut materials = world.resource_mut::<Assets<ColorMaterial>>();
        Self {
            tile_mesh,
            floor: materials.add(Color::srgb(0.16, 0.14, 0.13)),
            wall: materials.add(Color::srgb(0.35, 0.30, 0.28)),
            rock: materials.add(Color::srgb(0.50, 0.47, 0.44)),
            pit: materials.add(Color::srgb(0.02, 0.02, 0.03)),
            door_open: materials.add(Color::srgb(0.22, 0.18, 0.12)),
            door_closed: materials.add(Color::srgb(0.55, 0.35, 0.15)),
            player_mesh,
            player_material: materials.add(Color::srgb(0.85, 0.75, 0.55)),
            player_dead_material: materials.add(Color::srgb(0.55, 0.12, 0.12)),
            tear_mesh,
            tear_material: materials.add(Color::srgb(0.55, 0.75, 0.95)),
            enemy_mesh,
            boss_mesh,
            chaser_material: materials.add(Color::srgb(0.78, 0.25, 0.22)),
            shooter_material: materials.add(Color::srgb(0.90, 0.55, 0.15)),
            charger_material: materials.add(Color::srgb(0.35, 0.40, 0.85)),
            boss_material: materials.add(Color::srgb(0.60, 0.15, 0.50)),
            enemy_shot_material: materials.add(Color::srgb(0.95, 0.35, 0.30)),
            flash_material: materials.add(Color::srgb(1.0, 1.0, 1.0)),
            pickup_mesh,
            item_mesh,
            half_heart_material: materials.add(Color::srgb(0.95, 0.45, 0.50)),
            heart_material: materials.add(Color::srgb(0.90, 0.10, 0.15)),
            coin_material: materials.add(Color::srgb(0.95, 0.80, 0.20)),
            key_material: materials.add(Color::srgb(0.75, 0.78, 0.85)),
            bomb_material: materials.add(Color::srgb(0.12, 0.12, 0.14)),
            item_material: materials.add(Color::srgb(0.40, 0.90, 0.95)),
        }
    }
}
