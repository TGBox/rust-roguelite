//! Etagen-Generator im Stil von *The Binding of Isaac*.
//!
//! Ablauf:
//! 1. Zielanzahl Räume abhängig von der Etagentiefe bestimmen.
//! 2. Vom Startraum in der Mitte aus per Breitensuche wachsen. Ein neuer Raum
//!    darf nur entstehen, wenn er genau **einen** belegten Nachbarn hat –
//!    dadurch entsteht ein Baum (keine Schleifen, viele Sackgassen).
//! 3. Sackgassen werden zu Spezialräumen: Boss = am weitesten entfernte,
//!    Schatz und Shop = zufällige andere, dazu mit etwas Glück eine
//!    Herausforderung und ein Opferraum.
//! 4. Ein **Geheimraum** hängt versteckt an einem normalen Raum. Er hat genau
//!    einen Nachbarn – der Baum bleibt also ein Baum. Seine Tür ist im Spiel
//!    eine rissige Wand, die man freisprengen muss.
//! 5. Klappt etwas nicht (zu wenige Räume/Sackgassen), neu versuchen – mit
//!    demselben, weiterlaufenden Generator, also weiterhin deterministisch.
//!
//! Alle Sammlungen sind `BTreeMap`/`BTreeSet`: Ihre Iterationsreihenfolge ist
//! festgelegt. Bei `HashMap` wäre sie zufällig pro Programmstart – und derselbe
//! Seed würde plötzlich verschiedene Etagen erzeugen.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    Direction, GridPos, RoomLayout,
    rng::{Rng, RunSeed},
    templates::{self, RoomTemplate},
};

/// Kantenlänge des Etagenrasters (in Räumen).
pub const FLOOR_SIZE: i32 = 13;
/// Startraum in der Mitte.
pub const START_POS: GridPos = GridPos::new(FLOOR_SIZE / 2, FLOOR_SIZE / 2);
pub const MAX_ROOMS: usize = 20;
/// Wahrscheinlichkeit, einen möglichen Nachbarraum auszulassen.
const SKIP_CHANCE: f64 = 0.5;
const MAX_ATTEMPTS: u32 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RoomKind {
    Start,
    Normal,
    Boss,
    Treasure,
    Shop,
    /// Mehrere Gegnerwellen, danach ein Item.
    Challenge,
    /// Altar: Leben gegen Belohnungen tauschen.
    Sacrifice,
    /// Versteckt – nur per Bombe erreichbar.
    Secret,
}

impl RoomKind {
    pub const ALL: [RoomKind; 8] = [
        RoomKind::Start,
        RoomKind::Normal,
        RoomKind::Boss,
        RoomKind::Treasure,
        RoomKind::Shop,
        RoomKind::Challenge,
        RoomKind::Sacrifice,
        RoomKind::Secret,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomInfo {
    pub kind: RoomKind,
    /// Anzahl Raumwechsel vom Startraum aus.
    pub distance: u32,
    pub template: &'static RoomTemplate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Floor {
    /// 1 = erste Etage.
    pub depth: u32,
    rooms: BTreeMap<GridPos, RoomInfo>,
}

impl Floor {
    pub fn start(&self) -> GridPos {
        START_POS
    }

    pub fn get(&self, pos: GridPos) -> Option<&RoomInfo> {
        self.rooms.get(&pos)
    }

    pub fn rooms(&self) -> impl Iterator<Item = (GridPos, &RoomInfo)> {
        self.rooms.iter().map(|(p, r)| (*p, r))
    }

    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// Richtungen, in denen ein Nachbarraum liegt (= wo Türen hingehören).
    pub fn doors(&self, pos: GridPos) -> impl Iterator<Item = Direction> {
        pos.neighbors()
            .filter(|(_, n)| self.rooms.contains_key(n))
            .map(|(d, _)| d)
    }

    /// Fertiges Kachel-Layout eines Raums: Vorlage plus Türen zu allen Nachbarn.
    pub fn room_layout(&self, pos: GridPos) -> Option<RoomLayout> {
        let room = self.get(pos)?;
        Some(room.template.layout().with_doors(self.doors(pos)))
    }

    /// Liegt an `pos` in Richtung `dir` der versteckte Geheimraum?
    pub fn is_secret_door(&self, pos: GridPos, dir: Direction) -> bool {
        self.get(pos.neighbor(dir))
            .is_some_and(|r| r.kind == RoomKind::Secret)
    }

    /// Erster Raum dieser Art (Spezialräume gibt es je genau einmal).
    pub fn find(&self, kind: RoomKind) -> Option<GridPos> {
        self.rooms().find(|(_, r)| r.kind == kind).map(|(p, _)| p)
    }

    /// Übersichtskarte, oberste Reihe zuerst.
    /// `S` Start, `B` Boss, `T` Schatz, `$` Shop, `C` Herausforderung,
    /// `X` Opferraum, `?` Geheimraum, `#` normal, `.` leer.
    pub fn to_ascii(&self) -> String {
        let mut out = String::new();
        for y in (0..FLOOR_SIZE).rev() {
            for x in 0..FLOOR_SIZE {
                let c = match self.get(GridPos::new(x, y)).map(|r| r.kind) {
                    None => '.',
                    Some(RoomKind::Start) => 'S',
                    Some(RoomKind::Normal) => '#',
                    Some(RoomKind::Boss) => 'B',
                    Some(RoomKind::Treasure) => 'T',
                    Some(RoomKind::Shop) => '$',
                    Some(RoomKind::Challenge) => 'C',
                    Some(RoomKind::Sacrifice) => 'X',
                    Some(RoomKind::Secret) => '?',
                };
                out.push(c);
            }
            out.push('\n');
        }
        out
    }
}

/// Erzeugt die Etage `depth` (ab 1) für diesen Seed. Immer dasselbe Ergebnis
/// für dieselben Eingaben.
pub fn generate(seed: RunSeed, depth: u32) -> Floor {
    let mut rng = seed.stream("floor", u64::from(depth));
    let target = target_room_count(depth, &mut rng);

    for _ in 0..MAX_ATTEMPTS {
        if let Some(floor) = try_generate(&mut rng, depth, target) {
            return floor;
        }
    }
    // Die Property-Tests zeigen, dass das praktisch nie passiert.
    panic!("Keine gültige Etage nach {MAX_ATTEMPTS} Versuchen (Seed {seed}, Tiefe {depth})");
}

/// Wie bei Isaac: wächst mit der Tiefe, begrenzt auf `MAX_ROOMS`.
pub fn target_room_count(depth: u32, rng: &mut Rng) -> usize {
    let base = (depth * 10 / 3) as i32;
    ((base + rng.range(5..=6)) as usize).min(MAX_ROOMS)
}

fn in_bounds(p: GridPos) -> bool {
    (0..FLOOR_SIZE).contains(&p.x) && (0..FLOOR_SIZE).contains(&p.y)
}

fn occupied_neighbors(occupied: &BTreeSet<GridPos>, p: GridPos) -> usize {
    p.neighbors().filter(|(_, n)| occupied.contains(n)).count()
}

fn try_generate(rng: &mut Rng, depth: u32, target: usize) -> Option<Floor> {
    // --- 1. Layout wachsen lassen ---
    let mut occupied = BTreeSet::from([START_POS]);
    let mut queue = VecDeque::from([START_POS]);

    while let Some(current) = queue.pop_front() {
        for (_, next) in current.neighbors() {
            if occupied.len() >= target {
                break;
            }
            if !in_bounds(next) || occupied.contains(&next) {
                continue;
            }
            // Genau ein Nachbar (nämlich `current`) – sonst entstünde eine Schleife.
            if occupied_neighbors(&occupied, next) > 1 {
                continue;
            }
            if rng.chance(SKIP_CHANCE) {
                continue;
            }
            occupied.insert(next);
            queue.push_back(next);
        }
    }
    if occupied.len() != target {
        return None;
    }

    // --- 2. Entfernungen vom Start (Breitensuche) ---
    let distance = distances_from(&occupied, START_POS);

    // --- 3. Sackgassen zu Spezialräumen machen ---
    let mut dead_ends: Vec<GridPos> = occupied
        .iter()
        .copied()
        .filter(|&p| p != START_POS && occupied_neighbors(&occupied, p) == 1)
        .collect();
    if dead_ends.len() < 3 {
        return None;
    }

    // Stabile Sortierung: Bei gleicher Entfernung bleibt die BTree-Reihenfolge,
    // die Wahl ist also deterministisch.
    dead_ends.sort_by_key(|p| std::cmp::Reverse(distance[p]));
    let boss = dead_ends.remove(0);
    // Der Boss soll nicht direkt neben dem Start liegen.
    if distance[&boss] < 2 {
        return None;
    }
    rng.shuffle(&mut dead_ends);
    let (treasure, shop) = (dead_ends[0], dead_ends[1]);
    // Weitere Sackgassen werden mit etwas Glück zu optionalen Spezialräumen.
    // `get(2)` statt `[2]`: Es gibt nicht immer so viele Sackgassen.
    let challenge = dead_ends.get(2).copied().filter(|_| rng.chance(0.6));
    let sacrifice = dead_ends.get(3).copied().filter(|_| rng.chance(0.5));

    // --- 4. Räume mit Art und Vorlage befüllen ---
    let rooms = occupied
        .iter()
        .map(|&pos| {
            let kind = match pos {
                p if p == START_POS => RoomKind::Start,
                p if p == boss => RoomKind::Boss,
                p if p == treasure => RoomKind::Treasure,
                p if p == shop => RoomKind::Shop,
                p if Some(p) == challenge => RoomKind::Challenge,
                p if Some(p) == sacrifice => RoomKind::Sacrifice,
                _ => RoomKind::Normal,
            };
            let template = rng
                .choose(templates::pool(kind))
                .expect("jeder Raumtyp hat mindestens eine Vorlage");
            let info = RoomInfo {
                kind,
                distance: distance[&pos],
                template,
            };
            (pos, info)
        })
        .collect();

    let mut floor = Floor { depth, rooms };
    place_secret_room(&mut floor, rng);
    Some(floor)
}

/// Sucht eine freie Stelle, die an genau einen *normalen* Raum grenzt, und
/// legt dort den Geheimraum an. Gibt es keine, hat die Etage eben keinen.
fn place_secret_room(floor: &mut Floor, rng: &mut Rng) {
    let candidates: Vec<(GridPos, u32)> = (0..FLOOR_SIZE)
        .flat_map(|y| (0..FLOOR_SIZE).map(move |x| GridPos::new(x, y)))
        .filter(|&p| floor.get(p).is_none())
        .filter_map(|p| {
            let mut neighbors = p.neighbors().filter_map(|(_, n)| floor.get(n));
            let first = neighbors.next()?;
            // Genau ein Nachbar, und der ist ein normaler Raum.
            (neighbors.next().is_none() && first.kind == RoomKind::Normal)
                .then_some((p, first.distance + 1))
        })
        .collect();
    let Some(&(pos, distance)) = rng.choose(&candidates) else {
        return;
    };
    let template = rng
        .choose(templates::pool(RoomKind::Secret))
        .expect("Geheimraum-Vorlage fehlt");
    floor.rooms.insert(
        pos,
        RoomInfo {
            kind: RoomKind::Secret,
            distance,
            template,
        },
    );
}

fn distances_from(occupied: &BTreeSet<GridPos>, start: GridPos) -> BTreeMap<GridPos, u32> {
    let mut distance = BTreeMap::from([(start, 0)]);
    let mut queue = VecDeque::from([start]);
    while let Some(p) = queue.pop_front() {
        let d = distance[&p];
        for (_, n) in p.neighbors() {
            if occupied.contains(&n) && !distance.contains_key(&n) {
                distance.insert(n, d + 1);
                queue.push_back(n);
            }
        }
    }
    distance
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wie viele Seeds die Eigenschaftstests durchprobieren.
    const SEEDS: u64 = 10_000;
    const DEPTHS: [u32; 4] = [1, 2, 3, 6];

    /// Führt `check` für viele Seed/Tiefe-Kombinationen aus – ein einfacher
    /// „Property-Test“ ohne externe Crate.
    fn for_many_floors(mut check: impl FnMut(RunSeed, &Floor)) {
        for s in 0..SEEDS {
            let seed = RunSeed(s.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            for depth in DEPTHS {
                check(seed, &generate(seed, depth));
            }
        }
    }

    #[test]
    fn same_seed_same_floor() {
        let seed = RunSeed(12_345);
        assert_eq!(generate(seed, 1), generate(seed, 1));
        assert_eq!(generate(seed, 3), generate(seed, 3));
    }

    #[test]
    fn different_seeds_usually_differ() {
        let distinct: BTreeSet<String> = (0..100)
            .map(|s| generate(RunSeed(s), 1).to_ascii())
            .collect();
        assert!(
            distinct.len() > 90,
            "nur {} verschiedene Etagen",
            distinct.len()
        );
    }

    #[test]
    fn room_count_matches_depth() {
        for_many_floors(|seed, floor| {
            let base = (floor.depth * 10 / 3) as usize;
            let expected = (base + 5).min(MAX_ROOMS)..=(base + 6).min(MAX_ROOMS);
            // Der Geheimraum kommt zusätzlich zur Zielanzahl dazu.
            let visible = floor
                .rooms()
                .filter(|(_, r)| r.kind != RoomKind::Secret)
                .count();
            assert!(
                expected.contains(&visible),
                "Seed {seed}, Tiefe {}: {visible} Räume",
                floor.depth,
            );
        });
    }

    #[test]
    fn all_rooms_reachable_and_in_bounds() {
        for_many_floors(|seed, floor| {
            let occupied: BTreeSet<GridPos> = floor.rooms().map(|(p, _)| p).collect();
            assert!(occupied.iter().all(|&p| in_bounds(p)), "Seed {seed}");
            let reached = distances_from(&occupied, floor.start());
            assert_eq!(
                reached.len(),
                occupied.len(),
                "Seed {seed}: nicht zusammenhängend"
            );
        });
    }

    #[test]
    fn layout_is_a_tree() {
        // Ein zusammenhängender Graph mit n Knoten ist genau dann ein Baum,
        // wenn er n - 1 Kanten hat.
        for_many_floors(|seed, floor| {
            let edges: usize = floor
                .rooms()
                .map(|(p, _)| floor.doors(p).count())
                .sum::<usize>()
                / 2;
            assert_eq!(edges, floor.len() - 1, "Seed {seed}: Schleife gefunden");
        });
    }

    #[test]
    fn special_rooms_exist_once_and_are_dead_ends() {
        for_many_floors(|seed, floor| {
            for kind in [RoomKind::Boss, RoomKind::Treasure, RoomKind::Shop] {
                let count = floor.rooms().filter(|(_, r)| r.kind == kind).count();
                assert_eq!(count, 1, "Seed {seed}: {kind:?} kommt {count}-mal vor");
                let pos = floor.find(kind).unwrap();
                assert_eq!(
                    floor.doors(pos).count(),
                    1,
                    "Seed {seed}: {kind:?} keine Sackgasse"
                );
            }
            assert_eq!(
                floor.get(floor.start()).map(|r| r.kind),
                Some(RoomKind::Start)
            );
        });
    }

    #[test]
    fn boss_is_farthest_dead_end_and_not_next_to_start() {
        for_many_floors(|seed, floor| {
            let boss = floor.get(floor.find(RoomKind::Boss).unwrap()).unwrap();
            assert!(boss.distance >= 2, "Seed {seed}: Boss direkt neben Start");
            let farthest_dead_end = floor
                .rooms()
                .filter(|(p, r)| {
                    !matches!(r.kind, RoomKind::Start | RoomKind::Secret)
                        && floor.doors(*p).count() == 1
                })
                .map(|(_, r)| r.distance)
                .max()
                .unwrap();
            assert_eq!(boss.distance, farthest_dead_end, "Seed {seed}");
        });
    }

    #[test]
    fn optional_rooms_are_unique_dead_ends() {
        let mut secrets = 0;
        let mut challenges = 0;
        for_many_floors(|seed, floor| {
            for kind in [RoomKind::Challenge, RoomKind::Sacrifice, RoomKind::Secret] {
                let rooms: Vec<_> = floor.rooms().filter(|(_, r)| r.kind == kind).collect();
                assert!(rooms.len() <= 1, "Seed {seed}: {kind:?} mehrfach");
                for (pos, _) in rooms {
                    assert_eq!(floor.doors(pos).count(), 1, "Seed {seed}: {kind:?}");
                }
            }
            if let Some(secret) = floor.find(RoomKind::Secret) {
                secrets += 1;
                let dir = floor.doors(secret).next().unwrap();
                let host = floor.get(secret.neighbor(dir)).unwrap();
                assert_eq!(host.kind, RoomKind::Normal, "Seed {seed}");
                assert!(floor.is_secret_door(secret.neighbor(dir), dir.opposite()));
            }
            if floor.find(RoomKind::Challenge).is_some() {
                challenges += 1;
            }
        });
        let total = (SEEDS * DEPTHS.len() as u64) as usize;
        // Fast jede Etage hat einen Geheimraum, Herausforderungen gibt es oft.
        assert!(secrets > total * 9 / 10, "{secrets}/{total}");
        assert!(challenges > total / 4, "{challenges}/{total}");
    }

    #[test]
    fn doors_match_neighbors_on_both_sides() {
        use crate::{Tile, room::door_pos};
        for s in 0..500 {
            let floor = generate(RunSeed(s), 2);
            for (pos, _) in floor.rooms() {
                let layout = floor.room_layout(pos).unwrap();
                for dir in Direction::ALL {
                    let has_neighbor = floor.get(pos.neighbor(dir)).is_some();
                    let has_door = layout.get(door_pos(dir)) == Some(Tile::Door);
                    assert_eq!(has_neighbor, has_door, "Raum {pos:?}, {dir:?}");
                    if has_door {
                        // Gegenüber muss die passende Tür sein.
                        let other = floor.room_layout(pos.neighbor(dir)).unwrap();
                        assert_eq!(other.get(door_pos(dir.opposite())), Some(Tile::Door));
                    }
                }
            }
        }
    }

    #[test]
    fn templates_match_room_kind() {
        for_many_floors(|_, floor| {
            for (_, room) in floor.rooms() {
                assert!(templates::pool(room.kind).contains(room.template));
            }
        });
    }
}
