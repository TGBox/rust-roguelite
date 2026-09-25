//! Wo in einem Raum Gegner erscheinen.
//!
//! Pro Raum ein eigener RNG-Stream: Egal in welcher Reihenfolge der Spieler
//! die Räume betritt, derselbe Raum bekommt immer dieselben Spawnpunkte.

use crate::{
    Direction, GridPos, RoomKind, RoomLayout, Tile,
    floor::FLOOR_SIZE,
    rng::{Rng, RunSeed},
    room::{CENTER, inside_door},
};

/// Mindestabstand (Manhattan) zwischen Spawnpunkt und Türeingang,
/// damit beim Betreten kein Gegner direkt neben dem Spieler steht.
pub const MIN_DOOR_DISTANCE: u32 = 4;

/// Eigener Stream je Etage und Raum.
pub fn spawn_rng(seed: RunSeed, depth: u32, room: GridPos) -> Rng {
    let room_index = (room.y * FLOOR_SIZE + room.x) as u64;
    seed.stream("spawns", u64::from(depth) * 1_000 + room_index)
}

/// Spawnpunkte für einen Raum dieser Art.
/// Boss: genau einer in der Mitte. Normale Räume: 2–4 zufällige Stellen.
pub fn plan_spawns(kind: RoomKind, layout: &RoomLayout, rng: &mut Rng) -> Vec<GridPos> {
    match kind {
        RoomKind::Boss => vec![CENTER],
        RoomKind::Normal => {
            let count = rng.range(2..=4) as usize;
            random_points(layout, count, rng)
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
    use std::collections::BTreeSet;

    use super::*;
    use crate::{floor, templates};

    #[test]
    fn same_room_same_spawns() {
        let seed = RunSeed(77);
        let layout = templates::pool(RoomKind::Normal)[1].layout();
        let a = plan_spawns(
            RoomKind::Normal,
            &layout,
            &mut spawn_rng(seed, 1, GridPos::new(3, 4)),
        );
        let b = plan_spawns(
            RoomKind::Normal,
            &layout,
            &mut spawn_rng(seed, 1, GridPos::new(3, 4)),
        );
        assert_eq!(a, b);
    }

    #[test]
    fn spawns_are_valid_for_many_floors() {
        for s in 0..500 {
            let seed = RunSeed(s);
            let floor = floor::generate(seed, 1);
            for (pos, room) in floor.rooms() {
                let layout = floor.room_layout(pos).unwrap();
                let points = plan_spawns(room.kind, &layout, &mut spawn_rng(seed, 1, pos));

                match room.kind {
                    RoomKind::Normal => assert!((2..=4).contains(&points.len())),
                    RoomKind::Boss => assert_eq!(points, vec![CENTER]),
                    _ => assert!(points.is_empty()),
                }
                let unique: BTreeSet<_> = points.iter().collect();
                assert_eq!(unique.len(), points.len(), "doppelter Spawnpunkt");
                for p in points {
                    assert!(!layout.blocks_movement(p), "Spawn in Hindernis bei {p:?}");
                    for d in Direction::ALL {
                        assert!(
                            p.manhattan(inside_door(d)) >= MIN_DOOR_DISTANCE
                                || room.kind == RoomKind::Boss
                        );
                    }
                }
            }
        }
    }
}
