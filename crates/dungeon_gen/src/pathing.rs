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

/// Zusatzkosten für das Betreten einer Stachel-Kachel: so viel wie vier
/// gerade Schritte. Gegner machen also einen Umweg, solange er kurz ist –
/// und laufen trotzdem über Stacheln, wenn es keinen anderen Weg gibt.
pub const SPIKE_PENALTY: u32 = 4 * COST_STRAIGHT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowField {
    target: GridPos,
    /// `None` = nicht erreichbar oder nicht begehbar.
    dist: Vec<Option<u32>>,
    /// `None` = nicht begehbar, `Some(extra)` = begehbar; `extra` kommt beim
    /// Betreten der Kachel zu den Schrittkosten dazu (0 = normaler Boden).
    entry_cost: Vec<Option<u32>>,
}

fn index(pos: GridPos) -> Option<usize> {
    let inside = (0..ROOM_WIDTH).contains(&pos.x) && (0..ROOM_HEIGHT).contains(&pos.y);
    inside.then(|| (pos.y * ROOM_WIDTH + pos.x) as usize)
}

impl FlowField {
    /// Dijkstra von `target` aus über alle Kacheln, für die `walkable` gilt.
    /// Alle Kacheln kosten gleich viel.
    pub fn compute(target: GridPos, walkable: impl Fn(GridPos) -> bool) -> Self {
        // `bool::then_some`: true → Some(0), false → None.
        Self::compute_weighted(target, |p| walkable(p).then_some(0))
    }

    /// Wie [`compute`](Self::compute), aber jede Kachel darf Zusatzkosten
    /// haben: `entry_cost` liefert `None` für „nicht begehbar“ und sonst die
    /// Kosten, die beim Betreten dazukommen (z. B. [`SPIKE_PENALTY`]).
    pub fn compute_weighted(target: GridPos, entry_cost: impl Fn(GridPos) -> Option<u32>) -> Self {
        let size = (ROOM_WIDTH * ROOM_HEIGHT) as usize;
        let entry_cost: Vec<Option<u32>> = (0..size)
            .map(|i| {
                let pos = GridPos::new(i as i32 % ROOM_WIDTH, i as i32 / ROOM_WIDTH);
                entry_cost(pos)
            })
            .collect();
        let mut field = Self {
            target,
            dist: vec![None; size],
            entry_cost,
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
            // Die Suche läuft vom Ziel weg, ein Gegner aber zum Ziel hin: Er
            // geht von `next` nach `pos` und betritt dabei `pos`. Deshalb
            // zählen hier die Zusatzkosten von `pos`, nicht die von `next`.
            let enter = field.entry_cost_at(pos);
            // Nur `field.entry_cost` wird hier geliehen, nicht ganz `field`. Deshalb
            // dürfen wir im Schleifenrumpf gleichzeitig `field.dist` verändern
            // („disjoint field borrows“). Mit `field.passable_neighbors(pos)`
            // lehnt der Borrow-Checker das ab.
            for (next, step) in passable_neighbors(&field.entry_cost, pos) {
                let nd = d + step + enter;
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

    /// Zusatzkosten beim Betreten von `pos` (0 für normalen Boden und für
    /// alles, was gar nicht begehbar ist).
    fn entry_cost_at(&self, pos: GridPos) -> u32 {
        index(pos).and_then(|i| self.entry_cost[i]).unwrap_or(0)
    }

    /// Nächstes Feld auf dem günstigsten Weg zum Ziel, oder `None`, wenn `pos`
    /// das Ziel ist oder keinen Weg hat.
    ///
    /// Verglichen werden die **Gesamtkosten über den Nachbarn** (seine
    /// Entfernung + Schritt + Zusatzkosten). Nur die Entfernung zu vergleichen
    /// wäre falsch: Eine Stachel-Kachel kann näher am Ziel liegen und
    /// trotzdem der teurere Weg sein.
    pub fn next_step(&self, pos: GridPos) -> Option<GridPos> {
        let here = self.distance(pos)?;
        passable_neighbors(&self.entry_cost, pos)
            .filter_map(|(n, step)| {
                let d = self.distance(n)?;
                // `d < here` garantiert Fortschritt – so endet jeder Weg am Ziel.
                (d < here).then_some((d + step + self.entry_cost_at(n), n))
            })
            .min()
            .map(|(_, n)| n)
    }
}

/// Begehbare Nachbarn mit Schrittkosten. Diagonal nur, wenn beide
/// angrenzenden geraden Felder frei sind – sonst würde man durch die
/// Ecke eines Felsens „schneiden“.
fn passable_neighbors(
    entry_cost: &[Option<u32>],
    pos: GridPos,
) -> impl Iterator<Item = (GridPos, u32)> + '_ {
    let is_walkable = move |p: GridPos| index(p).is_some_and(|i| entry_cost[i].is_some());
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

/// Was es einen Läufer kostet, `pos` zu betreten (`None` = geht nicht).
pub fn walker_cost(layout: &RoomLayout, pos: GridPos) -> Option<u32> {
    if layout.blocks_movement(pos) {
        None
    } else if layout.is_hazard(pos) {
        Some(SPIKE_PENALTY)
    } else {
        Some(0)
    }
}

/// Bequemlichkeit: Flowfield für Läufer in einem Layout (meidet Stacheln).
pub fn walker_field(layout: &RoomLayout, target: GridPos) -> FlowField {
    FlowField::compute_weighted(target, |p| walker_cost(layout, p))
}

/// Flowfield für Flieger: Gruben und Stacheln sind ihnen egal.
pub fn flyer_field(layout: &RoomLayout, target: GridPos) -> FlowField {
    FlowField::compute(target, |p| !layout.blocks_flight(p))
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

    /// Folgt dem Feld von `start` bis zum Ziel und gibt alle betretenen Kacheln zurück.
    fn walk(field: &FlowField, start: GridPos) -> Vec<GridPos> {
        let mut path = Vec::new();
        let mut pos = start;
        while let Some(next) = field.next_step(pos) {
            path.push(next);
            pos = next;
            assert!(path.len() < 200, "Endlosschleife");
        }
        path
    }

    #[test]
    fn walkers_detour_around_spikes() {
        let l = layout(
            "
            ###############
            #.............#
            #.............#
            #.............#
            #......^......#
            #.............#
            #.............#
            #.............#
            ###############
        ",
        );
        let spikes = GridPos::new(7, 4);
        let target = GridPos::new(9, 4);
        let start = GridPos::new(5, 4);
        let field = walker_field(&l, target);
        // Geradeaus wären es 4 Schritte + Strafe = 40; der Bogen kostet nur 24.
        assert_eq!(
            field.distance(start),
            Some(2 * COST_STRAIGHT + 2 * COST_DIAGONAL)
        );
        let path = walk(&field, start);
        assert_eq!(path.last(), Some(&target));
        assert!(!path.contains(&spikes), "Weg führt über Stacheln: {path:?}");
        // Flieger nehmen die gerade Linie.
        assert_eq!(
            flyer_field(&l, target).distance(start),
            Some(4 * COST_STRAIGHT)
        );
    }

    #[test]
    fn walkers_cross_spikes_when_there_is_no_other_way() {
        let l = layout(
            "
            ###############
            #......^......#
            #......^......#
            #......^......#
            #......^......#
            #......^......#
            #......^......#
            #......^......#
            ###############
        ",
        );
        let target = GridPos::new(9, 4);
        let start = GridPos::new(5, 4);
        let field = walker_field(&l, target);
        assert_eq!(
            field.distance(start),
            Some(4 * COST_STRAIGHT + SPIKE_PENALTY)
        );
        let path = walk(&field, start);
        assert_eq!(path.last(), Some(&target));
        let on_spikes = path.iter().filter(|&&p| l.is_hazard(p)).count();
        assert_eq!(on_spikes, 1, "genau eine Stachel-Kachel: {path:?}");
    }

    #[test]
    fn flyers_cross_pits_but_not_rocks_or_barrels() {
        let l = layout(
            "
            ###############
            #.............#
            #.....___.....#
            #....._._.....#
            #.....___.....#
            #.............#
            #..ooo...bbb..#
            #..o.o...b.b..#
            ###############
        ",
        );
        let target = GridPos::new(1, 4);
        let ground = walker_field(&l, target);
        let air = flyer_field(&l, target);
        // Insel in der Grube: nur aus der Luft erreichbar.
        assert_eq!(ground.distance(GridPos::new(7, 5)), None);
        assert!(air.distance(GridPos::new(7, 5)).is_some());
        // Von Felsen bzw. Fässern (und der Wand) eingeschlossen: für niemanden.
        for cell in [GridPos::new(4, 1), GridPos::new(10, 1)] {
            assert_eq!(ground.distance(cell), None, "{cell:?}");
            assert_eq!(air.distance(cell), None, "{cell:?}");
        }
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
