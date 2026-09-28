//! Gemeinsame Grafiken: Sprites, Kachel-Texturen, Schrift.
//!
//! Einmal erzeugen, überall per Handle klonen: Ein `Handle` ist nur ein
//! Referenzzähler, die eigentlichen GPU-Daten existieren genau einmal.
//! Seit M8 ist fast alles Pixel-Art aus `pixel_art` statt farbiger Formen.

use bevy::prelude::*;

use dungeon_gen::GridPos;

use crate::{
    TILE_SIZE,
    pixel_art::{self, Palette},
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
    /// Schrift mit vollem Zeichensatz (Umlaute, –, ×, ✔). Wird allen Texten
    /// automatisch zugewiesen, siehe `ui::apply_game_font`.
    pub font: Handle<Font>,
    /// Explosions-Blitz einer Bombe (Kreis, wird beim Anzeigen skaliert).
    pub explosion_mesh: Handle<Mesh>,
    pub explosion_material: Handle<ColorMaterial>,
    pub sprites: Sprites,
    pub tiles: TileArt,
    pub actors: ActorSprites,
}

/// Pickups, HUD-Symbole und Partikel aus `pixel_art`.
pub struct Sprites {
    pub heart_full: Handle<Image>,
    pub heart_half: Handle<Image>,
    pub heart_empty: Handle<Image>,
    pub coin: Handle<Image>,
    pub key: Handle<Image>,
    pub bomb: Handle<Image>,
    pub item: Handle<Image>,
    pub trapdoor: Handle<Image>,
    pub exit: Handle<Image>,
    /// 1 × 1 weiß – für Partikel, per `Sprite::color` eingefärbt.
    pub pixel: Handle<Image>,
}

/// Kachel-Texturen (prozedural, siehe `pixel_art::tiles`).
pub struct TileArt {
    /// Mehrere Bodenvarianten, damit der Boden nicht wie ein Raster aussieht.
    pub floors: Vec<Handle<Image>>,
    pub wall: Handle<Image>,
    pub rock: Handle<Image>,
    pub pit: Handle<Image>,
    pub door_open: Handle<Image>,
    pub door_closed: Handle<Image>,
    /// Tür zu einem verschlossenen Raum (Schlüssel nötig).
    pub door_keyed: Handle<Image>,
}

impl TileArt {
    /// Bodenkachel für eine Position – immer dieselbe für dieselbe Stelle.
    pub fn floor_at(&self, room: GridPos, tile: GridPos) -> Handle<Image> {
        // Raumposition in den Seed mischen: Jeder Raum hat sein eigenes Muster.
        let room_seed = (room.x as u32).wrapping_mul(31) ^ (room.y as u32).wrapping_mul(977);
        let variant = pixel_art::tiles::floor_variant(tile.x, tile.y, room_seed);
        self.floors[variant as usize].clone()
    }
}

/// Bild, weiße Treffer-Silhouette und Anzeigegröße einer Figur.
#[derive(Clone)]
pub struct ActorArt {
    pub image: Handle<Image>,
    pub flash: Handle<Image>,
    pub size: Vec2,
}

pub struct ActorSprites {
    pub player: ActorArt,
    pub chaser: ActorArt,
    pub shooter: ActorArt,
    pub charger: ActorArt,
    pub boss: ActorArt,
    pub tear: ActorArt,
    pub enemy_shot: ActorArt,
}

fn actor(images: &mut Assets<Image>, rows: &[&str], palette: Palette, scale: f32) -> ActorArt {
    ActorArt {
        image: images.add(pixel_art::image_from_ascii(rows, palette)),
        flash: images.add(pixel_art::silhouette_image(rows)),
        // Kleine Figuren leicht größer als ihre Hitbox: wirkt fairer, weil
        // Treffer „knapp daneben“ nicht zählen.
        size: pixel_art::display_size(rows) * scale / pixel_art::PIXEL_SCALE,
    }
}

/// `FromWorld` statt `Default`: Wir brauchen Zugriff auf andere Ressourcen
/// (`Assets<Image>`, `Assets<Mesh>` …), um die Handles zu erzeugen.
impl FromWorld for GameAssets {
    fn from_world(world: &mut World) -> Self {
        // Jeder Block leiht genau einen Asset-Speicher mutable aus und gibt ihn
        // am Blockende zurück – zwei `resource_mut` gleichzeitig ginge nicht.
        let explosion_mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Circle::new(crate::bomb::BLAST_RADIUS_TILES * TILE_SIZE));
        let explosion_material = world
            .resource_mut::<Assets<ColorMaterial>>()
            .add(Color::srgb(1.0, 0.65, 0.20));
        let font = world.resource::<AssetServer>().load("fonts/DejaVuSans.ttf");

        let mut images = world.resource_mut::<Assets<Image>>();
        let images = &mut *images;
        use pixel_art::{tiles, *};

        let sprites = Sprites {
            heart_full: images.add(heart_full()),
            heart_half: images.add(heart_half()),
            heart_empty: images.add(heart_empty()),
            coin: images.add(image_from_ascii(COIN, COIN_PALETTE)),
            key: images.add(image_from_ascii(KEY, KEY_PALETTE)),
            bomb: images.add(image_from_ascii(BOMB, BOMB_PALETTE)),
            item: images.add(image_from_ascii(ITEM, ITEM_PALETTE)),
            trapdoor: images.add(image_from_ascii(TRAPDOOR, TRAPDOOR_PALETTE)),
            exit: images.add(image_from_ascii(TRAPDOOR, EXIT_PALETTE)),
            pixel: images.add(white_pixel()),
        };

        let tiles = TileArt {
            floors: (0..tiles::FLOOR_VARIANTS)
                .map(|v| images.add(image_from_canvas(tiles::floor(v))))
                .collect(),
            wall: images.add(image_from_canvas(tiles::wall())),
            rock: images.add(image_from_canvas(tiles::rock())),
            pit: images.add(image_from_canvas(tiles::pit())),
            door_open: images.add(image_from_canvas(tiles::door_open())),
            door_closed: images.add(image_from_canvas(tiles::door_closed())),
            door_keyed: images.add(image_from_canvas(tiles::door_keyed())),
        };

        let actors = ActorSprites {
            player: actor(images, PLAYER, PLAYER_PALETTE, 2.0),
            chaser: actor(images, CHASER, CHASER_PALETTE, 2.2),
            shooter: actor(images, SHOOTER, SHOOTER_PALETTE, 2.2),
            charger: actor(images, CHARGER, CHARGER_PALETTE, 2.2),
            // 20 Bildpixel × 2,7 ≈ 54 Weltpixel – passend zur Boss-Hitbox (≈ 51).
            boss: actor(images, BOSS, BOSS_PALETTE, 2.7),
            tear: actor(images, SHOT, TEAR_PALETTE, 2.0),
            enemy_shot: actor(images, SHOT, ENEMY_SHOT_PALETTE, 2.0),
        };

        Self {
            font,
            explosion_mesh,
            explosion_material,
            sprites,
            tiles,
            actors,
        }
    }
}
