//! Items: Definitionen, Effekte und Item-Pools.
//!
//! Ein Item ist reine **Daten**: eine Liste von `Effect`s. Was die Effekte
//! bedeuten, berechnen die Funktionen unten (`compute_stats`, `shot_pattern`,
//! …). Die Spiel-Crate wendet die Ergebnisse nur noch an. Dadurch lassen sich
//! alle Synergien hier ohne Engine testen.
//!
//! In M6b wandern die Definitionen in RON-Dateien. Die Struktur bleibt gleich.

use crate::{
    rng::Rng,
    stats::{Modifier, Op, Stat, Stats},
};

/// Aus welchem Pool ein Item kommen kann.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pool {
    Treasure,
    Boss,
    Shop,
}

/// Direkte Belohnung (ohne Pickup auf dem Boden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reward {
    /// Heilt so viele halbe Herzen.
    Heal(i32),
    Coins(u32),
    Keys(u32),
    Bombs(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// Fester Stat-Modifier.
    Stat(Modifier),
    /// Zusätzliche Herzcontainer (halbe Herzen). Heilt beim Aufheben mit.
    MaxHealth(i32),
    /// Zusätzliche Schüsse pro Salve, gefächert.
    ExtraShots(u32),
    /// Schüsse fliegen durch Gegner hindurch.
    Piercing,
    /// Schüsse lenken zum nächsten Gegner.
    Homing,
    /// Modifier wirkt einmal pro `per` Münzen (Add: addiert, Mul: potenziert).
    PerCoins { per: u32, modifier: Modifier },
    /// Modifier wirkt einmal pro fehlendem halben Herz.
    PerMissingHealth(Modifier),
    /// Belohnung nach jedem geräumten Raum.
    OnRoomClear(Reward),
    /// Belohnung mit Wahrscheinlichkeit `chance` bei jedem Kill.
    OnKill { chance: f64, reward: Reward },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemDef {
    /// Stabiler Bezeichner (für Speicherstände und später RON).
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub pools: &'static [Pool],
    /// Preis im Shop (Münzen).
    pub price: u32,
    pub effects: &'static [Effect],
}

use Effect::*;
use Pool::*;
use Stat::*;

const fn add(stat: Stat, v: f32) -> Effect {
    Effect::Stat(Modifier::add(stat, v))
}
const fn mul(stat: Stat, v: f32) -> Effect {
    Effect::Stat(Modifier::mul(stat, v))
}

/// Alle Items. `static` statt `const`: Es gibt genau eine Kopie im Programm,
/// und `&'static ItemDef` darf überall hin weitergereicht werden.
pub static ITEMS: &[ItemDef] = &[
    ItemDef {
        id: "onion",
        name: "Zwiebel",
        description: "Feuerrate +30 %",
        pools: &[Treasure, Shop],
        price: 15,
        effects: &[mul(FireRate, 1.3)],
    },
    ItemDef {
        id: "whetstone",
        name: "Wetzstein",
        description: "Schaden +1",
        pools: &[Treasure, Shop],
        price: 15,
        effects: &[add(Damage, 1.0)],
    },
    ItemDef {
        id: "steak",
        name: "Steak",
        description: "+1 Herz, Schaden +0,5",
        pools: &[Treasure, Boss],
        price: 15,
        effects: &[MaxHealth(2), add(Damage, 0.5)],
    },
    ItemDef {
        id: "sneakers",
        name: "Laufschuhe",
        description: "Tempo +1",
        pools: &[Treasure, Shop],
        price: 10,
        effects: &[add(MoveSpeed, 1.0)],
    },
    ItemDef {
        id: "spyglass",
        name: "Fernrohr",
        description: "Reichweite +2,5, Schussgeschwindigkeit +1,5",
        pools: &[Treasure, Shop],
        price: 10,
        effects: &[add(Range, 2.5), add(ShotSpeed, 1.5)],
    },
    ItemDef {
        id: "trident",
        name: "Dreizack",
        description: "Drei Schüsse pro Salve, Feuerrate -30 %",
        pools: &[Treasure],
        price: 20,
        effects: &[ExtraShots(2), mul(FireRate, 0.7)],
    },
    ItemDef {
        id: "drill",
        name: "Bohrer",
        description: "Schüsse durchschlagen Gegner, Schaden -10 %",
        pools: &[Treasure],
        price: 20,
        effects: &[Piercing, mul(Damage, 0.9)],
    },
    ItemDef {
        id: "magnet_eye",
        name: "Magnetauge",
        description: "Schüsse suchen ihr Ziel, Schussgeschwindigkeit -1",
        pools: &[Treasure],
        price: 20,
        effects: &[Homing, add(ShotSpeed, -1.0)],
    },
    ItemDef {
        id: "gold_rush",
        name: "Goldrausch",
        description: "Feuerrate +3 % pro Münze",
        pools: &[Treasure, Shop],
        price: 10,
        effects: &[PerCoins {
            per: 1,
            modifier: Modifier::mul(FireRate, 1.03),
        }],
    },
    ItemDef {
        id: "piggy_bank",
        name: "Sparschwein",
        description: "+1 Münze pro geräumtem Raum",
        pools: &[Treasure, Shop],
        price: 5,
        effects: &[OnRoomClear(Reward::Coins(1))],
    },
    ItemDef {
        id: "payday",
        name: "Zahltag",
        description: "25 % Chance auf eine Münze pro Kill",
        pools: &[Treasure, Shop],
        price: 10,
        effects: &[OnKill {
            chance: 0.25,
            reward: Reward::Coins(1),
        }],
    },
    ItemDef {
        id: "berserker",
        name: "Berserker",
        description: "Schaden +0,4 pro fehlendem halben Herz",
        pools: &[Treasure, Boss],
        price: 20,
        effects: &[PerMissingHealth(Modifier::add(Damage, 0.4))],
    },
    ItemDef {
        id: "vampire_fang",
        name: "Vampirzahn",
        description: "15 % Chance auf ein halbes Herz pro Kill",
        pools: &[Treasure, Boss],
        price: 15,
        effects: &[OnKill {
            chance: 0.15,
            reward: Reward::Heal(1),
        }],
    },
    ItemDef {
        id: "tree_of_life",
        name: "Lebensbaum",
        description: "Heilt ein halbes Herz pro geräumtem Raum",
        pools: &[Boss],
        price: 20,
        effects: &[OnRoomClear(Reward::Heal(1))],
    },
    ItemDef {
        id: "lead_bullet",
        name: "Bleikugel",
        description: "Schaden +50 %, Feuerrate -15 %, Schussgeschwindigkeit -2",
        pools: &[Treasure, Boss],
        price: 15,
        effects: &[mul(Damage, 1.5), mul(FireRate, 0.85), add(ShotSpeed, -2.0)],
    },
    ItemDef {
        id: "coffee",
        name: "Kaffee",
        description: "Tempo +0,7, Feuerrate +15 %, Schaden -10 %",
        pools: &[Treasure, Shop],
        price: 10,
        effects: &[add(MoveSpeed, 0.7), mul(FireRate, 1.15), mul(Damage, 0.9)],
    },
    ItemDef {
        id: "golden_heart",
        name: "Goldenes Herz",
        description: "+1 Herz",
        pools: &[Shop],
        price: 15,
        effects: &[MaxHealth(2)],
    },
    ItemDef {
        id: "shotgun",
        name: "Schrotflinte",
        description: "Fünf Schüsse, Reichweite -50 %, Schaden -40 %",
        pools: &[Boss],
        price: 25,
        effects: &[ExtraShots(4), mul(Range, 0.5), mul(Damage, 0.6)],
    },
];

/// Wird ausgegeben, wenn ein Pool leer ist.
pub static FALLBACK: ItemDef = ItemDef {
    id: "breakfast",
    name: "Frühstück",
    description: "+1 Herz",
    pools: &[],
    price: 5,
    effects: &[MaxHealth(2)],
};

pub fn by_id(id: &str) -> Option<&'static ItemDef> {
    ITEMS.iter().chain([&FALLBACK]).find(|i| i.id == id)
}

// --- Auswertung --------------------------------------------------------------

/// Endgültige Stats aus Grundwerten, Items und Situation.
pub fn compute_stats(items: &[&ItemDef], coins: u32, missing_health: u32) -> Stats {
    let modifiers = items
        .iter()
        .flat_map(|i| i.effects)
        .filter_map(|e| match *e {
            Effect::Stat(m) => Some(m),
            PerCoins { per, modifier } => Some(repeat(modifier, coins / per.max(1))),
            PerMissingHealth(modifier) => Some(repeat(modifier, missing_health)),
            _ => None,
        });
    Stats::with_modifiers(Stats::BASE, modifiers)
}

/// Einen Modifier `n`-mal anwenden, als ein einziger Modifier.
fn repeat(m: Modifier, n: u32) -> Modifier {
    let op = match m.op {
        Op::Add(v) => Op::Add(v * n as f32),
        Op::Mul(v) => Op::Mul(v.powi(n as i32)),
    };
    Modifier { stat: m.stat, op }
}

/// Wie geschossen wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShotPattern {
    pub count: u32,
    pub piercing: bool,
    pub homing: bool,
}

pub fn shot_pattern(items: &[&ItemDef]) -> ShotPattern {
    let effects = || items.iter().flat_map(|i| i.effects);
    ShotPattern {
        count: 1 + effects()
            .map(|e| if let ExtraShots(n) = e { *n } else { 0 })
            .sum::<u32>(),
        piercing: effects().any(|e| matches!(e, Piercing)),
        homing: effects().any(|e| matches!(e, Homing)),
    }
}

/// Zusätzliche maximale Lebenspunkte (halbe Herzen).
pub fn max_health_bonus(items: &[&ItemDef]) -> i32 {
    items
        .iter()
        .flat_map(|i| i.effects)
        .map(|e| if let MaxHealth(n) = e { *n } else { 0 })
        .sum()
}

pub fn room_clear_rewards(items: &[&ItemDef]) -> Vec<Reward> {
    items
        .iter()
        .flat_map(|i| i.effects)
        .filter_map(|e| {
            if let OnRoomClear(r) = e {
                Some(*r)
            } else {
                None
            }
        })
        .collect()
}

/// Würfelt alle Kill-Effekte aus.
pub fn kill_rewards(items: &[&ItemDef], rng: &mut Rng) -> Vec<Reward> {
    items
        .iter()
        .flat_map(|i| i.effects)
        .filter_map(|e| match e {
            OnKill { chance, reward } => rng.chance(*chance).then_some(*reward),
            _ => None,
        })
        .collect()
}

// --- Pools -------------------------------------------------------------------

/// Noch nicht vergebene Items eines Runs. Jedes Item erscheint höchstens einmal.
#[derive(Debug, Clone)]
pub struct ItemPools {
    remaining: Vec<&'static ItemDef>,
}

impl Default for ItemPools {
    fn default() -> Self {
        Self {
            remaining: ITEMS.iter().collect(),
        }
    }
}

impl ItemPools {
    /// Alle Items außer den gesperrten (siehe `meta::locked_items`).
    pub fn without(locked: &[&str]) -> Self {
        Self {
            remaining: ITEMS.iter().filter(|i| !locked.contains(&i.id)).collect(),
        }
    }

    /// Zieht ein Item aus `pool` und entfernt es aus allen Pools.
    pub fn draw(&mut self, pool: Pool, rng: &mut Rng) -> &'static ItemDef {
        let candidates: Vec<usize> = self
            .remaining
            .iter()
            .enumerate()
            .filter(|(_, i)| i.pools.contains(&pool))
            .map(|(idx, _)| idx)
            .collect();
        match rng.choose(&candidates) {
            // `swap_remove` ist O(1); die Reihenfolge in `remaining` ist egal.
            Some(&idx) => self.remaining.swap_remove(idx),
            None => &FALLBACK,
        }
    }

    pub fn remaining(&self) -> usize {
        self.remaining.len()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn item(id: &str) -> &'static ItemDef {
        by_id(id).unwrap_or_else(|| panic!("Item '{id}' fehlt"))
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn ids_are_unique_and_items_are_in_pools() {
        let ids: BTreeSet<_> = ITEMS.iter().map(|i| i.id).collect();
        assert_eq!(ids.len(), ITEMS.len());
        assert!(ITEMS.len() >= 15);
        for i in ITEMS {
            assert!(!i.pools.is_empty(), "{} ist in keinem Pool", i.id);
            assert!(!i.effects.is_empty(), "{} hat keine Effekte", i.id);
        }
    }

    #[test]
    fn every_pool_has_items() {
        for pool in [Treasure, Boss, Shop] {
            assert!(ITEMS.iter().filter(|i| i.pools.contains(&pool)).count() >= 4);
        }
    }

    #[test]
    fn no_items_means_base_stats() {
        assert_eq!(compute_stats(&[], 10, 3), Stats::BASE);
    }

    #[test]
    fn gold_rush_scales_with_coins() {
        let items = [item("gold_rush")];
        let none = compute_stats(&items, 0, 0).fire_rate;
        let ten = compute_stats(&items, 10, 0).fire_rate;
        assert!(approx(none, Stats::BASE.fire_rate));
        assert!(approx(ten, Stats::BASE.fire_rate * 1.03f32.powi(10)));
    }

    #[test]
    fn berserker_scales_with_missing_health() {
        let items = [item("berserker")];
        let d = compute_stats(&items, 0, 3).damage;
        assert!(approx(d, Stats::BASE.damage + 1.2));
    }

    #[test]
    fn trident_and_drill_combine() {
        let p = shot_pattern(&[item("trident"), item("drill")]);
        assert_eq!(
            p,
            ShotPattern {
                count: 3,
                piercing: true,
                homing: false
            }
        );
    }

    #[test]
    fn trident_and_shotgun_stack_shots() {
        assert_eq!(shot_pattern(&[item("trident"), item("shotgun")]).count, 7);
    }

    #[test]
    fn coin_engine_synergy() {
        let items = [item("gold_rush"), item("piggy_bank"), item("payday")];
        assert_eq!(room_clear_rewards(&items), vec![Reward::Coins(1)]);
        let mut rng = Rng::from_seed(9);
        let coins: usize = (0..1000)
            .map(|_| kill_rewards(&items, &mut rng).len())
            .sum();
        assert!((200..300).contains(&coins), "Zahltag ~25 %: {coins}");
    }

    #[test]
    fn max_health_bonus_sums() {
        assert_eq!(max_health_bonus(&[item("steak"), item("golden_heart")]), 4);
    }

    #[test]
    fn pools_never_repeat_and_fall_back() {
        let mut pools = ItemPools::default();
        let mut rng = Rng::from_seed(4);
        let mut seen = BTreeSet::new();
        for _ in 0..ITEMS.len() + 5 {
            let i = pools.draw(Treasure, &mut rng);
            if i.id != FALLBACK.id {
                assert!(seen.insert(i.id), "{} doppelt", i.id);
                assert!(i.pools.contains(&Treasure));
            }
        }
        // Irgendwann ist der Schatz-Pool leer, dann kommt das Frühstück.
        assert_eq!(pools.draw(Treasure, &mut rng).id, FALLBACK.id);
    }

    #[test]
    fn locked_items_never_drop() {
        let mut pools = ItemPools::without(&["shotgun", "steak"]);
        let mut rng = Rng::from_seed(8);
        for _ in 0..100 {
            let id = pools.draw(Boss, &mut rng).id;
            assert!(id != "shotgun" && id != "steak", "{id}");
        }
    }

    #[test]
    fn drawing_from_one_pool_removes_from_all() {
        let mut pools = ItemPools::default();
        let mut rng = Rng::from_seed(2);
        let before = pools.remaining();
        let item = pools.draw(Shop, &mut rng);
        assert_eq!(pools.remaining(), before - 1);
        for _ in 0..50 {
            assert_ne!(pools.draw(Treasure, &mut rng).id, item.id);
        }
    }
}
