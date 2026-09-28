// cspell:disable
//! Reines Rust ohne Bevy: eine kleine „Leinwand“ aus RGBA-Pixeln.
//!
//! Alles hier ist bewusst engine-unabhängig. So lassen sich die Grafiken
//! in Tests prüfen (oder als Bild ausgeben), ohne ein Fenster zu öffnen.

/// Eine Palette ordnet jedem ASCII-Zeichen eine Farbe zu.
pub type Palette = &'static [(char, [u8; 4])];

pub const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];

/// Wandelt eine ASCII-Karte in RGBA-Bytes um (Zeile für Zeile, oben zuerst).
///
/// # Panics
/// Bei unterschiedlich langen Zeilen oder Zeichen, die nicht in der Palette
/// stehen – beides sind Programmierfehler, die sofort auffallen sollen.
pub fn rgba_from_ascii(rows: &[&str], palette: Palette) -> (u32, u32, Vec<u8>) {
    let mut canvas = Canvas::new(ascii_width(rows), rows.len() as u32, TRANSPARENT);
    canvas.draw_ascii(rows, palette, 0, 0);
    (canvas.width, canvas.height, canvas.data)
}

/// Gleiche Form, aber jedes sichtbare Pixel weiß: für das Aufblitzen bei Treffern.
/// (Ein Sprite-`color` wird nur *multipliziert* – weißer als die Vorlage geht damit nicht.)
pub fn silhouette_from_ascii(rows: &[&str]) -> (u32, u32, Vec<u8>) {
    let width = ascii_width(rows);
    let mut data = Vec::with_capacity((width as usize) * rows.len() * 4);
    for row in rows {
        for c in row.chars() {
            data.extend_from_slice(&if c == '.' {
                TRANSPARENT
            } else {
                [255, 255, 255, 255]
            });
        }
    }
    (width, rows.len() as u32, data)
}

pub fn ascii_width(rows: &[&str]) -> u32 {
    rows.first().map_or(0, |r| r.chars().count()) as u32
}

/// Ein Pixelpuffer mit Zeichenfunktionen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// RGBA, Zeile für Zeile, oben links zuerst.
    pub data: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32, fill: [u8; 4]) -> Self {
        Self {
            width,
            height,
            data: fill.repeat((width * height) as usize),
        }
    }

    /// Byte-Index des Pixels – `None` außerhalb der Leinwand.
    /// Mit `Option` statt Panic dürfen Zeichenfunktionen über den Rand malen.
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let inside = x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height;
        inside.then(|| ((y as u32 * self.width + x as u32) * 4) as usize)
    }

    pub fn get(&self, x: i32, y: i32) -> [u8; 4] {
        self.index(x, y).map_or(TRANSPARENT, |i| {
            // `try_into` macht aus dem Slice ein Array fester Länge.
            self.data[i..i + 4].try_into().unwrap()
        })
    }

    pub fn set(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if let Some(i) = self.index(x, y) {
            self.data[i..i + 4].copy_from_slice(&color);
        }
    }

    /// Alle Pixel mit einer Funktion von (x, y) füllen.
    pub fn paint(&mut self, mut f: impl FnMut(i32, i32) -> [u8; 4]) {
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let c = f(x, y);
                self.set(x, y, c);
            }
        }
    }

    /// ASCII-Karte an (`ox`, `oy`) darüber zeichnen; `.` lässt den Hintergrund stehen.
    ///
    /// # Panics
    /// Wie [`rgba_from_ascii`].
    pub fn draw_ascii(&mut self, rows: &[&str], palette: Palette, ox: i32, oy: i32) {
        let width = ascii_width(rows) as usize;
        for (y, row) in rows.iter().enumerate() {
            assert_eq!(row.chars().count(), width, "Zeile {y} hat falsche Länge");
            for (x, c) in row.chars().enumerate() {
                if c == '.' {
                    continue;
                }
                let color = palette
                    .iter()
                    .find(|(k, _)| *k == c)
                    .unwrap_or_else(|| panic!("Zeichen '{c}' fehlt in der Palette"))
                    .1;
                self.set(ox + x as i32, oy + y as i32, color);
            }
        }
    }

    /// Mittig zeichnen (für Felsen auf Bodenkacheln u. Ä.).
    pub fn draw_ascii_centered(&mut self, rows: &[&str], palette: Palette) {
        let ox = (self.width as i32 - ascii_width(rows) as i32) / 2;
        let oy = (self.height as i32 - rows.len() as i32) / 2;
        self.draw_ascii(rows, palette, ox, oy);
    }
}

/// Heller/dunkler machen, ohne über 0..=255 hinauszulaufen.
pub fn shade(color: [u8; 4], delta: i32) -> [u8; 4] {
    let f = |c: u8| (c as i32 + delta).clamp(0, 255) as u8;
    [f(color[0]), f(color[1]), f(color[2]), color[3]]
}

/// Linear zwischen zwei Farben mischen (`t` = 0.0 … 1.0).
pub fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    let f = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    [f(0), f(1), f(2), f(3)]
}

/// Ganzzahliges „Rauschen“: gleiche Eingabe → gleiche Ausgabe, aber gut
/// durchmischt. Damit sehen Kacheln zufällig aus und sind trotzdem
/// bei jedem Start identisch (kein RNG-Zustand nötig).
pub fn noise(x: i32, y: i32, seed: u32) -> u32 {
    // „lowbias32“-Mischfunktion: wenige Operationen, sehr gleichmäßige Bits.
    let mut h = (x as u32)
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add((y as u32).wrapping_mul(0x85EB_CA77))
        ^ seed.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    h
}

/// Rauschen als Helligkeitsversatz im Bereich `-amount..=amount`.
pub fn jitter(x: i32, y: i32, seed: u32, amount: i32) -> i32 {
    (noise(x, y, seed) % (2 * amount as u32 + 1)) as i32 - amount
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_is_transparent() {
        let (_, _, data) = rgba_from_ascii(&[".K"], &[('K', [1, 2, 3, 255])]);
        assert_eq!(&data[0..4], &TRANSPARENT);
        assert_eq!(&data[4..8], &[1, 2, 3, 255]);
    }

    #[test]
    fn silhouette_keeps_shape() {
        let (w, h, data) = silhouette_from_ascii(&[".K", "K."]);
        assert_eq!((w, h), (2, 2));
        assert_eq!(data[3], 0);
        assert_eq!(&data[4..8], &[255; 4]);
    }

    #[test]
    fn drawing_outside_is_ignored() {
        let mut c = Canvas::new(2, 2, TRANSPARENT);
        c.set(-1, 0, [9; 4]);
        c.set(5, 5, [9; 4]);
        assert!(c.data.iter().all(|&b| b == 0));
        assert_eq!(c.get(-1, 0), TRANSPARENT);
    }

    #[test]
    fn noise_is_deterministic_and_varied() {
        assert_eq!(noise(3, 4, 7), noise(3, 4, 7));
        let distinct: std::collections::BTreeSet<_> =
            (0..64).map(|i| noise(i, 0, 1) % 16).collect();
        assert!(distinct.len() > 10, "Rauschen zu gleichförmig");
        for i in 0..200 {
            let j = jitter(i, i * 3, 5, 4);
            assert!((-4..=4).contains(&j));
        }
    }

    #[test]
    fn shade_and_mix_clamp() {
        assert_eq!(shade([250, 5, 100, 255], 10), [255, 15, 110, 255]);
        assert_eq!(shade([3, 5, 100, 7], -10), [0, 0, 90, 7]);
        assert_eq!(
            mix([0, 0, 0, 0], [200, 100, 50, 255], 0.5),
            [100, 50, 25, 128]
        );
    }
}
