//! Gemeinsame Meshes und Materialien.
//!
//! Einmal erzeugen, überall per Handle klonen: Ein `Handle` ist nur ein
//! Referenzzähler, die eigentlichen GPU-Daten existieren genau einmal.
//! In M8 kommen hier Sprites und Sounds dazu.

use bevy::prelude::*;

use crate::{TILE_SIZE, player::PLAYER_RADIUS_TILES, projectile::TEAR_RADIUS_TILES};

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
    pub player_mesh: Handle<Mesh>,
    pub player_material: Handle<ColorMaterial>,
    pub player_dead_material: Handle<ColorMaterial>,
    pub tear_mesh: Handle<Mesh>,
    pub tear_material: Handle<ColorMaterial>,
}

/// `FromWorld` statt `Default`: Wir brauchen Zugriff auf andere Ressourcen
/// (`Assets<Mesh>`, `Assets<ColorMaterial>`), um die Handles zu erzeugen.
impl FromWorld for GameAssets {
    fn from_world(world: &mut World) -> Self {
        // Eigener Block, damit der mutable Borrow auf `Assets<Mesh>` endet,
        // bevor wir `Assets<ColorMaterial>` ausleihen.
        let (tile_mesh, player_mesh, tear_mesh) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (
                // 1 px kleiner als die Kachel: ergibt ein dezentes Raster.
                meshes.add(Rectangle::from_size(Vec2::splat(TILE_SIZE - 1.0))),
                meshes.add(Circle::new(PLAYER_RADIUS_TILES * TILE_SIZE)),
                meshes.add(Circle::new(TEAR_RADIUS_TILES * TILE_SIZE)),
            )
        };

        let mut materials = world.resource_mut::<Assets<ColorMaterial>>();
        Self {
            tile_mesh,
            floor: materials.add(Color::srgb(0.16, 0.14, 0.13)),
            wall: materials.add(Color::srgb(0.35, 0.30, 0.28)),
            rock: materials.add(Color::srgb(0.50, 0.47, 0.44)),
            pit: materials.add(Color::srgb(0.02, 0.02, 0.03)),
            player_mesh,
            player_material: materials.add(Color::srgb(0.85, 0.75, 0.55)),
            player_dead_material: materials.add(Color::srgb(0.55, 0.12, 0.12)),
            tear_mesh,
            tear_material: materials.add(Color::srgb(0.55, 0.75, 0.95)),
        }
    }
}
