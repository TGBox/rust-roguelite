//! Ganzzahlige Rasterkoordinaten.
//!
//! Wird doppelt genutzt: für Kacheln innerhalb eines Raums und für Räume
//! innerhalb einer Etage. `y` wächst nach oben (wie in Bevy), damit beim
//! Umrechnen in Weltkoordinaten nichts gespiegelt werden muss.

use std::ops::{Add, Sub};

/// Position in einem Raster. `Copy`, weil nur 8 Byte groß: wird überall
/// per Wert herumgereicht, ohne Borrowing-Aufwand.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct GridPos {
    pub x: i32,
    pub y: i32,
}

impl GridPos {
    pub const ZERO: Self = Self::new(0, 0);

    /// `const fn`, damit auch Konstanten wie `ZERO` damit gebaut werden können.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Nachbarfeld in der gegebenen Richtung.
    pub fn neighbor(self, dir: Direction) -> Self {
        self + dir.offset()
    }

    /// Alle vier orthogonalen Nachbarn (ohne Diagonalen).
    ///
    /// Gibt einen Iterator statt `Vec` zurück: keine Heap-Allokation.
    pub fn neighbors(self) -> impl Iterator<Item = (Direction, GridPos)> {
        Direction::ALL
            .into_iter()
            .map(move |d| (d, self.neighbor(d)))
    }

    /// Manhattan-Distanz: Anzahl orthogonaler Schritte zwischen zwei Feldern.
    pub fn manhattan(self, other: Self) -> u32 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }
}

impl Add for GridPos {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for GridPos {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

/// Die vier Himmelsrichtungen – später auch für Türen eines Raums.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub const ALL: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];

    pub const fn offset(self) -> GridPos {
        match self {
            Direction::North => GridPos::new(0, 1),
            Direction::East => GridPos::new(1, 0),
            Direction::South => GridPos::new(0, -1),
            Direction::West => GridPos::new(-1, 0),
        }
    }

    pub const fn opposite(self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::East => Direction::West,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbor_and_opposite_cancel_out() {
        let start = GridPos::new(3, -2);
        for dir in Direction::ALL {
            assert_eq!(start.neighbor(dir).neighbor(dir.opposite()), start);
        }
    }

    #[test]
    fn neighbors_are_all_distance_one() {
        let p = GridPos::new(5, 5);
        let n: Vec<_> = p.neighbors().collect();
        assert_eq!(n.len(), 4);
        assert!(n.iter().all(|&(_, q)| p.manhattan(q) == 1));
    }

    #[test]
    fn manhattan_is_symmetric() {
        let a = GridPos::new(-4, 7);
        let b = GridPos::new(2, -1);
        assert_eq!(a.manhattan(b), 14);
        assert_eq!(a.manhattan(b), b.manhattan(a));
    }
}
