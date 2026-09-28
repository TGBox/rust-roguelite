//! Raumvorlagen: das Innere eines Raums als ASCII-Art.
//!
//! Regeln (werden von den Tests erzwungen):
//! - Die Kacheln direkt vor den vier Türpositionen sind Boden.
//! - Alle begehbaren Kacheln hängen zusammen – kein eingesperrter Bereich.
//!
//! Die Türen selbst sind hier noch Wand; M4 öffnet sie je nach Nachbarräumen.
//! Später (M6) wandern die Vorlagen in RON-Dateien.

use crate::{RoomLayout, floor::RoomKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomTemplate {
    pub name: &'static str,
    pub ascii: &'static str,
}

impl RoomTemplate {
    /// Die Vorlagen sind durch Tests validiert, daher ist `expect` hier vertretbar:
    /// Ein Fehler wäre ein Programmierfehler, kein Laufzeitfehler.
    pub fn layout(&self) -> RoomLayout {
        RoomLayout::from_ascii(self.ascii)
            .unwrap_or_else(|e| panic!("Vorlage '{}' ist ungültig: {e}", self.name))
    }
}

/// Alle Vorlagen, aus denen ein Raum dieser Art gewählt werden darf.
pub fn pool(kind: RoomKind) -> &'static [RoomTemplate] {
    match kind {
        RoomKind::Start => START,
        RoomKind::Normal => NORMAL,
        RoomKind::Boss => BOSS,
        RoomKind::Treasure => TREASURE,
        RoomKind::Shop => SHOP,
        RoomKind::Challenge => CHALLENGE,
        RoomKind::Sacrifice => SACRIFICE,
        RoomKind::Secret => SECRET,
    }
}

const EMPTY: &str = "
    ###############
    #.............#
    #.............#
    #.............#
    #.............#
    #.............#
    #.............#
    #.............#
    ###############
";

const START: &[RoomTemplate] = &[RoomTemplate {
    name: "start",
    ascii: EMPTY,
}];

const NORMAL: &[RoomTemplate] = &[
    RoomTemplate {
        name: "empty",
        ascii: EMPTY,
    },
    RoomTemplate {
        name: "pillars",
        ascii: "
            ###############
            #.............#
            #..oo.....__..#
            #..o......__..#
            #.............#
            #..o.......o..#
            #..oo.....ooo.#
            #.............#
            ###############
        ",
    },
    RoomTemplate {
        name: "cross",
        ascii: "
            ###############
            #.............#
            #.....o.o.....#
            #.....o.o.....#
            #.............#
            #.....o.o.....#
            #.....o.o.....#
            #.............#
            ###############
        ",
    },
    RoomTemplate {
        name: "pit_center",
        ascii: "
            ###############
            #.............#
            #.............#
            #...._____....#
            #...._____....#
            #...._____....#
            #.............#
            #.............#
            ###############
        ",
    },
    RoomTemplate {
        name: "rows",
        ascii: "
            ###############
            #.............#
            #.ooo.....ooo.#
            #.............#
            #.............#
            #.............#
            #.ooo.....ooo.#
            #.............#
            ###############
        ",
    },
    RoomTemplate {
        name: "diamond",
        ascii: "
            ###############
            #.............#
            #......o......#
            #.....o.o.....#
            #.............#
            #.....o.o.....#
            #......o......#
            #.............#
            ###############
        ",
    },
    RoomTemplate {
        name: "pit_lanes",
        ascii: "
            ###############
            #.............#
            #.__.......__.#
            #.__.......__.#
            #.............#
            #.__.......__.#
            #.__.......__.#
            #.............#
            ###############
        ",
    },
];

const BOSS: &[RoomTemplate] = &[RoomTemplate {
    name: "arena",
    ascii: "
        ###############
        #.............#
        #.o.........o.#
        #.............#
        #.............#
        #.............#
        #.o.........o.#
        #.............#
        ###############
    ",
}];

const TREASURE: &[RoomTemplate] = &[RoomTemplate {
    name: "altar",
    ascii: "
        ###############
        #.............#
        #.............#
        #.....o.o.....#
        #.............#
        #.....o.o.....#
        #.............#
        #.............#
        ###############
    ",
}];

const SHOP: &[RoomTemplate] = &[RoomTemplate {
    name: "shop",
    ascii: EMPTY,
}];

/// Herausforderung: Arena mit Deckung, damit man den Wellen ausweichen kann.
const CHALLENGE: &[RoomTemplate] = &[
    RoomTemplate {
        name: "gauntlet",
        ascii: "
            ###############
            #.............#
            #..o...o...o..#
            #.............#
            #.............#
            #.............#
            #..o...o...o..#
            #.............#
            ###############
        ",
    },
    RoomTemplate {
        name: "pit_ring",
        ascii: "
            ###############
            #.............#
            #...__...__...#
            #.............#
            #.............#
            #.............#
            #...__...__...#
            #.............#
            ###############
        ",
    },
];

/// Opferraum: Der Altar steht in der Mitte, Gruben rahmen ihn ein.
const SACRIFICE: &[RoomTemplate] = &[RoomTemplate {
    name: "sacrifice",
    ascii: "
        ###############
        #.............#
        #.__.......__.#
        #.............#
        #.............#
        #.............#
        #.__.......__.#
        #.............#
        ###############
    ",
}];

/// Geheimraum: klein wirkend durch viele Felsen.
const SECRET: &[RoomTemplate] = &[RoomTemplate {
    name: "hideout",
    ascii: "
        ###############
        #oo.........oo#
        #o...........o#
        #.............#
        #.............#
        #.............#
        #o...........o#
        #oo.........oo#
        ###############
    ",
}];

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, VecDeque};

    use super::*;
    use crate::{Direction, GridPos, room::inside_door};

    const ALL_KINDS: [RoomKind; 8] = RoomKind::ALL;

    fn all_templates() -> impl Iterator<Item = &'static RoomTemplate> {
        ALL_KINDS.into_iter().flat_map(|k| pool(k).iter())
    }

    #[test]
    fn every_kind_has_templates() {
        for kind in ALL_KINDS {
            assert!(!pool(kind).is_empty(), "{kind:?} hat keine Vorlagen");
        }
    }

    #[test]
    fn all_templates_parse() {
        for t in all_templates() {
            assert!(
                RoomLayout::from_ascii(t.ascii).is_ok(),
                "Vorlage '{}' ungültig",
                t.name
            );
        }
    }

    #[test]
    fn door_entrances_are_free() {
        for t in all_templates() {
            let layout = t.layout();
            for dir in Direction::ALL {
                let pos = inside_door(dir);
                assert!(
                    !layout.blocks_movement(pos),
                    "Vorlage '{}': Eingang {dir:?} bei {pos:?} ist blockiert",
                    t.name
                );
            }
        }
    }

    #[test]
    fn walkable_area_is_connected() {
        for t in all_templates() {
            let layout = t.layout();
            let walkable: BTreeSet<GridPos> = layout
                .iter()
                .filter(|&(p, _)| !layout.blocks_movement(p))
                .map(|(p, _)| p)
                .collect();

            // Flutfüllung vom Nordeingang aus.
            let start = inside_door(Direction::North);
            let mut reached = BTreeSet::from([start]);
            let mut queue = VecDeque::from([start]);
            while let Some(p) = queue.pop_front() {
                for (_, n) in p.neighbors() {
                    if walkable.contains(&n) && reached.insert(n) {
                        queue.push_back(n);
                    }
                }
            }
            assert_eq!(
                reached.len(),
                walkable.len(),
                "Vorlage '{}' hat abgeschnittene Bereiche",
                t.name
            );
        }
    }

    #[test]
    fn template_names_are_unique() {
        let mut names = BTreeSet::new();
        for t in all_templates() {
            // Mehrere Vorlagen dürfen dasselbe Layout nutzen, aber nicht denselben Namen.
            assert!(names.insert(t.name), "doppelter Name '{}'", t.name);
        }
    }
}
