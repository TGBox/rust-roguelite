//! Welche Gegner wo in einem Raum erscheinen.
//!
//! Pro Raum ein eigener RNG-Stream: Egal in welcher Reihenfolge der Spieler
//! die Räume betritt, derselbe Raum bekommt immer dieselben Gegner.

use crate::{
    Direction, GridPos, RoomKind, RoomLayout, Tile,
    floor::FLOOR_SIZE,
    rng::{Rng, RunSeed},
    room::{CENTER, inside_door},
};

/// Mindestabstand (Manhattan) zwischen Spawnpunkt und Türeingang,
/// damit beim Betreten kein Gegner direkt neben dem Spieler steht.
pub const MIN_DOOR_DISTANCE: u32 = 4;

/// Gegnertypen. Werte und Verhalten legt die Spiel-Crate fest – hier geht es
/// nur darum, *welcher* Typ *wo* erscheint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EnemyKind {
    /// Läuft auf kürzestem Weg zum Spieler.
    Chaser,
    /// Hält Abstand und schießt.
    Shooter,
    /// Wartet, bis der Spieler in einer Linie steht, und sprintet dann los.
    Charger,
    Boss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spawn {
    pub pos: GridPos,
    pub kind: EnemyKind,
}

/// Normale Gegner mit relativer Häufigkeit.
const NORMAL_WEIGHTS: [(EnemyKind, u32); 3] = [
    (EnemyKind::Chaser, 50),
    (EnemyKind::Shooter, 25),
    (EnemyKind::Charger, 25),
];

/// Eigener Stream je Etage und Raum.
pub fn spawn_rng(seed: RunSeed, depth: u32, room: GridPos) -> Rng {
    let room_index = (room.y * FLOOR_SIZE + room.x) as u64;
    seed.stream("spawns", u64::from(depth) * 1_000 + room_index)
}

/// Gegner für einen Raum dieser Art.
/// Boss: genau einer in der Mitte. Normale Räume: 2–4 zufällige.
pub fn plan_spawns(kind: RoomKind, layout: &RoomLayout, rng: &mut Rng) -> Vec<Spawn> {
    match kind {
        RoomKind::Boss => vec![Spawn {
            pos: CENTER,
            kind: EnemyKind::Boss,
        }],
        RoomKind::Normal => {
            let count = rng.range(2..=4) as usize;
            let weights = NORMAL_WEIGHTS.map(|(_, w)| w);
            random_points(layout, count, rng)
                .into_iter()
                .map(|pos| Spawn {
                    pos,
                    kind: NORMAL_WEIGHTS[rng.weighted_index(&weights)].0,
                })
                .collect()
        }
        RoomKind::Start | RoomKind::Treasure | RoomKind::Shop => Vec::new(),
    }
}

fn random_points(layout: &RoomLayout, count: usize, rng: &mut Rng) -> Vec<GridPos> {
    let mut candidates: Vec<GridPos> = layout
        .iter()
        .filter(|&(p, t)| {
            t == Tile::Floor
                && Direction::ALL
                    .into_iter()
                    .all(|d| p.manhattan(inside_door(d)) >= MIN_DOOR_DISTANCE)
        })
        .map(|(p, _)| p)
        .collect();
    rng.shuffle(&mut candidates);
    candidates.truncate(count);
    candidates
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::{floor, templates};

    #[test]
    fn same_room_same_spawns() {
        let seed = RunSeed(77);
        let layout = templates::pool(RoomKind::Normal)[1].layout();
        let plan = || {
            plan_spawns(
                RoomKind::Normal,
                &layout,
                &mut spawn_rng(seed, 1, GridPos::new(3, 4)),
            )
        };
        assert_eq!(plan(), plan());
    }

    #[test]
    fn spawns_are_valid_for_many_floors() {
        for s in 0..500 {
            let seed = RunSeed(s);
            let floor = floor::generate(seed, 1);
            for (pos, room) in floor.rooms() {
                let layout = floor.room_layout(pos).unwrap();
                let spawns = plan_spawns(room.kind, &layout, &mut spawn_rng(seed, 1, pos));

                match room.kind {
                    RoomKind::Normal => {
                        assert!((2..=4).contains(&spawns.len()));
                        assert!(spawns.iter().all(|s| s.kind != EnemyKind::Boss));
                    }
                    RoomKind::Boss => assert_eq!(
                        spawns,
                        vec![Spawn {
                            pos: CENTER,
                            kind: EnemyKind::Boss
                        }]
                    ),
                    _ => assert!(spawns.is_empty()),
                }
                let unique: BTreeSet<_> = spawns.iter().map(|s| s.pos).collect();
                assert_eq!(unique.len(), spawns.len(), "doppelter Spawnpunkt");
                for s in &spawns {
                    assert!(
                        !layout.blocks_movement(s.pos),
                        "Spawn in Hindernis bei {s:?}"
                    );
                    if room.kind != RoomKind::Boss {
                        for d in Direction::ALL {
                            assert!(s.pos.manhattan(inside_door(d)) >= MIN_DOOR_DISTANCE);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn all_normal_kinds_appear_roughly_by_weight() {
        let mut counts: BTreeMap<EnemyKind, u32> = BTreeMap::new();
        let layout = RoomLayout::empty();
        for s in 0..2_000 {
            let mut rng = spawn_rng(RunSeed(s), 1, GridPos::new(6, 6));
            for spawn in plan_spawns(RoomKind::Normal, &layout, &mut rng) {
                *counts.entry(spawn.kind).or_default() += 1;
            }
        }
        let total: u32 = counts.values().sum();
        let share = |k| counts.get(&k).copied().unwrap_or(0) as f64 / total as f64;
        assert!(
            (0.45..0.55).contains(&share(EnemyKind::Chaser)),
            "{counts:?}"
        );
        assert!(
            (0.20..0.30).contains(&share(EnemyKind::Shooter)),
            "{counts:?}"
        );
        assert!(
            (0.20..0.30).contains(&share(EnemyKind::Charger)),
            "{counts:?}"
        );
    }
}
