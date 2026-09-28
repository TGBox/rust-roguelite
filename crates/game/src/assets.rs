//! Gemeinsame Meshes und Materialien.
//!
//! Einmal erzeugen, überall per Handle klonen: Ein `Handle` ist nur ein
//! Referenzzähler, die eigentlichen GPU-Daten existieren genau einmal.
//! In M8 kommen hier Sprites und Sounds dazu.

use bevy::prelude::*;

use crate::{
    TILE_SIZE,
    enemy::{BOSS_HALF_TILES, ENEMY_HALF_TILES},
    pixel_art,
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
    /// Tür zu einem verschlossenen Raum (Schlüssel nötig).
    pub door_keyed: Handle<ColorMaterial>,
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
    /// Schrift mit vollem Zeichensatz (Umlaute, –, ×, ✔). Wird allen Texten
    /// automatisch zugewiesen, siehe `ui::apply_game_font`.
    pub font: Handle<Font>,
    /// Explosions-Blitz einer Bombe (Kreis, wird beim Anzeigen skaliert).
    pub explosion_mesh: Handle<Mesh>,
    pub explosion_material: Handle<ColorMaterial>,
    pub sprites: Sprites,
}

/// Pixel-Grafiken aus `pixel_art.rs`.
pub struct Sprites {
    pub heart_full: Handle<Image>,
    pub heart_half: Handle<Image>,
    pub heart_empty: Handle<Image>,
    pub coin: Handle<Image>,
    pub key: Handle<Image>,
    pub bomb: Handle<Image>,
    pub item: Handle<Image>,
}

/// `FromWorld` statt `Default`: Wir brauchen Zugriff auf andere Ressourcen
/// (`Assets<Mesh>`, `Assets<ColorMaterial>`), um die Handles zu erzeugen.
impl FromWorld for GameAssets {
    fn from_world(world: &mut World) -> Self {
        // Eigener Block, damit der mutable Borrow auf `Assets<Mesh>` endet,
        // bevor wir `Assets<ColorMaterial>` ausleihen.
        let (tile_mesh, player_mesh, tear_mesh, enemy_mesh, boss_mesh, explosion_mesh) = {
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
                meshes.add(Circle::new(crate::bomb::BLAST_RADIUS_TILES * TILE_SIZE)),
            )
        };

        let font = world.resource::<AssetServer>().load("fonts/DejaVuSans.ttf");
        let sprites = {
            let mut images = world.resource_mut::<Assets<Image>>();
            use pixel_art::*;
            Sprites {
                heart_full: images.add(heart_full()),
                heart_half: images.add(heart_half()),
                heart_empty: images.add(heart_empty()),
                coin: images.add(image_from_ascii(COIN, COIN_PALETTE)),
                key: images.add(image_from_ascii(KEY, KEY_PALETTE)),
                bomb: images.add(image_from_ascii(BOMB, BOMB_PALETTE)),
                item: images.add(image_from_ascii(ITEM, ITEM_PALETTE)),
            }
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
            door_keyed: materials.add(Color::srgb(0.85, 0.70, 0.20)),
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
            font,
            sprites,
            explosion_mesh,
            explosion_material: materials.add(Color::srgb(1.0, 0.65, 0.20)),
        }
    }
}
