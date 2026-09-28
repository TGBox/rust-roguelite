// cspell:disable
// Die ASCII-Pixelkarten sind keine Wörter – Rechtschreibprüfung hier aus.

//! Kleine Pixel-Grafiken, direkt im Code als ASCII-Karten.
//!
//! Ein Zeichen = ein Pixel, `.` = durchsichtig. Die Palette ordnet jedem
//! Zeichen eine Farbe zu. So brauchen wir bis M8 keine Bilddateien und
//! behalten trotzdem gut unterscheidbare Symbole.
//!
//! Dargestellt werden die Bilder vergrößert. Dank `ImagePlugin::default_nearest()`
//! (siehe `main.rs`) bleiben die Pixel dabei scharf.

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

/// Vergrößerung beim Anzeigen: 1 Bildpixel = 2 Weltpixel.
pub const PIXEL_SCALE: f32 = 2.0;

type Palette = &'static [(char, [u8; 4])];

/// Wandelt eine ASCII-Karte in RGBA-Bytes um (Zeile für Zeile, oben zuerst).
///
/// # Panics
/// Bei unterschiedlich langen Zeilen oder Zeichen, die nicht in der Palette
/// stehen – beides sind Programmierfehler, die sofort auffallen sollen.
pub fn rgba_from_ascii(rows: &[&str], palette: Palette) -> (u32, u32, Vec<u8>) {
    let width = rows.first().map_or(0, |r| r.chars().count());
    let mut data = Vec::with_capacity(width * rows.len() * 4);
    for (y, row) in rows.iter().enumerate() {
        assert_eq!(row.chars().count(), width, "Zeile {y} hat falsche Länge");
        for c in row.chars() {
            let rgba = if c == '.' {
                [0, 0, 0, 0]
            } else {
                palette
                    .iter()
                    .find(|(k, _)| *k == c)
                    .unwrap_or_else(|| panic!("Zeichen '{c}' fehlt in der Palette"))
                    .1
            };
            data.extend_from_slice(&rgba);
        }
    }
    (width as u32, rows.len() as u32, data)
}

pub fn image_from_ascii(rows: &[&str], palette: Palette) -> Image {
    let (width, height, data) = rgba_from_ascii(rows, palette);
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

/// Anzeigegröße eines Bildes (Bildpixel × `PIXEL_SCALE`).
pub fn display_size(rows: &[&str]) -> Vec2 {
    let w = rows.first().map_or(0, |r| r.chars().count());
    Vec2::new(w as f32, rows.len() as f32) * PIXEL_SCALE
}

// --- Farben -----------------------------------------------------------------

const OUTLINE: [u8; 4] = [18, 14, 16, 255];
const WHITE: [u8; 4] = [255, 250, 240, 255];

// --- Herz --------------------------------------------------------------------

pub const HEART: &[&str] = &[
    "..KKK.KKK..",
    ".KRRRKRRRK.",
    "KRWRRRRRRRK",
    "KRWRRRRRRRK",
    "KRRRRRRRRRK",
    ".KRRRRRRRK.",
    "..KRRRRRK..",
    "...KRRRK...",
    "....KRK....",
    ".....K.....",
];

const HEART_FULL: Palette = &[('K', OUTLINE), ('R', [220, 30, 45, 255]), ('W', WHITE)];
/// Gleiche Karte, aber Füllung dunkel: ein leeres Herz.
const HEART_EMPTY: Palette = &[
    ('K', OUTLINE),
    ('R', [70, 25, 32, 255]),
    ('W', [95, 45, 52, 255]),
];

pub fn heart_full() -> Image {
    image_from_ascii(HEART, HEART_FULL)
}

pub fn heart_empty() -> Image {
    image_from_ascii(HEART, HEART_EMPTY)
}

/// Linke Hälfte voll, rechte leer. Die Karte wird dafür spaltenweise umgefärbt.
pub fn heart_half() -> Image {
    let (w, h, mut data) = rgba_from_ascii(HEART, HEART_FULL);
    let (_, _, empty) = rgba_from_ascii(HEART, HEART_EMPTY);
    let (w, h) = (w as usize, h as usize);
    for y in 0..h {
        for x in (w / 2 + 1)..w {
            let i = (y * w + x) * 4;
            data[i..i + 4].copy_from_slice(&empty[i..i + 4]);
        }
    }
    Image::new(
        Extent3d {
            width: w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

// --- Münze, Schlüssel, Bombe, Item ---------------------------------------------

pub const COIN: &[&str] = &[
    "..KKKK..", ".KYYYYK.", "KYWYYYYK", "KYWYYOYK", "KYYYYOYK", "KYYYYOYK", ".KYOOOK.", "..KKKK..",
];

pub const COIN_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('Y', [245, 200, 50, 255]),
    ('O', [185, 130, 20, 255]),
    ('W', WHITE),
];

pub const KEY: &[&str] = &[
    ".KKK.......",
    "KGGGKKKKKKK",
    "KG.GGGGGGGK",
    "KGGGKKKGKGK",
    ".KKK...K.K.",
];

pub const KEY_PALETTE: Palette = &[('K', OUTLINE), ('G', [205, 210, 225, 255])];

pub const BOMB: &[&str] = &[
    "......Y..",
    ".....O.Y.",
    "....K....",
    "..KKKKK..",
    ".KBBBBBK.",
    "KBWBBBBBK",
    "KBWBBBBBK",
    "KBBBBBBBK",
    ".KBBBBBK.",
    "..KKKKK..",
];

pub const BOMB_PALETTE: Palette = &[
    ('K', OUTLINE),
    // Mittelgrau statt Schwarz: sonst verschwindet die Bombe auf dem dunklen Boden.
    ('B', [100, 100, 118, 255]),
    ('W', [190, 190, 205, 255]),
    ('O', [240, 140, 30, 255]),
    ('Y', [255, 230, 90, 255]),
];

/// Falltür (Holzrahmen, dunkles Loch) – mit anderer Palette der goldene Ausgang.
pub const TRAPDOOR: &[&str] = &[
    ".KKKKKKKKKKKK.",
    "KWWWWWWWWWWWWK",
    "KWHHHHHHHHHHWK",
    "KWHHHHHHHHHHWK",
    "KWHHHHHHHHHHWK",
    "KWHHHHHHHHHHWK",
    "KWHHHHHHHHHHWK",
    "KWHHHHHHHHHHWK",
    "KWWWWWWWWWWWWK",
    ".KKKKKKKKKKKK.",
];

pub const TRAPDOOR_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('W', [125, 82, 45, 255]),
    ('H', [6, 5, 8, 255]),
];

pub const EXIT_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('W', [235, 195, 60, 255]),
    ('H', [255, 245, 190, 255]),
];

/// Edelstein über einem Steinsockel.
pub const ITEM: &[&str] = &[
    ".....KK.....",
    "....KCCK....",
    "...KCWCCK...",
    "..KCWCCCCK..",
    "...KCCCCK...",
    "....KCCK....",
    ".....KK.....",
    "............",
    ".KKKKKKKKKK.",
    ".KSSSSSSSSK.",
    "..KSSSSSSK..",
    "..KKKKKKKK..",
];

pub const ITEM_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('C', [80, 225, 235, 255]),
    ('W', WHITE),
    ('S', [140, 132, 125, 255]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sprites_are_rectangular_and_fully_mapped() {
        // `rgba_from_ascii` panict bei Fehlern – der Test ruft es nur für alle auf.
        let sprites: [(&[&str], Palette); 8] = [
            (TRAPDOOR, TRAPDOOR_PALETTE),
            (TRAPDOOR, EXIT_PALETTE),
            (HEART, HEART_FULL),
            (HEART, HEART_EMPTY),
            (COIN, COIN_PALETTE),
            (KEY, KEY_PALETTE),
            (BOMB, BOMB_PALETTE),
            (ITEM, ITEM_PALETTE),
        ];
        for (rows, palette) in sprites {
            let (w, h, data) = rgba_from_ascii(rows, palette);
            assert_eq!(data.len(), (w * h * 4) as usize);
        }
    }

    #[test]
    fn dot_is_transparent() {
        let (_, _, data) = rgba_from_ascii(&[".K"], &[('K', OUTLINE)]);
        assert_eq!(&data[0..4], &[0, 0, 0, 0]);
        assert_eq!(&data[4..8], &OUTLINE);
    }
}
