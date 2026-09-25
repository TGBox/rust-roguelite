//! Wegfindung und Sichtlinie innerhalb eines Raums.
//!
//! **Flowfield** statt A* pro Gegner: Ein einziger Dijkstra-Lauf vom Spieler
//! aus berechnet für *jede* Kachel die Entfernung zum Spieler. Jeder Gegner
//! geht dann einfach zum Nachbarfeld mit der kleinsten Entfernung. Das kostet
//! gleich viel, egal ob 1 oder 50 Gegner im Raum sind.

use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{
    GridPos, RoomLayout,
    room::{ROOM_HEIGHT, ROOM_WIDTH},
};

/// Kosten für einen geraden bzw. diagonalen Schritt. 5:7 ≈ 1:√2 – ganzzahlig,
/// damit die Ergebnisse auf jeder Plattform exakt gleich sind.
pub const COST_STRAIGHT: u32 = 5;
pub const COST_DIAGONAL: u32 = 7;

const NEIGHBORS: [(i32, i32); 8] = [
    (0, 1),
    (1, 0),
    (0, -1),
    (-1, 0),
    (1, 1),
    (1, -1),
    (-1, -1),
    (-1, 1),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowField {
    target: GridPos,
    /// `None` = nicht erreichbar oder nicht begehbar.
    dist: Vec<Option<u32>>,
    walkable: Vec<bool>,
}

fn index(pos: GridPos) -> Option<usize> {
    let inside = (0..ROOM_WIDTH).contains(&pos.x) && (0..ROOM_HEIGHT).contains(&pos.y);
    inside.then(|| (pos.y * ROOM_WIDTH + pos.x) as usize)
}

impl FlowField {
    /// Dijkstra von `target` aus über alle Kacheln, für die `walkable` gilt.
    pub fn compute(target: GridPos, walkable: impl Fn(GridPos) -> bool) -> Self {
        let size = (ROOM_WIDTH * ROOM_HEIGHT) as usize;
        let walkable: Vec<bool> = (0..size)
            .map(|i| {
                let pos = GridPos::new(i as i32 % ROOM_WIDTH, i as i32 / ROOM_WIDTH);
                walkable(pos)
            })
            .collect();
        let mut field = Self {
            target,
            dist: vec![None; size],
            walkable,
        };

        let Some(start) = index(target) else {
            return field;
        };
        field.dist[start] = Some(0);

        // `BinaryHeap` ist ein Max-Heap. `Reverse` dreht die Ordnung um, damit
        // immer das Feld mit der KLEINSTEN Entfernung zuerst herauskommt.
        // Das `GridPos` im Tupel macht die Reihenfolge bei Gleichstand eindeutig.
        let mut heap = BinaryHeap::from([Reverse((0u32, target))]);
        while let Some(Reverse((d, pos))) = heap.pop() {
            if field.distance(pos).is_some_and(|best| d > best) {
                continue; // veralteter Eintrag
            }
            // Nur `field.walkable` wird hier geliehen, nicht ganz `field`. Deshalb
            // dürfen wir im Schleifenrumpf gleichzeitig `field.dist` verändern
            // („disjoint field borrows“). Mit `field.passable_neighbors(pos)`
            // lehnt der Borrow-Checker das ab.
            for (next, cost) in passable_neighbors(&field.walkable, pos) {
                let nd = d + cost;
                let i = index(next).expect("Nachbar liegt im Raum");
                if field.dist[i].is_none_or(|old| nd < old) {
                    field.dist[i] = Some(nd);
                    heap.push(Reverse((nd, next)));
                }
            }
        }
        field
    }

    pub fn target(&self) -> GridPos {
        self.target
    }

    pub fn distance(&self, pos: GridPos) -> Option<u32> {
        index(pos).and_then(|i| self.dist[i])
    }

    /// Nächstes Feld auf dem kürzesten Weg zum Ziel, oder `None`, wenn `pos`
    /// das Ziel ist oder keinen Weg hat.
    pub fn next_step(&self, pos: GridPos) -> Option<GridPos> {
        let here = self.distance(pos)?;
        passable_neighbors(&self.walkable, pos)
            .filter_map(|(n, _)| self.distance(n).map(|d| (d, n)))
            .filter(|&(d, _)| d < here)
            .min()
            .map(|(_, n)| n)
    }
}

/// Begehbare Nachbarn mit Schrittkosten. Diagonal nur, wenn beide
/// angrenzenden geraden Felder frei sind – sonst würde man durch die
/// Ecke eines Felsens „schneiden“.
fn passable_neighbors(
    walkable: &[bool],
    pos: GridPos,
) -> impl Iterator<Item = (GridPos, u32)> + '_ {
    let is_walkable = move |p: GridPos| index(p).is_some_and(|i| walkable[i]);
    NEIGHBORS.into_iter().filter_map(move |(dx, dy)| {
        let next = pos + GridPos::new(dx, dy);
        if !is_walkable(next) {
            return None;
        }
        if dx != 0 && dy != 0 {
            let side_a = pos + GridPos::new(dx, 0);
            let side_b = pos + GridPos::new(0, dy);
            if !is_walkable(side_a) || !is_walkable(side_b) {
                return None;
            }
            Some((next, COST_DIAGONAL))
        } else {
            Some((next, COST_STRAIGHT))
        }
    })
}

/// Freie Sichtlinie zwischen zwei Punkten im Kachelraum?
/// Tastet die Strecke in kleinen Schritten ab; `blocks` entscheidet, welche
/// Kacheln die Sicht versperren (z. B. Fels ja, Grube nein).
pub fn line_of_sight(from: [f32; 2], to: [f32; 2], blocks: impl Fn(GridPos) -> bool) -> bool {
    const STEP: f32 = 0.2;
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let len = (dx * dx + dy * dy).sqrt();
    let steps = (len / STEP).ceil() as u32;
    (0..=steps).all(|i| {
        let t = if steps == 0 {
            0.0
        } else {
            i as f32 / steps as f32
        };
        let x = from[0] + dx * t;
        let y = from[1] + dy * t;
        !blocks(GridPos::new(x.floor() as i32, y.floor() as i32))
    })
}

/// Bequemlichkeit: Flowfield für Läufer in einem Layout.
pub fn walker_field(layout: &RoomLayout, target: GridPos) -> FlowField {
    FlowField::compute(target, |p| !layout.blocks_movement(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{room::CENTER, templates};

    fn layout(ascii: &str) -> RoomLayout {
        RoomLayout::from_ascii(ascii).unwrap()
    }

    #[test]
    fn empty_room_distances() {
        let field = walker_field(&RoomLayout::empty(), CENTER);
        assert_eq!(field.distance(CENTER), Some(0));
        assert_eq!(
            field.distance(CENTER + GridPos::new(1, 0)),
            Some(COST_STRAIGHT)
        );
        assert_eq!(
            field.distance(CENTER + GridPos::new(1, 1)),
            Some(COST_DIAGONAL)
        );
        assert_eq!(
            field.distance(CENTER + GridPos::new(3, 1)),
            Some(2 * COST_STRAIGHT + COST_DIAGONAL)
        );
        assert_eq!(field.distance(GridPos::new(0, 0)), None, "Wand");
    }

    #[test]
    fn following_the_field_reaches_the_target() {
        for t in templates::pool(crate::RoomKind::Normal) {
            let l = t.layout();
            let target = GridPos::new(1, 1);
            let field = walker_field(&l, target);
            for (start, _) in l.iter().filter(|&(p, _)| field.distance(p).is_some()) {
                let mut pos = start;
                let mut steps = 0;
                while let Some(next) = field.next_step(pos) {
                    assert!(!l.blocks_movement(next), "{}: Schritt in Hindernis", t.name);
                    pos = next;
                    steps += 1;
                    assert!(steps < 200, "{}: Endlosschleife", t.name);
                }
                assert_eq!(pos, target, "{}: Start {start:?} kommt nicht an", t.name);
            }
        }
    }

    #[test]
    fn goes_around_obstacles() {
        let l = layout(
            "
            ###############
            #.............#
            #......o......#
            #......o......#
            #......o......#
            #......o......#
            #......o......#
            #.............#
            ###############
        ",
        );
        let field = walker_field(&l, GridPos::new(9, 4));
        // Direkt links der Mauer: Weg führt um die Mauer herum, also deutlich
        // länger als die Luftlinie von 3 Feldern.
        let d = field.distance(GridPos::new(6, 4)).unwrap();
        assert!(d > 3 * COST_STRAIGHT * 2, "Weg zu kurz: {d}");
    }

    #[test]
    fn never_cuts_corners() {
        let l = layout(
            "
            ###############
            #.............#
            #.............#
            #.............#
            #......o......#
            #.............#
            #.............#
            #.............#
            ###############
        ",
        );
        let field = walker_field(&l, GridPos::new(8, 5));
        // Von (6,4) nach (7,5) wäre diagonal – aber (7,4) ist Fels.
        let d = field.distance(GridPos::new(6, 4)).unwrap();
        assert!(d > COST_DIAGONAL + COST_STRAIGHT, "Ecke geschnitten: {d}");
    }

    #[test]
    fn pit_island_is_unreachable() {
        let l = layout(
            "
            ###############
            #.............#
            #.....___.....#
            #....._._.....#
            #.....___.....#
            #.............#
            #.............#
            #.............#
            ###############
        ",
        );
        let field = walker_field(&l, GridPos::new(1, 1));
        assert_eq!(field.distance(GridPos::new(7, 5)), None);
        assert_eq!(field.next_step(GridPos::new(7, 5)), None);
    }

    #[test]
    fn line_of_sight_cases() {
        let rock = |p: GridPos| p == GridPos::new(5, 5);
        assert!(line_of_sight([1.5, 5.5], [3.5, 5.5], rock));
        assert!(!line_of_sight([1.5, 5.5], [8.5, 5.5], rock));
        assert!(line_of_sight([1.5, 1.5], [8.5, 1.5], rock));
        assert!(
            line_of_sight([2.5, 2.5], [2.5, 2.5], rock),
            "gleicher Punkt"
        );
    }

    #[test]
    fn deterministic() {
        let l = templates::pool(crate::RoomKind::Normal)[1].layout();
        assert_eq!(walker_field(&l, CENTER), walker_field(&l, CENTER));
    }
}
