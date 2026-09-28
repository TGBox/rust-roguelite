//! Ein Raum: Maße und Kachel-Layout.
//!
//! Die Maße sind Spiellogik (Kollision, Pathfinding, Generierung) und
//! liegen deshalb hier. Wie groß eine Kachel in Pixeln ist, entscheidet
//! erst die Spiel-Crate.

use std::fmt;

use crate::{Direction, GridPos};

/// Gesamtbreite inkl. Wandring.
pub const ROOM_WIDTH: i32 = 15;
/// Gesamthöhe inkl. Wandring.
pub const ROOM_HEIGHT: i32 = 9;

/// Liegt die Kachel auf dem äußeren Ring des Raums?
pub fn is_border(pos: GridPos) -> bool {
    pos.x == 0 || pos.y == 0 || pos.x == ROOM_WIDTH - 1 || pos.y == ROOM_HEIGHT - 1
}

/// Position der Tür in der Mitte der jeweiligen Wand.
pub const fn door_pos(dir: Direction) -> GridPos {
    match dir {
        Direction::North => GridPos::new(ROOM_WIDTH / 2, ROOM_HEIGHT - 1),
        Direction::South => GridPos::new(ROOM_WIDTH / 2, 0),
        Direction::West => GridPos::new(0, ROOM_HEIGHT / 2),
        Direction::East => GridPos::new(ROOM_WIDTH - 1, ROOM_HEIGHT / 2),
    }
}

/// Die Bodenkachel direkt vor einer Tür (innen) – hier erscheint der Spieler.
pub fn inside_door(dir: Direction) -> GridPos {
    door_pos(dir) - dir.offset()
}

/// Ist `pos` eine Türposition? Wenn ja, in welcher Wand?
pub fn door_direction(pos: GridPos) -> Option<Direction> {
    Direction::ALL.into_iter().find(|&d| door_pos(d) == pos)
}

/// Mitte des Raums.
pub const CENTER: GridPos = GridPos::new(ROOM_WIDTH / 2, ROOM_HEIGHT / 2);

/// Alle Kachelpositionen des Raums, Zeile für Zeile von unten links.
pub fn all_tiles() -> impl Iterator<Item = GridPos> {
    (0..ROOM_HEIGHT).flat_map(|y| (0..ROOM_WIDTH).map(move |x| GridPos::new(x, y)))
}

/// Was auf einer Kachel liegt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tile {
    Floor,
    Wall,
    /// Hindernis: blockiert Laufen und Schüsse.
    Rock,
    /// Grube: blockiert Laufen, Schüsse fliegen darüber.
    Pit,
    /// Durchgang zum Nachbarraum. Offen begehbar; ob sie gerade verschlossen
    /// ist, entscheidet das Spiel (Zustand, nicht Layout).
    Door,
}

impl Tile {
    pub fn blocks_movement(self) -> bool {
        !matches!(self, Tile::Floor | Tile::Door)
    }

    /// Schüsse zerplatzen auch am Türrahmen.
    pub fn blocks_projectiles(self) -> bool {
        matches!(self, Tile::Wall | Tile::Rock | Tile::Door)
    }

    fn from_char(c: char) -> Option<Tile> {
        match c {
            '.' => Some(Tile::Floor),
            '#' => Some(Tile::Wall),
            'o' => Some(Tile::Rock),
            '_' => Some(Tile::Pit),
            'D' => Some(Tile::Door),
            _ => None,
        }
    }

    fn to_char(self) -> char {
        match self {
            Tile::Floor => '.',
            Tile::Wall => '#',
            Tile::Rock => 'o',
            Tile::Pit => '_',
            Tile::Door => 'D',
        }
    }
}

/// Das Kachel-Raster eines Raums.
///
/// Das Feld ist privat: So kann niemand von außen einen `Vec` mit falscher
/// Länge hineinschmuggeln. Die Invariante `tiles.len() == W * H` wird nur
/// von den Konstruktoren hergestellt und gilt danach immer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomLayout {
    /// Zeilenweise von unten links (Index = y * ROOM_WIDTH + x).
    tiles: Vec<Tile>,
}

impl RoomLayout {
    /// Leerer Raum: Wandring außen, Boden innen.
    pub fn empty() -> Self {
        let tiles = all_tiles()
            .map(|p| {
                if is_border(p) {
                    Tile::Wall
                } else {
                    Tile::Floor
                }
            })
            .collect();
        Self { tiles }
    }

    fn index(pos: GridPos) -> Option<usize> {
        let in_bounds = (0..ROOM_WIDTH).contains(&pos.x) && (0..ROOM_HEIGHT).contains(&pos.y);
        in_bounds.then(|| (pos.y * ROOM_WIDTH + pos.x) as usize)
    }

    /// `None` außerhalb des Raums.
    pub fn get(&self, pos: GridPos) -> Option<Tile> {
        Self::index(pos).map(|i| self.tiles[i])
    }

    /// Setzt eine Kachel. Gibt `false` zurück, wenn `pos` außerhalb liegt.
    pub fn set(&mut self, pos: GridPos, tile: Tile) -> bool {
        match Self::index(pos) {
            Some(i) => {
                self.tiles[i] = tile;
                true
            }
            None => false,
        }
    }

    /// Außerhalb des Raums gilt alles als blockiert – so kann nie etwas
    /// aus dem Raster „herausfallen“.
    pub fn blocks_movement(&self, pos: GridPos) -> bool {
        self.get(pos).is_none_or(Tile::blocks_movement)
    }

    pub fn blocks_projectiles(&self, pos: GridPos) -> bool {
        self.get(pos).is_none_or(Tile::blocks_projectiles)
    }

    /// Setzt Türen in die angegebenen Wände (Builder-Stil: nimmt `self` und gibt es zurück).
    pub fn with_doors(mut self, dirs: impl IntoIterator<Item = Direction>) -> Self {
        for dir in dirs {
            self.set(door_pos(dir), Tile::Door);
        }
        self
    }

    /// Sprengt alle Felsen im 3×3-Bereich um `center` weg (Wände und Türen
    /// bleiben). Gibt die zerstörten Kacheln zurück.
    pub fn blast(&mut self, center: GridPos) -> Vec<GridPos> {
        let mut destroyed = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                let p = center + GridPos::new(dx, dy);
                if self.get(p) == Some(Tile::Rock) {
                    self.set(p, Tile::Floor);
                    destroyed.push(p);
                }
            }
        }
        destroyed
    }

    /// Nächste Bodenkachel zu `target` (Breitensuche), z. B. für Beute, die
    /// nicht in einer Grube landen soll. `occupied` wird übersprungen.
    pub fn nearest_floor(&self, target: GridPos, occupied: &[GridPos]) -> Option<GridPos> {
        let mut seen = std::collections::BTreeSet::from([target]);
        let mut queue = std::collections::VecDeque::from([target]);
        while let Some(p) = queue.pop_front() {
            if self.get(p) == Some(Tile::Floor) && !occupied.contains(&p) {
                return Some(p);
            }
            for (_, n) in p.neighbors() {
                if self.get(n).is_some() && seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        None
    }

    pub fn iter(&self) -> impl Iterator<Item = (GridPos, Tile)> {
        all_tiles().zip(self.tiles.iter().copied())
    }

    /// Parst ein Layout aus ASCII-Art. Die **oberste** Textzeile ist die
    /// oberste Raumreihe (y = ROOM_HEIGHT - 1). Leerzeilen und Einrückung
    /// werden ignoriert, damit Layouts bequem als Raw-Strings im Code stehen können.
    ///
    /// Zeichen: `#` Wand, `.` Boden, `o` Fels, `_` Grube, `D` Tür (nur an Türpositionen).
    pub fn from_ascii(text: &str) -> Result<Self, LayoutError> {
        let rows: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();

        if rows.len() != ROOM_HEIGHT as usize {
            return Err(LayoutError::WrongRowCount {
                expected: ROOM_HEIGHT as usize,
                found: rows.len(),
            });
        }

        let mut layout = Self::empty();
        for (row, line) in rows.iter().enumerate() {
            let len = line.chars().count();
            if len != ROOM_WIDTH as usize {
                return Err(LayoutError::WrongRowLength {
                    row,
                    expected: ROOM_WIDTH as usize,
                    found: len,
                });
            }
            let y = ROOM_HEIGHT - 1 - row as i32;
            for (col, ch) in line.chars().enumerate() {
                let tile = Tile::from_char(ch).ok_or(LayoutError::UnknownChar { row, col, ch })?;
                let pos = GridPos::new(col as i32, y);
                let at_door = door_direction(pos).is_some();
                if tile == Tile::Door && !at_door {
                    return Err(LayoutError::MisplacedDoor { pos });
                }
                if is_border(pos) && !matches!(tile, Tile::Wall | Tile::Door) {
                    return Err(LayoutError::OpenBorder { pos });
                }
                layout.set(pos, tile);
            }
        }
        Ok(layout)
    }

    /// Gegenstück zu [`from_ascii`](Self::from_ascii) – praktisch für Debug-Ausgaben.
    pub fn to_ascii(&self) -> String {
        let mut out = String::with_capacity(((ROOM_WIDTH + 1) * ROOM_HEIGHT) as usize);
        for y in (0..ROOM_HEIGHT).rev() {
            for x in 0..ROOM_WIDTH {
                // unwrap ist hier sicher: (x, y) liegt garantiert im Raum.
                out.push(self.get(GridPos::new(x, y)).unwrap().to_char());
            }
            out.push('\n');
        }
        out
    }
}

/// Fehler beim Parsen eines Layouts.
///
/// Eigener Fehlertyp statt `String`: Aufrufer können gezielt matchen,
/// und Tests prüfen den genauen Fehler statt eines Textes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    WrongRowCount {
        expected: usize,
        found: usize,
    },
    WrongRowLength {
        row: usize,
        expected: usize,
        found: usize,
    },
    UnknownChar {
        row: usize,
        col: usize,
        ch: char,
    },
    /// Der Rand darf nur aus Wänden und Türen bestehen.
    OpenBorder {
        pos: GridPos,
    },
    /// Tür an einer Stelle, an der keine Tür sein darf.
    MisplacedDoor {
        pos: GridPos,
    },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::WrongRowCount { expected, found } => {
                write!(f, "erwartet {expected} Zeilen, gefunden {found}")
            }
            LayoutError::WrongRowLength {
                row,
                expected,
                found,
            } => write!(
                f,
                "Zeile {row}: erwartet {expected} Zeichen, gefunden {found}"
            ),
            LayoutError::UnknownChar { row, col, ch } => {
                write!(f, "Zeile {row}, Spalte {col}: unbekanntes Zeichen '{ch}'")
            }
            LayoutError::OpenBorder { pos } => {
                write!(f, "Rand bei ({}, {}) ist keine Wand", pos.x, pos.y)
            }
            LayoutError::MisplacedDoor { pos } => {
                write!(
                    f,
                    "Tür bei ({}, {}) liegt nicht in einer Wandmitte",
                    pos.x, pos.y
                )
            }
        }
    }
}

impl std::error::Error for LayoutError {}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "
        ###############
        #.............#
        #..oo.....__..#
        #..o......__..#
        #.............#
        #..o.......o..#
        #..oo.....ooo.#
        #.............#
        ###############
    ";

    #[test]
    fn floor_is_13_by_7() {
        let floor = all_tiles().filter(|&p| !is_border(p)).count();
        assert_eq!(floor, 13 * 7);
    }

    #[test]
    fn parses_sample_with_correct_orientation() {
        let layout = RoomLayout::from_ascii(SAMPLE).unwrap();
        // Zweite Textzeile von oben = y 7; dritte = y 6 mit Fels bei x 3.
        assert_eq!(layout.get(GridPos::new(3, 6)), Some(Tile::Rock));
        assert_eq!(layout.get(GridPos::new(10, 6)), Some(Tile::Pit));
        assert_eq!(layout.get(GridPos::new(7, 4)), Some(Tile::Floor));
        assert_eq!(layout.get(GridPos::new(0, 0)), Some(Tile::Wall));
    }

    #[test]
    fn ascii_roundtrip() {
        let layout = RoomLayout::from_ascii(SAMPLE).unwrap();
        let again = RoomLayout::from_ascii(&layout.to_ascii()).unwrap();
        assert_eq!(layout, again);
    }

    #[test]
    fn empty_equals_plain_ascii() {
        let text = RoomLayout::empty().to_ascii();
        assert_eq!(text.lines().next(), Some("###############"));
        assert_eq!(text.lines().nth(1), Some("#.............#"));
    }

    #[test]
    fn rejects_bad_layouts() {
        assert!(matches!(
            RoomLayout::from_ascii("#"),
            Err(LayoutError::WrongRowCount { found: 1, .. })
        ));

        let open = SAMPLE.replacen("#.............#", "..............#", 1);
        assert!(matches!(
            RoomLayout::from_ascii(&open),
            Err(LayoutError::OpenBorder { pos }) if pos == GridPos::new(0, 7)
        ));

        let unknown = SAMPLE.replacen("#..oo", "#..X.", 1);
        assert!(matches!(
            RoomLayout::from_ascii(&unknown),
            Err(LayoutError::UnknownChar { ch: 'X', .. })
        ));
    }

    #[test]
    fn outside_is_solid() {
        let layout = RoomLayout::empty();
        assert!(layout.blocks_movement(GridPos::new(-1, 3)));
        assert!(layout.blocks_projectiles(GridPos::new(ROOM_WIDTH, 0)));
    }

    #[test]
    fn doors_are_centered_and_entries_are_inside() {
        for dir in Direction::ALL {
            let door = door_pos(dir);
            assert!(is_border(door), "{dir:?}");
            assert!(!is_border(inside_door(dir)), "{dir:?}");
            assert_eq!(door.manhattan(inside_door(dir)), 1);
            assert_eq!(door_direction(door), Some(dir));
        }
        assert_eq!(door_direction(CENTER), None);
    }

    #[test]
    fn with_doors_sets_only_requested_doors() {
        let layout = RoomLayout::empty().with_doors([Direction::North, Direction::West]);
        let doors: Vec<GridPos> = layout
            .iter()
            .filter(|&(_, t)| t == Tile::Door)
            .map(|(p, _)| p)
            .collect();
        assert_eq!(doors.len(), 2);
        assert!(doors.contains(&door_pos(Direction::North)));
        assert!(doors.contains(&door_pos(Direction::West)));
        // Türen überleben den ASCII-Roundtrip.
        assert_eq!(RoomLayout::from_ascii(&layout.to_ascii()), Ok(layout));
    }

    #[test]
    fn rejects_misplaced_door() {
        let bad = SAMPLE.replacen("#..oo.....__..#", "#..oD.....__..#", 1);
        assert!(matches!(
            RoomLayout::from_ascii(&bad),
            Err(LayoutError::MisplacedDoor { .. })
        ));
    }

    #[test]
    fn doors_are_walkable_but_stop_shots() {
        assert!(!Tile::Door.blocks_movement());
        assert!(Tile::Door.blocks_projectiles());
    }

    #[test]
    fn blast_removes_only_nearby_rocks() {
        let mut layout = RoomLayout::from_ascii(SAMPLE).unwrap();
        // Felsen bei (3,6), (4,6), (3,5); Grube bei (10,6).
        let destroyed = layout.blast(GridPos::new(3, 5));
        assert_eq!(destroyed.len(), 3, "{destroyed:?}");
        assert_eq!(layout.get(GridPos::new(3, 6)), Some(Tile::Floor));
        assert_eq!(
            layout.get(GridPos::new(3, 2)),
            Some(Tile::Rock),
            "zu weit weg"
        );
        // Am Rand: Wände überleben.
        let destroyed = layout.blast(GridPos::new(1, 1));
        assert!(destroyed.is_empty());
        assert_eq!(layout.get(GridPos::new(0, 0)), Some(Tile::Wall));
    }

    #[test]
    fn nearest_floor_avoids_pits_and_occupied_tiles() {
        let layout = crate::templates::pool(crate::RoomKind::Normal)
            .iter()
            .find(|t| t.name == "pit_center")
            .unwrap()
            .layout();
        assert_eq!(layout.get(CENTER), Some(Tile::Pit));
        let p = layout.nearest_floor(CENTER, &[]).unwrap();
        assert_eq!(layout.get(p), Some(Tile::Floor));
        assert!(p.manhattan(CENTER) <= 3);
        let q = layout.nearest_floor(CENTER, &[p]).unwrap();
        assert_ne!(p, q);
        let empty = RoomLayout::empty();
        assert_eq!(empty.nearest_floor(CENTER, &[]), Some(CENTER));
    }

    #[test]
    fn pits_block_walking_but_not_shots() {
        assert!(Tile::Pit.blocks_movement());
        assert!(!Tile::Pit.blocks_projectiles());
        assert!(Tile::Rock.blocks_projectiles());
    }
}
