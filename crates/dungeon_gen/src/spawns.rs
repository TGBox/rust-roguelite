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
    /// Springt in Sätzen auf den Spieler zu, verschießt beim Landen ein Kreuz.
    Hopper,
    /// Zerfällt beim Tod in zwei `Splitling`s.
    Splitter,
    /// Kleiner, schneller Verfolger – entsteht nur im Spiel (Teiler, Beschwörer).
    Splitling,
    /// Hält Abstand und ruft Splitlinge herbei.
    Summoner,
    /// Boss Etage 1: Klumpenkönig – Schussringe.
    Boss,
    /// Boss Etage 2: Brutmutter – ruft Nachwuchs, später Sturmangriffe.
    BroodMother,
    /// Boss Etage 3: Wächter – Spiralsalven, später gezielte Stöße.
    Warden,
}

impl EnemyKind {
    pub fn is_boss(self) -> bool {
        matches!(
            self,
            EnemyKind::Boss | EnemyKind::BroodMother | EnemyKind::Warden
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spawn {
    pub pos: GridPos,
    pub kind: EnemyKind,
    /// Champion: doppeltes Leben, größer, lässt Beute fallen.
    pub champion: bool,
}

/// Gegnermischung mit relativer Häufigkeit – tiefer wird es vielfältiger.
fn weights_for_depth(depth: u32) -> &'static [(EnemyKind, u32)] {
    use EnemyKind::*;
    match depth {
        0 | 1 => &[(Chaser, 40), (Shooter, 22), (Charger, 20), (Hopper, 18)],
        2 => &[
            (Chaser, 25),
            (Shooter, 18),
            (Charger, 15),
            (Hopper, 15),
            (Splitter, 15),
            (Summoner, 12),
        ],
        _ => &[
            (Chaser, 18),
            (Shooter, 17),
            (Charger, 15),
            (Hopper, 17),
            (Splitter, 18),
            (Summoner, 15),
        ],
    }
}

/// Welcher Boss auf welcher Etage wartet.
pub fn boss_for_depth(depth: u32) -> EnemyKind {
    match depth {
        0 | 1 => EnemyKind::Boss,
        2 => EnemyKind::BroodMother,
        _ => EnemyKind::Warden,
    }
}

/// Wahrscheinlichkeit, dass ein normaler Gegner ein Champion ist.
pub fn champion_chance(depth: u32) -> f64 {
    (0.05 + 0.07 * depth.saturating_sub(1) as f64).min(0.3)
}

/// Anzahl Gegnerwellen eines Raums.
pub fn waves(kind: RoomKind) -> u32 {
    match kind {
        RoomKind::Challenge => 3,
        _ => 1,
    }
}

/// Eigener Stream je Etage und Raum.
pub fn spawn_rng(seed: RunSeed, depth: u32, room: GridPos) -> Rng {
    let room_index = (room.y * FLOOR_SIZE + room.x) as u64;
    seed.stream("spawns", u64::from(depth) * 1_000 + room_index)
}

/// Stream für die Welle `wave` (ab 0). Welle 0 = derselbe Stream wie
/// `spawn_rng`, damit normale Räume unverändert bleiben.
pub fn wave_rng(seed: RunSeed, depth: u32, room: GridPos, wave: u32) -> Rng {
    if wave == 0 {
        return spawn_rng(seed, depth, room);
    }
    let room_index = (room.y * FLOOR_SIZE + room.x) as u64;
    seed.stream(
        "waves",
        (u64::from(depth) * 1_000 + room_index) * 10 + u64::from(wave),
    )
}

/// Gegner für die erste (oder einzige) Welle eines Raums.
pub fn plan_spawns(kind: RoomKind, layout: &RoomLayout, depth: u32, rng: &mut Rng) -> Vec<Spawn> {
    plan_wave(kind, layout, depth, 0, rng)
}

/// Gegner einer Welle. Boss: genau einer in der Mitte. Normale Räume:
/// 2–4 (ab Etage 2: 3–5). Herausforderung: jede Welle größer.
pub fn plan_wave(
    kind: RoomKind,
    layout: &RoomLayout,
    depth: u32,
    wave: u32,
    rng: &mut Rng,
) -> Vec<Spawn> {
    let count = match kind {
        RoomKind::Boss => {
            return vec![Spawn {
                pos: CENTER,
                kind: boss_for_depth(depth),
                champion: false,
            }];
        }
        RoomKind::Normal if depth <= 1 => rng.range(2..=4),
        RoomKind::Normal => rng.range(3..=5),
        RoomKind::Challenge => 3 + wave as i32 + depth.saturating_sub(1) as i32,
        RoomKind::Start
        | RoomKind::Treasure
        | RoomKind::Shop
        | RoomKind::Sacrifice
        | RoomKind::Secret => return Vec::new(),
    } as usize;

    let table = weights_for_depth(depth);
    let weights: Vec<u32> = table.iter().map(|(_, w)| *w).collect();
    // Herausforderungen: mehr Champions, damit sich die Belohnung lohnt.
    let champion = champion_chance(depth)
        * if kind == RoomKind::Challenge {
            2.0
        } else {
            1.0
        };
    random_points(layout, count, rng)
        .into_iter()
        .map(|pos| Spawn {
            pos,
            kind: table[rng.weighted_index(&weights)].0,
            champion: rng.chance(champion),
        })
        .collect()
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
                1,
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
                let spawns = plan_spawns(room.kind, &layout, 1, &mut spawn_rng(seed, 1, pos));

                match room.kind {
                    RoomKind::Normal => {
                        assert!((2..=4).contains(&spawns.len()));
                        assert!(spawns.iter().all(|s| !s.kind.is_boss()));
                        assert!(spawns.iter().all(|s| s.kind != EnemyKind::Splitling));
                    }
                    RoomKind::Challenge => assert_eq!(spawns.len(), 3),
                    RoomKind::Boss => assert_eq!(
                        spawns,
                        vec![Spawn {
                            pos: CENTER,
                            kind: EnemyKind::Boss,
                            champion: false,
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
            for spawn in plan_spawns(RoomKind::Normal, &layout, 1, &mut rng) {
                *counts.entry(spawn.kind).or_default() += 1;
            }
        }
        let total: u32 = counts.values().sum();
        let share = |k| counts.get(&k).copied().unwrap_or(0) as f64 / total as f64;
        for (kind, weight) in weights_for_depth(1) {
            let expected = *weight as f64 / 100.0;
            assert!(
                (share(*kind) - expected).abs() < 0.04,
                "{kind:?}: {counts:?}"
            );
        }
    }

    #[test]
    fn deeper_floors_bring_new_enemies_and_bosses() {
        let layout = RoomLayout::empty();
        let mut kinds = BTreeSet::new();
        for s in 0..500 {
            let mut rng = spawn_rng(RunSeed(s), 3, GridPos::new(6, 6));
            kinds.extend(
                plan_spawns(RoomKind::Normal, &layout, 3, &mut rng)
                    .iter()
                    .map(|s| s.kind),
            );
        }
        assert!(kinds.contains(&EnemyKind::Splitter) && kinds.contains(&EnemyKind::Summoner));
        let bosses: Vec<_> = (1..=3).map(boss_for_depth).collect();
        assert_eq!(
            bosses,
            [EnemyKind::Boss, EnemyKind::BroodMother, EnemyKind::Warden]
        );
        assert!(bosses.iter().all(|b| b.is_boss()));
    }

    #[test]
    fn challenge_waves_grow_and_differ() {
        let layout = RoomLayout::empty();
        let seed = RunSeed(5);
        let room = GridPos::new(4, 4);
        let sizes: Vec<usize> = (0..waves(RoomKind::Challenge))
            .map(|w| {
                plan_wave(
                    RoomKind::Challenge,
                    &layout,
                    2,
                    w,
                    &mut wave_rng(seed, 2, room, w),
                )
                .len()
            })
            .collect();
        assert_eq!(sizes, vec![4, 5, 6]);
        assert_eq!(waves(RoomKind::Normal), 1);
    }

    #[test]
    fn champions_appear_at_expected_rate() {
        let layout = RoomLayout::empty();
        let (mut total, mut champs) = (0, 0);
        for s in 0..3_000 {
            let mut rng = spawn_rng(RunSeed(s), 3, GridPos::new(2, 2));
            for spawn in plan_spawns(RoomKind::Normal, &layout, 3, &mut rng) {
                total += 1;
                champs += spawn.champion as u32;
            }
        }
        let rate = champs as f64 / total as f64;
        assert!((rate - champion_chance(3)).abs() < 0.03, "{rate}");
    }
}
