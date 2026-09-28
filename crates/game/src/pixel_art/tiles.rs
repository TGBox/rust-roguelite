//! Prozedurale Kachel-Texturen (16 × 16 Pixel). Reines Rust, keine Bevy-Typen.
//!
//! Statt jede Kachel von Hand zu zeichnen, beschreibt eine Funktion, welche
//! Farbe Pixel (x, y) bekommt. Das Rauschen aus `canvas::noise` sorgt für
//! lebendige, aber reproduzierbare Oberflächen.

use super::canvas::{Canvas, jitter, mix, noise, shade};
use super::maps::{KEYHOLE, KEYHOLE_PALETTE, ROCK, ROCK_PALETTE};

/// Kantenlänge einer Kachel in Bildpixeln (× `PIXEL_SCALE` = `TILE_SIZE`).
pub const TILE_PIXELS: u32 = 16;
/// So viele Bodenvarianten gibt es; die Auswahl hängt von der Kachelposition ab.
pub const FLOOR_VARIANTS: u32 = 4;

const FLOOR: [u8; 4] = [46, 40, 37, 255];
const GROUT: [u8; 4] = [31, 27, 26, 255];
const BRICK: [u8; 4] = [96, 82, 75, 255];
const MORTAR: [u8; 4] = [38, 32, 31, 255];
const WOOD: [u8; 4] = [118, 76, 42, 255];
const WOOD_DARK: [u8; 4] = [72, 45, 25, 255];
const IRON: [u8; 4] = [70, 70, 78, 255];
const GOLD: [u8; 4] = [210, 170, 55, 255];

/// Steinplatten: leicht verrauschte Fläche, dunkle Fuge unten und rechts.
/// Variante 1 bekommt einen Riss, Variante 2 Moos, Variante 3 Kiesel.
pub fn floor(variant: u32) -> Canvas {
    let seed = 100 + variant;
    let mut c = Canvas::new(TILE_PIXELS, TILE_PIXELS, FLOOR);
    let last = TILE_PIXELS as i32 - 1;
    c.paint(|x, y| {
        if x == last || y == last {
            return shade(GROUT, jitter(x, y, seed, 2));
        }
        shade(FLOOR, jitter(x, y, seed, 3))
    });
    match variant {
        1 => {
            // Riss: ein kleiner Zickzack-Pfad.
            let (mut x, mut y) = (3, 4);
            for step in 0..9 {
                c.set(x, y, GROUT);
                x += 1;
                if noise(step, 0, seed).is_multiple_of(2) {
                    y += 1;
                }
            }
        }
        2 => {
            for i in 0..7 {
                let x = (noise(i, 1, seed) % 14) as i32;
                let y = (noise(i, 2, seed) % 14) as i32;
                c.set(x, y, [58, 72, 44, 255]);
            }
        }
        3 => {
            for i in 0..4 {
                let x = 1 + (noise(i, 3, seed) % 12) as i32;
                let y = 1 + (noise(i, 4, seed) % 12) as i32;
                c.set(x, y, shade(FLOOR, 22));
                c.set(x + 1, y + 1, shade(FLOOR, -12));
            }
        }
        _ => {}
    }
    c
}

/// Ziegelmauer: 4 Pixel hohe Reihen, jede zweite um einen halben Stein versetzt.
pub fn wall() -> Canvas {
    let mut c = Canvas::new(TILE_PIXELS, TILE_PIXELS, BRICK);
    c.paint(|x, y| {
        let row = y / 4;
        let offset = if row % 2 == 0 { 0 } else { 4 };
        let column = (x + offset) / 8;
        if y % 4 == 3 || (x + offset) % 8 == 7 {
            return MORTAR;
        }
        // Jeder Stein bekommt einen eigenen Grundton, jede Oberkante etwas Licht.
        let tone = jitter(column, row, 7, 8);
        let light = if y % 4 == 0 { 12 } else { 0 };
        shade(BRICK, tone + light + jitter(x, y, 8, 3))
    });
    c
}

/// Rissige Mauer: Hier liegt ein Geheimraum. Eine Bombe sprengt sie auf.
/// Der Riss ist deutlich, aber nicht grell – aufmerksame Spieler sehen ihn.
pub fn wall_cracked() -> Canvas {
    let mut c = wall();
    // Zwei Risse von der Mitte aus, als feste Pixelpfade.
    let crack = [
        (7, 2),
        (7, 3),
        (8, 4),
        (8, 5),
        (7, 6),
        (7, 7),
        (8, 8),
        (9, 9),
        (9, 10),
        (8, 11),
        (8, 12),
        (6, 7),
        (5, 8),
        (4, 8),
        (10, 9),
        (11, 10),
    ];
    for (x, y) in crack {
        c.set(x, y, [14, 11, 11, 255]);
        // Helle Kante daneben lässt den Riss „tief“ wirken.
        c.set(x + 1, y, shade(c.get(x + 1, y), 18));
    }
    c
}

/// Fels: Bodenkachel mit Brocken darauf.
pub fn rock() -> Canvas {
    let mut c = floor(0);
    c.draw_ascii_centered(ROCK, ROCK_PALETTE);
    c
}

/// Grube: fast schwarz, zur Mitte hin dunkler.
pub fn pit() -> Canvas {
    let mut c = Canvas::new(TILE_PIXELS, TILE_PIXELS, [0, 0, 0, 255]);
    let half = TILE_PIXELS as f32 / 2.0;
    c.paint(|x, y| {
        let dx = (x as f32 + 0.5 - half).abs();
        let dy = (y as f32 + 0.5 - half).abs();
        let edge = dx.max(dy) / half; // 0 = Mitte, 1 = Rand
        let base = mix([4, 3, 6, 255], [22, 18, 22, 255], edge * edge);
        shade(base, jitter(x, y, 9, 1))
    });
    c
}

/// Offene Tür: dunkler Durchgang mit Holzrahmen.
pub fn door_open() -> Canvas {
    let mut c = door_frame();
    c.paint_inner(|x, y| shade([24, 20, 19, 255], jitter(x, y, 11, 2)));
    c
}

/// Geschlossene Tür: Holzbretter mit Eisenbändern.
pub fn door_closed() -> Canvas {
    let mut c = door_frame();
    c.paint_inner(|x, y| {
        if y == 5 || y == 10 {
            IRON
        } else if x % 4 == 1 {
            WOOD_DARK
        } else {
            shade(WOOD, jitter(x, y, 12, 5))
        }
    });
    c
}

/// Verschlossene Tür: goldene Beschläge und Schlüsselloch.
pub fn door_keyed() -> Canvas {
    let mut c = door_closed();
    c.paint_inner(|x, y| {
        if y == 5 || y == 10 {
            GOLD
        } else if x % 4 == 1 {
            WOOD_DARK
        } else {
            shade(WOOD, jitter(x, y, 12, 5))
        }
    });
    c.draw_ascii_centered(KEYHOLE, KEYHOLE_PALETTE);
    c
}

/// Rahmen aus dunklem Holz; der Inhalt wird von `paint_inner` gefüllt.
/// Symmetrisch, damit die Tür in jeder Wand (oben, unten, links, rechts) passt.
fn door_frame() -> Canvas {
    let mut c = Canvas::new(TILE_PIXELS, TILE_PIXELS, WOOD_DARK);
    c.paint(|x, y| {
        let last = TILE_PIXELS as i32 - 1;
        if x == 0 || y == 0 || x == last || y == last {
            MORTAR
        } else {
            shade(WOOD_DARK, jitter(x, y, 10, 4))
        }
    });
    c
}

impl Canvas {
    /// Wie `paint`, aber nur innerhalb eines 2-Pixel-Rahmens.
    fn paint_inner(&mut self, mut f: impl FnMut(i32, i32) -> [u8; 4]) {
        let (w, h) = (self.width as i32, self.height as i32);
        for y in 2..h - 2 {
            for x in 2..w - 2 {
                let color = f(x, y);
                self.set(x, y, color);
            }
        }
    }
}

/// Welche Bodenvariante an (x, y) liegt – deterministisch, damit ein Raum
/// beim Zurückkommen gleich aussieht. Variante 0 ist die häufigste.
pub fn floor_variant(x: i32, y: i32, room_seed: u32) -> u32 {
    match noise(x, y, room_seed) % 10 {
        0..=5 => 0,
        6 | 7 => 3,
        8 => 1,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tiles_have_tile_size_and_are_opaque() {
        let mut tiles = vec![
            wall(),
            rock(),
            pit(),
            door_open(),
            door_closed(),
            door_keyed(),
        ];
        tiles.extend((0..FLOOR_VARIANTS).map(floor));
        for t in tiles {
            assert_eq!((t.width, t.height), (TILE_PIXELS, TILE_PIXELS));
            // Jedes vierte Byte ist Alpha: Kacheln dürfen keine Löcher haben.
            assert!(t.data.chunks(4).all(|p| p[3] == 255));
        }
    }

    #[test]
    fn floor_variants_differ_and_cover_all() {
        assert_ne!(floor(0), floor(1));
        let used: std::collections::BTreeSet<_> =
            (0..200).map(|i| floor_variant(i, i / 3, 42)).collect();
        assert_eq!(used.len(), FLOOR_VARIANTS as usize);
    }
}
