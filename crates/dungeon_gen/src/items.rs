//! Items: Definitionen, Effekte, Datenbank und Item-Pools.
//!
//! Ein Item ist reine **Daten**: eine Liste von `Effect`s. Was die Effekte
//! bedeuten, berechnen die Funktionen unten (`compute_stats`, `shot_pattern`,
//! …). Die Spiel-Crate wendet die Ergebnisse nur noch an. Dadurch lassen sich
//! alle Synergien hier ohne Engine testen.
//!
//! Seit M6b werden Items zur Laufzeit aus `items.ron` geladen. Deshalb gehört
//! jedes `ItemDef` seiner `ItemDb` (owned `String`/`Vec`), und der Rest des
//! Spiels verweist nur per `ItemId` darauf. `ItemDb::builtin()` liefert die
//! eingebauten Standardwerte – als Rückfall und für Tests.
//!
//! Mit dem Feature `serde` sind alle Typen (de)serialisierbar.

use std::fmt;

use crate::{
    rng::Rng,
    stats::{Modifier, Op, Stats},
};

/// Aus welchem Pool ein Item kommen kann.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Pool {
    Treasure,
    Boss,
    Shop,
}

/// Direkte Belohnung (ohne Pickup auf dem Boden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Reward {
    /// Heilt so viele halbe Herzen.
    Heal(i32),
    Coins(u32),
    Keys(u32),
    Bombs(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

/// Stabiler Bezeichner eines Items, z. B. `"onion"`.
///
/// Newtype statt nacktem `String`: Der Compiler verhindert, dass man
/// versehentlich einen Item-Namen („Zwiebel“) statt einer ID übergibt.
/// `serde(transparent)`: in Dateien steht einfach `"onion"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ItemId(pub String);

impl ItemId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ItemId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ItemDef {
    pub id: ItemId,
    pub name: String,
    pub description: String,
    /// Leer = kommt in keinem Pool vor (z. B. das Ersatz-Item).
    pub pools: Vec<Pool>,
    /// Preis im Shop (Münzen).
    pub price: u32,
    pub effects: Vec<Effect>,
}

/// ID des Ersatz-Items, das kommt, wenn ein Pool leer ist. Muss in jeder Db stehen.
pub const FALLBACK_ID: &str = "breakfast";

// --- Datenbank ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ItemDb {
    pub items: Vec<ItemDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemDbError {
    DuplicateId(String),
    MissingFallback,
    NoPool(String),
    NoEffects(String),
}

impl fmt::Display for ItemDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemDbError::DuplicateId(id) => write!(f, "Item-ID '{id}' kommt mehrfach vor"),
            ItemDbError::MissingFallback => write!(f, "Ersatz-Item '{FALLBACK_ID}' fehlt"),
            ItemDbError::NoPool(id) => write!(f, "Item '{id}' ist in keinem Pool"),
            ItemDbError::NoEffects(id) => write!(f, "Item '{id}' hat keine Effekte"),
        }
    }
}

impl std::error::Error for ItemDbError {}

impl ItemDb {
    pub fn get(&self, id: &ItemId) -> Option<&ItemDef> {
        self.items.iter().find(|i| &i.id == id)
    }

    pub fn get_str(&self, id: &str) -> Option<&ItemDef> {
        self.items.iter().find(|i| i.id.as_str() == id)
    }

    /// IDs → Definitionen. Unbekannte IDs (z. B. nach dem Löschen aus der
    /// Datei) werden übersprungen statt das Spiel abstürzen zu lassen.
    pub fn resolve<'a>(&'a self, ids: &[ItemId]) -> Vec<&'a ItemDef> {
        ids.iter().filter_map(|id| self.get(id)).collect()
    }

    /// Prüft, ob die Datenbank benutzbar ist. Geladene Dateien werden erst
    /// übernommen, wenn diese Prüfung besteht.
    pub fn validate(&self) -> Result<(), ItemDbError> {
        let mut seen = std::collections::BTreeSet::new();
        for item in &self.items {
            if !seen.insert(item.id.as_str()) {
                return Err(ItemDbError::DuplicateId(item.id.0.clone()));
            }
            if item.effects.is_empty() {
                return Err(ItemDbError::NoEffects(item.id.0.clone()));
            }
            if item.pools.is_empty() && item.id.as_str() != FALLBACK_ID {
                return Err(ItemDbError::NoPool(item.id.0.clone()));
            }
        }
        if self.get_str(FALLBACK_ID).is_none() {
            return Err(ItemDbError::MissingFallback);
        }
        Ok(())
    }

    /// Die eingebauten Items.
    pub fn builtin() -> Self {
        // Vollständiger Pfad nötig: `use Effect::*` bringt auch die Variante
        // `Effect::Stat` in den Scope, ein bloßes `use Stat::*` wäre mehrdeutig.
        use crate::stats::Stat::*;
        use Effect::*;
        use Pool::*;

        let add = |stat, v| Effect::Stat(Modifier::add(stat, v));
        let mul = |stat, v| Effect::Stat(Modifier::mul(stat, v));
        let item = |id: &str, name: &str, desc: &str, pools: &[Pool], price, effects| ItemDef {
            id: ItemId::from(id),
            name: name.to_string(),
            description: desc.to_string(),
            pools: pools.to_vec(),
            price,
            effects,
        };

        let items = vec![
            item(
                "onion",
                "Zwiebel",
                "Feuerrate +30 %",
                &[Treasure, Shop],
                15,
                vec![mul(FireRate, 1.3)],
            ),
            item(
                "whetstone",
                "Wetzstein",
                "Schaden +1",
                &[Treasure, Shop],
                15,
                vec![add(Damage, 1.0)],
            ),
            item(
                "steak",
                "Steak",
                "+1 Herz, Schaden +0,5",
                &[Treasure, Boss],
                15,
                vec![MaxHealth(2), add(Damage, 0.5)],
            ),
            item(
                "sneakers",
                "Laufschuhe",
                "Tempo +1",
                &[Treasure, Shop],
                10,
                vec![add(MoveSpeed, 1.0)],
            ),
            item(
                "spyglass",
                "Fernrohr",
                "Reichweite +2,5, Schussgeschwindigkeit +1,5",
                &[Treasure, Shop],
                10,
                vec![add(Range, 2.5), add(ShotSpeed, 1.5)],
            ),
            item(
                "trident",
                "Dreizack",
                "Drei Schüsse pro Salve, Feuerrate -30 %",
                &[Treasure],
                20,
                vec![ExtraShots(2), mul(FireRate, 0.7)],
            ),
            item(
                "drill",
                "Bohrer",
                "Schüsse durchschlagen Gegner, Schaden -10 %",
                &[Treasure],
                20,
                vec![Piercing, mul(Damage, 0.9)],
            ),
            item(
                "magnet_eye",
                "Magnetauge",
                "Schüsse suchen ihr Ziel, Schussgeschwindigkeit -1",
                &[Treasure],
                20,
                vec![Homing, add(ShotSpeed, -1.0)],
            ),
            item(
                "gold_rush",
                "Goldrausch",
                "Feuerrate +3 % pro Münze",
                &[Treasure, Shop],
                10,
                vec![PerCoins {
                    per: 1,
                    modifier: Modifier::mul(FireRate, 1.03),
                }],
            ),
            item(
                "piggy_bank",
                "Sparschwein",
                "+1 Münze pro geräumtem Raum",
                &[Treasure, Shop],
                5,
                vec![OnRoomClear(Reward::Coins(1))],
            ),
            item(
                "payday",
                "Zahltag",
                "25 % Chance auf eine Münze pro Kill",
                &[Treasure, Shop],
                10,
                vec![OnKill {
                    chance: 0.25,
                    reward: Reward::Coins(1),
                }],
            ),
            item(
                "berserker",
                "Berserker",
                "Schaden +0,4 pro fehlendem halben Herz",
                &[Treasure, Boss],
                20,
                vec![PerMissingHealth(Modifier::add(Damage, 0.4))],
            ),
            item(
                "vampire_fang",
                "Vampirzahn",
                "15 % Chance auf ein halbes Herz pro Kill",
                &[Treasure, Boss],
                15,
                vec![OnKill {
                    chance: 0.15,
                    reward: Reward::Heal(1),
                }],
            ),
            item(
                "tree_of_life",
                "Lebensbaum",
                "Heilt ein halbes Herz pro geräumtem Raum",
                &[Boss],
                20,
                vec![OnRoomClear(Reward::Heal(1))],
            ),
            item(
                "lead_bullet",
                "Bleikugel",
                "Schaden +50 %, Feuerrate -15 %, Schussgeschwindigkeit -2",
                &[Treasure, Boss],
                15,
                vec![mul(Damage, 1.5), mul(FireRate, 0.85), add(ShotSpeed, -2.0)],
            ),
            item(
                "coffee",
                "Kaffee",
                "Tempo +0,7, Feuerrate +15 %, Schaden -10 %",
                &[Treasure, Shop],
                10,
                vec![add(MoveSpeed, 0.7), mul(FireRate, 1.15), mul(Damage, 0.9)],
            ),
            item(
                "golden_heart",
                "Goldenes Herz",
                "+1 Herz",
                &[Shop],
                15,
                vec![MaxHealth(2)],
            ),
            item(
                "shotgun",
                "Schrotflinte",
                "Fünf Schüsse, Reichweite -50 %, Schaden -40 %",
                &[Boss],
                25,
                vec![ExtraShots(4), mul(Range, 0.5), mul(Damage, 0.6)],
            ),
            item(
                FALLBACK_ID,
                "Frühstück",
                "+1 Herz",
                &[],
                5,
                vec![MaxHealth(2)],
            ),
        ];
        Self { items }
    }
}

// --- Auswertung ---------------------------------------------------------------

/// Endgültige Stats aus Grundwerten, Items und Situation.
pub fn compute_stats(items: &[&ItemDef], coins: u32, missing_health: u32) -> Stats {
    let modifiers = items
        .iter()
        .flat_map(|i| i.effects.iter())
        .filter_map(|e| match *e {
            Effect::Stat(m) => Some(m),
            Effect::PerCoins { per, modifier } => Some(repeat(modifier, coins / per.max(1))),
            Effect::PerMissingHealth(modifier) => Some(repeat(modifier, missing_health)),
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

fn all_effects<'a>(items: &'a [&'a ItemDef]) -> impl Iterator<Item = &'a Effect> + 'a {
    items.iter().flat_map(|i| i.effects.iter())
}

pub fn shot_pattern(items: &[&ItemDef]) -> ShotPattern {
    ShotPattern {
        count: 1 + all_effects(items)
            .map(|e| if let Effect::ExtraShots(n) = e { *n } else { 0 })
            .sum::<u32>(),
        piercing: all_effects(items).any(|e| matches!(e, Effect::Piercing)),
        homing: all_effects(items).any(|e| matches!(e, Effect::Homing)),
    }
}

/// Zusätzliche maximale Lebenspunkte (halbe Herzen).
pub fn max_health_bonus(items: &[&ItemDef]) -> i32 {
    all_effects(items)
        .map(|e| if let Effect::MaxHealth(n) = e { *n } else { 0 })
        .sum()
}

pub fn room_clear_rewards(items: &[&ItemDef]) -> Vec<Reward> {
    all_effects(items)
        .filter_map(|e| {
            if let Effect::OnRoomClear(r) = e {
                Some(*r)
            } else {
                None
            }
        })
        .collect()
}

/// Würfelt alle Kill-Effekte aus.
pub fn kill_rewards(items: &[&ItemDef], rng: &mut Rng) -> Vec<Reward> {
    all_effects(items)
        .filter_map(|e| match e {
            Effect::OnKill { chance, reward } => rng.chance(*chance).then_some(*reward),
            _ => None,
        })
        .collect()
}

// --- Pools --------------------------------------------------------------------

/// Noch nicht vergebene Items eines Runs. Jedes Item erscheint höchstens einmal.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ItemPools {
    remaining: Vec<ItemId>,
}

impl ItemPools {
    /// Alle Items der Db außer dem Ersatz-Item und den gesperrten
    /// (siehe `meta::locked_items`).
    pub fn new(db: &ItemDb, locked: &[&str]) -> Self {
        Self {
            remaining: db
                .items
                .iter()
                .map(|i| i.id.clone())
                .filter(|id| id.as_str() != FALLBACK_ID && !locked.contains(&id.as_str()))
                .collect(),
        }
    }

    /// Zieht ein Item aus `pool` und entfernt es aus allen Pools.
    /// Ist nichts mehr da, kommt das Ersatz-Item.
    pub fn draw(&mut self, db: &ItemDb, pool: Pool, rng: &mut Rng) -> ItemId {
        let candidates: Vec<usize> = self
            .remaining
            .iter()
            .enumerate()
            .filter(|(_, id)| db.get(id).is_some_and(|i| i.pools.contains(&pool)))
            .map(|(idx, _)| idx)
            .collect();
        match rng.choose(&candidates) {
            // `swap_remove` ist O(1); die Reihenfolge in `remaining` ist egal.
            Some(&idx) => self.remaining.swap_remove(idx),
            None => ItemId::from(FALLBACK_ID),
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

    fn db() -> ItemDb {
        ItemDb::builtin()
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// Hilfsfunktion: Items per ID aus einer Db holen.
    fn pick<'a>(db: &'a ItemDb, ids: &[&str]) -> Vec<&'a ItemDef> {
        ids.iter()
            .map(|id| {
                db.get_str(id)
                    .unwrap_or_else(|| panic!("Item '{id}' fehlt"))
            })
            .collect()
    }

    #[test]
    fn builtin_db_is_valid() {
        let db = db();
        assert_eq!(db.validate(), Ok(()));
        assert!(db.items.len() >= 16);
    }

    #[test]
    fn validation_catches_errors() {
        let mut dup = db();
        dup.items.push(dup.items[0].clone());
        assert!(matches!(dup.validate(), Err(ItemDbError::DuplicateId(_))));

        let mut no_fallback = db();
        no_fallback.items.retain(|i| i.id.as_str() != FALLBACK_ID);
        assert_eq!(no_fallback.validate(), Err(ItemDbError::MissingFallback));

        let mut no_pool = db();
        no_pool.items[0].pools.clear();
        assert!(matches!(no_pool.validate(), Err(ItemDbError::NoPool(_))));

        let mut no_effects = db();
        no_effects.items[0].effects.clear();
        assert!(matches!(
            no_effects.validate(),
            Err(ItemDbError::NoEffects(_))
        ));
    }

    #[test]
    fn every_pool_has_items() {
        let db = db();
        for pool in [Pool::Treasure, Pool::Boss, Pool::Shop] {
            assert!(db.items.iter().filter(|i| i.pools.contains(&pool)).count() >= 4);
        }
    }

    #[test]
    fn resolve_skips_unknown_ids() {
        let db = db();
        let ids = [ItemId::from("onion"), ItemId::from("gibts_nicht")];
        let resolved = db.resolve(&ids);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "Zwiebel");
    }

    #[test]
    fn no_items_means_base_stats() {
        assert_eq!(compute_stats(&[], 10, 3), Stats::BASE);
    }

    #[test]
    fn gold_rush_scales_with_coins() {
        let db = db();
        let items = pick(&db, &["gold_rush"]);
        let none = compute_stats(&items, 0, 0).fire_rate;
        let ten = compute_stats(&items, 10, 0).fire_rate;
        assert!(approx(none, Stats::BASE.fire_rate));
        assert!(approx(ten, Stats::BASE.fire_rate * 1.03f32.powi(10)));
    }

    #[test]
    fn berserker_scales_with_missing_health() {
        let db = db();
        let d = compute_stats(&pick(&db, &["berserker"]), 0, 3).damage;
        assert!(approx(d, Stats::BASE.damage + 1.2));
    }

    #[test]
    fn trident_and_drill_combine() {
        let db = db();
        let p = shot_pattern(&pick(&db, &["trident", "drill"]));
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
        let db = db();
        assert_eq!(shot_pattern(&pick(&db, &["trident", "shotgun"])).count, 7);
    }

    #[test]
    fn coin_engine_synergy() {
        let db = db();
        let items = pick(&db, &["gold_rush", "piggy_bank", "payday"]);
        assert_eq!(room_clear_rewards(&items), vec![Reward::Coins(1)]);
        let mut rng = Rng::from_seed(9);
        let coins: usize = (0..1000)
            .map(|_| kill_rewards(&items, &mut rng).len())
            .sum();
        assert!((200..300).contains(&coins), "Zahltag ~25 %: {coins}");
    }

    #[test]
    fn max_health_bonus_sums() {
        let db = db();
        assert_eq!(max_health_bonus(&pick(&db, &["steak", "golden_heart"])), 4);
    }

    #[test]
    fn pools_never_repeat_and_fall_back() {
        let db = db();
        let mut pools = ItemPools::new(&db, &[]);
        let mut rng = Rng::from_seed(4);
        let mut seen = BTreeSet::new();
        for _ in 0..db.items.len() + 5 {
            let id = pools.draw(&db, Pool::Treasure, &mut rng);
            if id.as_str() != FALLBACK_ID {
                assert!(seen.insert(id.clone()), "{id} doppelt");
                assert!(db.get(&id).unwrap().pools.contains(&Pool::Treasure));
            }
        }
        assert_eq!(
            pools.draw(&db, Pool::Treasure, &mut rng).as_str(),
            FALLBACK_ID
        );
    }

    #[test]
    fn locked_items_never_drop() {
        let db = db();
        let mut pools = ItemPools::new(&db, &["shotgun", "steak"]);
        let mut rng = Rng::from_seed(8);
        for _ in 0..100 {
            let id = pools.draw(&db, Pool::Boss, &mut rng);
            assert!(id.as_str() != "shotgun" && id.as_str() != "steak", "{id}");
        }
    }

    #[test]
    fn drawing_from_one_pool_removes_from_all() {
        let db = db();
        let mut pools = ItemPools::new(&db, &[]);
        let mut rng = Rng::from_seed(2);
        let before = pools.remaining();
        let item = pools.draw(&db, Pool::Shop, &mut rng);
        assert_eq!(pools.remaining(), before - 1);
        for _ in 0..50 {
            assert_ne!(pools.draw(&db, Pool::Treasure, &mut rng), item);
        }
    }

    #[test]
    fn items_removed_from_db_are_skipped_by_pools() {
        // Hot-Reload: Eine Id im Pool, die es in der neuen Datei nicht mehr gibt,
        // darf nie gezogen werden.
        let full = db();
        let mut pools = ItemPools::new(&full, &[]);
        let mut small = db();
        small
            .items
            .retain(|i| i.id.as_str() == "onion" || i.id.as_str() == FALLBACK_ID);
        let mut rng = Rng::from_seed(3);
        assert_eq!(
            pools.draw(&small, Pool::Treasure, &mut rng).as_str(),
            "onion"
        );
        assert_eq!(
            pools.draw(&small, Pool::Treasure, &mut rng).as_str(),
            FALLBACK_ID
        );
    }
}
