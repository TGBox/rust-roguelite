// cspell:disable
//! Pixel-Grafiken, komplett im Code erzeugt – keine Bilddateien nötig.
//!
//! Aufteilung:
//! - `canvas`: Pixelpuffer, Rauschen, Farbhilfen (reines Rust, testbar)
//! - `maps`:   handgezeichnete ASCII-Karten und Paletten
//! - `tiles`:  prozedurale Kacheln (Boden, Mauer, Türen …)
//! - hier:     Umwandlung in Bevy-`Image`s
//!
//! Moderne Modul-Aufteilung: `pixel_art.rs` ist die Wurzel, die Untermodule
//! liegen im Ordner `pixel_art/` (statt einer `pixel_art/mod.rs`).
//!
//! Dargestellt werden die Bilder vergrößert. Dank `ImagePlugin::default_nearest()`
//! (siehe `main.rs`) bleiben die Pixel dabei scharf.

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

pub mod canvas;
pub mod maps;
pub mod tiles;

pub use canvas::{Canvas, Palette, rgba_from_ascii, silhouette_from_ascii};
pub use maps::*;

/// Vergrößerung beim Anzeigen: 1 Bildpixel = 2 Weltpixel.
pub const PIXEL_SCALE: f32 = 2.0;

/// RGBA-Bytes → Bevy-Bild. `Rgba8UnormSrgb`: die Bytes sind sRGB-Farben,
/// genau wie in einem Malprogramm.
pub fn image_from_rgba(width: u32, height: u32, data: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

pub fn image_from_ascii(rows: &[&str], palette: Palette) -> Image {
    let (w, h, data) = rgba_from_ascii(rows, palette);
    image_from_rgba(w, h, data)
}

pub fn silhouette_image(rows: &[&str]) -> Image {
    let (w, h, data) = silhouette_from_ascii(rows);
    image_from_rgba(w, h, data)
}

pub fn image_from_canvas(canvas: Canvas) -> Image {
    image_from_rgba(canvas.width, canvas.height, canvas.data)
}

/// Anzeigegröße eines Bildes (Bildpixel × `PIXEL_SCALE`).
pub fn display_size(rows: &[&str]) -> Vec2 {
    let w = canvas::ascii_width(rows);
    Vec2::new(w as f32, rows.len() as f32) * PIXEL_SCALE
}

pub fn heart_full() -> Image {
    image_from_ascii(HEART, HEART_FULL)
}

pub fn heart_empty() -> Image {
    image_from_ascii(HEART, HEART_EMPTY)
}

/// Linke Hälfte voll, rechte leer: die leere Karte wird spaltenweise übermalt.
pub fn heart_half() -> Image {
    let (w, h, empty) = rgba_from_ascii(HEART, HEART_EMPTY);
    let mut canvas = Canvas::new(w, h, canvas::TRANSPARENT);
    canvas.data = empty;
    let mut full = Canvas::new(w, h, canvas::TRANSPARENT);
    full.draw_ascii(HEART, HEART_FULL, 0, 0);
    for y in 0..h as i32 {
        for x in 0..=(w as i32 / 2) {
            canvas.set(x, y, full.get(x, y));
        }
    }
    image_from_canvas(canvas)
}

/// Ein einzelnes weißes Pixel – Grundlage für Partikel (per `Sprite::color` eingefärbt).
pub fn white_pixel() -> Image {
    image_from_rgba(1, 1, vec![255, 255, 255, 255])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sprites_are_rectangular_and_fully_mapped() {
        // `rgba_from_ascii` panict bei Fehlern – der Test ruft es nur für alle auf.
        let sprites: &[(&[&str], Palette)] = &[
            (TRAPDOOR, TRAPDOOR_PALETTE),
            (TRAPDOOR, EXIT_PALETTE),
            (HEART, HEART_FULL),
            (HEART, HEART_EMPTY),
            (COIN, COIN_PALETTE),
            (KEY, KEY_PALETTE),
            (BOMB, BOMB_PALETTE),
            (ITEM, ITEM_PALETTE),
            (PLAYER, PLAYER_PALETTE),
            (CHASER, CHASER_PALETTE),
            (SHOOTER, SHOOTER_PALETTE),
            (CHARGER, CHARGER_PALETTE),
            (BOSS, BOSS_PALETTE),
            (SHOT, TEAR_PALETTE),
            (SHOT, ENEMY_SHOT_PALETTE),
            (ROCK, ROCK_PALETTE),
            (KEYHOLE, KEYHOLE_PALETTE),
            (HOPPER, HOPPER_PALETTE),
            (SPLITTER, SPLITTER_PALETTE),
            (SPLITLING, SPLITTER_PALETTE),
            (SUMMONER, SUMMONER_PALETTE),
            (BROOD_MOTHER, BROOD_MOTHER_PALETTE),
            (WARDEN, WARDEN_PALETTE),
            (ALTAR, ALTAR_PALETTE),
            (ITEM, ACTIVE_ITEM_PALETTE),
        ];
        for (rows, palette) in sprites {
            let (w, h, data) = rgba_from_ascii(rows, palette);
            assert_eq!(data.len(), (w * h * 4) as usize);
        }
    }
}
