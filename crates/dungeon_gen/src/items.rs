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
//!
//! Seit der Spieltiefe-Erweiterung gibt es außerdem:
//! - **Treffereffekte** (Gift, Frost, Abpraller, kritische Treffer),
//! - **aktive Items** (Taste Q, laden sich durch geräumte Räume auf),
//! - **Synergien**: Bonus-Effekte, wenn man bestimmte Items zusammen besitzt.
//!
//! Alle Auswertungen laufen über ein [`Loadout`]: die Items des Spielers
//! plus alle dadurch aktiven Synergien.

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
    /// Schüsse vergiften mit Wahrscheinlichkeit `chance`: Schaden pro Sekunde.
    Poison {
        chance: f64,
        damage_per_sec: f32,
        secs: f32,
    },
    /// Schüsse frieren Gegner mit Wahrscheinlichkeit `chance` kurz ein.
    Freeze { chance: f64, secs: f32 },
    /// Schüsse prallen so oft von Wänden ab.
    Bounce(u32),
    /// Kritischer Treffer: Schaden × `multiplier` mit Wahrscheinlichkeit `chance`.
    Crit { chance: f64, multiplier: f32 },
    /// Modifier wirkt einmal pro besessenem Item.
    PerItem(Modifier),
    /// Macht das Item zu einem **aktiven** Item: Taste Q, braucht `charges`
    /// geräumte Räume zum Aufladen.
    Active { kind: ActiveKind, charges: u32 },
}

/// Was ein aktives Item beim Benutzen tut.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ActiveKind {
    /// Heilt so viele halbe Herzen.
    Heal(i32),
    /// Schaden an allen Gegnern im Raum, mit Rückstoß.
    Shockwave { damage: f32 },
    /// Friert alle Gegner ein und löscht feindliche Schüsse.
    FreezeAll { secs: f32 },
    /// Unverwundbar für einige Sekunden.
    Shield { secs: f32 },
    /// Sofort Münzen.
    Coins(u32),
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

impl ItemDef {
    /// `Some`, wenn dies ein aktives Item ist.
    pub fn active(&self) -> Option<(ActiveKind, u32)> {
        self.effects.iter().find_map(|e| match *e {
            Effect::Active { kind, charges } => Some((kind, charges)),
            _ => None,
        })
    }

    /// Zusätzliche maximale Lebenspunkte (halbe Herzen), auch negativ.
    pub fn max_health_bonus(&self) -> i32 {
        self.effects
            .iter()
            .map(|e| if let Effect::MaxHealth(n) = e { *n } else { 0 })
            .sum()
    }
}

/// Bonus für eine Item-Kombination. Aktiv, sobald man **alle** `requires`
/// besitzt (aktive Items zählen mit).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Synergy {
    pub name: String,
    pub description: String,
    pub requires: Vec<ItemId>,
    pub effects: Vec<Effect>,
}

/// Aktuelle Version des Item-Formats. Ältere `items.ron`-Dateien werden
/// von der Spiel-Crate durch die eingebauten Items ersetzt (mit Sicherung).
pub const ITEMS_VERSION: u32 = 2;

/// Dateien ohne `version`-Feld stammen aus der ersten Fassung.
#[cfg(feature = "serde")]
fn legacy_version() -> u32 {
    1
}

/// ID des Ersatz-Items, das kommt, wenn ein Pool leer ist. Muss in jeder Db stehen.
pub const FALLBACK_ID: &str = "breakfast";

// --- Datenbank ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ItemDb {
    #[cfg_attr(feature = "serde", serde(default = "legacy_version"))]
    pub version: u32,
    pub items: Vec<ItemDef>,
    /// `default`: Fehlt das Feld in der Datei, gibt es eben keine Synergien.
    #[cfg_attr(feature = "serde", serde(default))]
    pub synergies: Vec<Synergy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemDbError {
    DuplicateId(String),
    MissingFallback,
    NoPool(String),
    NoEffects(String),
    /// Synergie verweist auf ein unbekanntes Item.
    UnknownSynergyItem {
        synergy: String,
        item: String,
    },
    /// Aktives Item braucht mindestens eine Ladung.
    ZeroCharges(String),
}

impl fmt::Display for ItemDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemDbError::DuplicateId(id) => write!(f, "Item-ID '{id}' kommt mehrfach vor"),
            ItemDbError::MissingFallback => write!(f, "Ersatz-Item '{FALLBACK_ID}' fehlt"),
            ItemDbError::NoPool(id) => write!(f, "Item '{id}' ist in keinem Pool"),
            ItemDbError::NoEffects(id) => write!(f, "Item '{id}' hat keine Effekte"),
            ItemDbError::UnknownSynergyItem { synergy, item } => {
                write!(f, "Synergie '{synergy}' braucht unbekanntes Item '{item}'")
            }
            ItemDbError::ZeroCharges(id) => write!(f, "Aktives Item '{id}' hat 0 Ladungen"),
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

    /// Synergien, die mit diesen Items aktiv sind.
    pub fn synergies_for<'a>(&'a self, ids: &[ItemId]) -> Vec<&'a Synergy> {
        self.synergies
            .iter()
            .filter(|s| s.requires.iter().all(|r| ids.contains(r)))
            .collect()
    }

    /// Passive Items plus aktive Synergien – die Grundlage aller Auswertungen.
    ///
    /// `active` ist das aktive Item (falls vorhanden): Es wirkt nicht passiv,
    /// zählt aber für Synergien mit.
    pub fn loadout<'a>(&'a self, passive: &[ItemId], active: Option<&ItemId>) -> Loadout<'a> {
        let mut all: Vec<ItemId> = passive.to_vec();
        all.extend(active.cloned());
        Loadout {
            items: self.resolve(passive),
            synergies: self.synergies_for(&all),
        }
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
            if item.active().is_some_and(|(_, charges)| charges == 0) {
                return Err(ItemDbError::ZeroCharges(item.id.0.clone()));
            }
        }
        for synergy in &self.synergies {
            if let Some(missing) = synergy.requires.iter().find(|r| self.get(r).is_none()) {
                return Err(ItemDbError::UnknownSynergyItem {
                    synergy: synergy.name.clone(),
                    item: missing.0.clone(),
                });
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
            // --- Treffereffekte ---
            item(
                "venom_gland",
                "Giftdrüse",
                "30 % Chance: Gift (1 Schaden/s, 3 s)",
                &[Treasure, Shop],
                15,
                vec![Poison {
                    chance: 0.3,
                    damage_per_sec: 1.0,
                    secs: 3.0,
                }],
            ),
            item(
                "frost_crystal",
                "Frostkristall",
                "15 % Chance: Gegner 1 s eingefroren",
                &[Treasure],
                15,
                vec![Freeze {
                    chance: 0.15,
                    secs: 1.0,
                }],
            ),
            item(
                "rubber_ball",
                "Gummiball",
                "Schüsse prallen zweimal von Wänden ab",
                &[Treasure, Shop],
                15,
                vec![Bounce(2), add(Range, 1.0)],
            ),
            item(
                "lucky_clover",
                "Glücksklee",
                "12 % Chance auf dreifachen Schaden",
                &[Treasure, Shop],
                15,
                vec![Crit {
                    chance: 0.12,
                    multiplier: 3.0,
                }],
            ),
            item(
                "collector",
                "Sammlerwahn",
                "Schaden +0,2 pro Item",
                &[Treasure, Boss],
                20,
                vec![PerItem(Modifier::add(Damage, 0.2))],
            ),
            item(
                "glass_cannon",
                "Glaskanone",
                "Schaden ×2, aber -1 Herz",
                &[Boss],
                25,
                vec![mul(Damage, 2.0), MaxHealth(-2)],
            ),
            item(
                "hourglass",
                "Sanduhr",
                "Feuerrate +20 %, Schussgeschwindigkeit -1",
                &[Treasure],
                10,
                vec![mul(FireRate, 1.2), add(ShotSpeed, -1.0)],
            ),
            item(
                "iron_skin",
                "Eisenhaut",
                "+1 Herz, Tempo -0,5",
                &[Shop, Boss],
                15,
                vec![MaxHealth(2), add(MoveSpeed, -0.5)],
            ),
            item(
                "key_ring",
                "Schlüsselbund",
                "6 % Chance auf einen Schlüssel pro Kill",
                &[Shop],
                10,
                vec![OnKill {
                    chance: 0.06,
                    reward: Reward::Keys(1),
                }],
            ),
            item(
                "bomb_bag",
                "Bombenbeutel",
                "8 % Chance auf eine Bombe pro Kill",
                &[Treasure, Shop],
                10,
                vec![OnKill {
                    chance: 0.08,
                    reward: Reward::Bombs(1),
                }],
            ),
            // --- Aktive Items (Taste Q) ---
            item(
                "salve",
                "Heilsalbe",
                "Aktiv (3 Räume): heilt ein Herz",
                &[Treasure, Shop],
                15,
                vec![Active {
                    kind: ActiveKind::Heal(2),
                    charges: 3,
                }],
            ),
            item(
                "thunder_drum",
                "Donnertrommel",
                "Aktiv (3 Räume): 8 Schaden an allen Gegnern",
                &[Treasure, Boss],
                20,
                vec![Active {
                    kind: ActiveKind::Shockwave { damage: 8.0 },
                    charges: 3,
                }],
            ),
            item(
                "frost_horn",
                "Frosthorn",
                "Aktiv (2 Räume): friert alle Gegner 3 s ein",
                &[Treasure],
                20,
                vec![Active {
                    kind: ActiveKind::FreezeAll { secs: 3.0 },
                    charges: 2,
                }],
            ),
            item(
                "aegis",
                "Ägide",
                "Aktiv (2 Räume): 4 s unverwundbar",
                &[Shop, Boss],
                20,
                vec![Active {
                    kind: ActiveKind::Shield { secs: 4.0 },
                    charges: 2,
                }],
            ),
            item(
                "gold_purse",
                "Goldbeutel",
                "Aktiv (3 Räume): +6 Münzen",
                &[Treasure],
                10,
                vec![Active {
                    kind: ActiveKind::Coins(6),
                    charges: 3,
                }],
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

        let synergy = |name: &str, desc: &str, requires: &[&str], effects| Synergy {
            name: name.to_string(),
            description: desc.to_string(),
            requires: requires.iter().map(|r| ItemId::from(*r)).collect(),
            effects,
        };
        let synergies = vec![
            synergy(
                "Giftpfeile",
                "Durchschlagende Schüsse vergiften öfter",
                &["venom_gland", "drill"],
                vec![Poison {
                    chance: 0.3,
                    damage_per_sec: 1.5,
                    secs: 3.0,
                }],
            ),
            synergy(
                "Eissturm",
                "Frost-Chance +20 %, länger eingefroren",
                &["frost_crystal", "frost_horn"],
                vec![Freeze {
                    chance: 0.2,
                    secs: 1.5,
                }],
            ),
            synergy(
                "Querschläger",
                "Ein Abpraller mehr, Schaden +0,5",
                &["rubber_ball", "trident"],
                vec![Bounce(1), add(Damage, 0.5)],
            ),
            synergy(
                "Glückspilz",
                "Krit-Chance +10 %",
                &["lucky_clover", "gold_rush"],
                vec![Crit {
                    chance: 0.1,
                    multiplier: 3.0,
                }],
            ),
            synergy(
                "Blutrausch",
                "Schaden +1",
                &["berserker", "vampire_fang"],
                vec![add(Damage, 1.0)],
            ),
            synergy(
                "Goldader",
                "Noch eine Münze pro geräumtem Raum",
                &["piggy_bank", "payday"],
                vec![OnRoomClear(Reward::Coins(1))],
            ),
            synergy(
                "Scharfschütze",
                "Suchende, durchschlagende Schüsse: Schaden +50 %",
                &["magnet_eye", "drill"],
                vec![mul(Damage, 1.5)],
            ),
        ];
        Self {
            version: ITEMS_VERSION,
            items,
            synergies,
        }
    }
}

// --- Auswertung ---------------------------------------------------------------

/// Alles, was passiv wirkt: Items plus aktive Synergien.
///
/// Die Auswertungsfunktionen unten sehen nur `effects()` – für sie ist
/// egal, ob ein Effekt von einem Item oder einer Synergie stammt.
#[derive(Debug, Clone, Default)]
pub struct Loadout<'a> {
    pub items: Vec<&'a ItemDef>,
    pub synergies: Vec<&'a Synergy>,
}

impl<'a> Loadout<'a> {
    /// Nur Items, ohne Synergien (vor allem für Tests).
    pub fn from_items(items: Vec<&'a ItemDef>) -> Self {
        Self {
            items,
            synergies: Vec::new(),
        }
    }

    /// Alle Effekte hintereinander. `impl Iterator + '_`: Der Iterator leiht
    /// sich `self` nur aus – es wird nichts kopiert oder gesammelt.
    pub fn effects(&self) -> impl Iterator<Item = &Effect> + '_ {
        let from_items = self.items.iter().flat_map(|i| i.effects.iter());
        let from_synergies = self.synergies.iter().flat_map(|s| s.effects.iter());
        from_items.chain(from_synergies)
    }
}

/// Endgültige Stats aus Grundwerten, Items und Situation.
pub fn compute_stats(loadout: &Loadout, coins: u32, missing_health: u32) -> Stats {
    let item_count = loadout.items.len() as u32;
    let modifiers = loadout.effects().filter_map(|e| match *e {
        Effect::Stat(m) => Some(m),
        Effect::PerCoins { per, modifier } => Some(repeat(modifier, coins / per.max(1))),
        Effect::PerMissingHealth(modifier) => Some(repeat(modifier, missing_health)),
        Effect::PerItem(modifier) => Some(repeat(modifier, item_count)),
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

/// Gift auf Schüssen: Chance pro Schuss, Schaden pro Sekunde, Dauer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoisonSpec {
    pub chance: f64,
    pub damage_per_sec: f32,
    pub secs: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreezeSpec {
    pub chance: f64,
    pub secs: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CritSpec {
    pub chance: f64,
    pub multiplier: f32,
}

/// Wie geschossen wird. Nur `PartialEq` (kein `Eq`), weil `f32`/`f64`
/// wegen `NaN` keine vollständige Gleichheit haben.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotPattern {
    pub count: u32,
    pub piercing: bool,
    pub homing: bool,
    pub bounces: u32,
    pub poison: Option<PoisonSpec>,
    pub freeze: Option<FreezeSpec>,
    pub crit: Option<CritSpec>,
}

/// Mehrere Quellen desselben Effekts: Chancen addieren sich (max. 100 %),
/// Stärke und Dauer nimmt man vom Besten.
pub fn shot_pattern(loadout: &Loadout) -> ShotPattern {
    let mut p = ShotPattern {
        count: 1,
        piercing: false,
        homing: false,
        bounces: 0,
        poison: None,
        freeze: None,
        crit: None,
    };
    for effect in loadout.effects() {
        match *effect {
            Effect::ExtraShots(n) => p.count += n,
            Effect::Piercing => p.piercing = true,
            Effect::Homing => p.homing = true,
            Effect::Bounce(n) => p.bounces += n,
            Effect::Poison {
                chance,
                damage_per_sec,
                secs,
            } => {
                // `get_or_insert` legt einen „leeren“ Wert an, falls noch keiner da ist.
                let spec = p.poison.get_or_insert(PoisonSpec {
                    chance: 0.0,
                    damage_per_sec: 0.0,
                    secs: 0.0,
                });
                spec.chance = (spec.chance + chance).min(1.0);
                spec.damage_per_sec = spec.damage_per_sec.max(damage_per_sec);
                spec.secs = spec.secs.max(secs);
            }
            Effect::Freeze { chance, secs } => {
                let spec = p.freeze.get_or_insert(FreezeSpec {
                    chance: 0.0,
                    secs: 0.0,
                });
                spec.chance = (spec.chance + chance).min(1.0);
                spec.secs = spec.secs.max(secs);
            }
            Effect::Crit { chance, multiplier } => {
                let spec = p.crit.get_or_insert(CritSpec {
                    chance: 0.0,
                    multiplier: 1.0,
                });
                spec.chance = (spec.chance + chance).min(1.0);
                spec.multiplier = spec.multiplier.max(multiplier);
            }
            _ => {}
        }
    }
    p
}

pub fn room_clear_rewards(loadout: &Loadout) -> Vec<Reward> {
    loadout
        .effects()
        .filter_map(|e| match e {
            Effect::OnRoomClear(r) => Some(*r),
            _ => None,
        })
        .collect()
}

/// Würfelt alle Kill-Effekte aus.
pub fn kill_rewards(loadout: &Loadout, rng: &mut Rng) -> Vec<Reward> {
    loadout
        .effects()
        .filter_map(|e| match e {
            Effect::OnKill { chance, reward } => rng.chance(*chance).then_some(*reward),
            _ => None,
        })
        .collect()
}

/// Treffereffekte eines einzelnen Schusses, beim Abfeuern ausgewürfelt.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ShotRoll {
    /// Schadensfaktor (1.0 oder Krit-Multiplikator).
    pub damage_factor: f32,
    pub crit: bool,
    pub poison: Option<(f32, f32)>,
    pub freeze: Option<f32>,
}

/// Würfelt beim Abfeuern aus, was dieser Schuss kann. So sieht man es dem
/// Schuss schon im Flug an (Farbe), und Treffer brauchen keinen Zufall mehr.
pub fn roll_shot(pattern: &ShotPattern, rng: &mut Rng) -> ShotRoll {
    let crit = pattern.crit.filter(|c| rng.chance(c.chance));
    ShotRoll {
        damage_factor: crit.map_or(1.0, |c| c.multiplier),
        crit: crit.is_some(),
        poison: pattern
            .poison
            .filter(|p| rng.chance(p.chance))
            .map(|p| (p.damage_per_sec, p.secs)),
        freeze: pattern
            .freeze
            .filter(|f| rng.chance(f.chance))
            .map(|f| f.secs),
    }
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

    /// Hilfsfunktion: Loadout (mit Synergien) aus Item-IDs.
    fn pick<'a>(db: &'a ItemDb, ids: &[&str]) -> Loadout<'a> {
        for id in ids {
            assert!(db.get_str(id).is_some(), "Item '{id}' fehlt");
        }
        let ids: Vec<ItemId> = ids.iter().map(|i| ItemId::from(*i)).collect();
        db.loadout(&ids, None)
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
        assert_eq!(compute_stats(&Loadout::default(), 10, 3), Stats::BASE);
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
        assert_eq!((p.count, p.piercing, p.homing), (3, true, false));
        assert_eq!(p.poison, None);
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
        // Sparschwein + Zahltag = Synergie „Goldader“: zwei Münzen pro Raum.
        assert_eq!(
            room_clear_rewards(&items),
            vec![Reward::Coins(1), Reward::Coins(1)]
        );
        let mut rng = Rng::from_seed(9);
        let coins: usize = (0..1000)
            .map(|_| kill_rewards(&items, &mut rng).len())
            .sum();
        assert!((200..300).contains(&coins), "Zahltag ~25 %: {coins}");
    }

    #[test]
    fn max_health_bonus_sums() {
        let db = db();
        let bonus: i32 = ["steak", "golden_heart", "glass_cannon"]
            .iter()
            .map(|id| db.get_str(id).unwrap().max_health_bonus())
            .sum();
        assert_eq!(bonus, 2);
    }

    #[test]
    fn synergies_need_all_items_and_can_use_active() {
        let db = db();
        assert!(pick(&db, &["rubber_ball"]).synergies.is_empty());
        let both = pick(&db, &["rubber_ball", "trident"]);
        assert_eq!(both.synergies.len(), 1);
        assert_eq!(shot_pattern(&both).bounces, 3);
        // Frosthorn ist aktiv – zählt trotzdem für „Eissturm“.
        let passive = [ItemId::from("frost_crystal")];
        let horn = ItemId::from("frost_horn");
        let lo = db.loadout(&passive, Some(&horn));
        assert_eq!(lo.synergies[0].name, "Eissturm");
        let freeze = shot_pattern(&lo).freeze.unwrap();
        assert!((freeze.chance - 0.35).abs() < 1e-9 && freeze.secs == 1.5);
        // Das aktive Item selbst wirkt nicht passiv.
        assert_eq!(lo.items.len(), 1);
    }

    #[test]
    fn effect_chances_stack_and_cap() {
        let db = db();
        let p = shot_pattern(&pick(&db, &["venom_gland", "drill"]));
        let poison = p.poison.unwrap();
        assert!((poison.chance - 0.6).abs() < 1e-9);
        assert_eq!(poison.damage_per_sec, 1.5);
        let crit = shot_pattern(&pick(&db, &["lucky_clover", "gold_rush"]))
            .crit
            .unwrap();
        assert!((crit.chance - 0.22).abs() < 1e-9);
    }

    #[test]
    fn collector_counts_items() {
        let db = db();
        let lo = pick(&db, &["collector", "onion", "sneakers", "coffee"]);
        let d = compute_stats(&lo, 0, 0).damage;
        // (2,0 + 4 × 0,2) × 0,9 (Kaffee)
        assert!(approx(d, (Stats::BASE.damage + 0.8) * 0.9), "{d}");
    }

    #[test]
    fn rolled_shots_follow_chances() {
        let db = db();
        let pattern = shot_pattern(&pick(&db, &["lucky_clover", "venom_gland"]));
        let mut rng = Rng::from_seed(21);
        let rolls: Vec<ShotRoll> = (0..10_000).map(|_| roll_shot(&pattern, &mut rng)).collect();
        let crits = rolls.iter().filter(|r| r.crit).count();
        let poisons = rolls.iter().filter(|r| r.poison.is_some()).count();
        assert!((1_000..1_400).contains(&crits), "{crits}");
        assert!((2_700..3_300).contains(&poisons), "{poisons}");
        assert!(
            rolls
                .iter()
                .filter(|r| r.crit)
                .all(|r| r.damage_factor == 3.0)
        );
        assert!(rolls.iter().all(|r| r.freeze.is_none()));
    }

    #[test]
    fn actives_are_recognized() {
        let db = db();
        let actives: Vec<_> = db.items.iter().filter(|i| i.active().is_some()).collect();
        assert_eq!(actives.len(), 5);
        assert_eq!(
            db.get_str("salve").unwrap().active(),
            Some((ActiveKind::Heal(2), 3))
        );
        assert!(db.get_str("onion").unwrap().active().is_none());
    }

    #[test]
    fn validation_checks_synergies_and_charges() {
        let mut bad = db();
        bad.synergies[0].requires.push(ItemId::from("gibts_nicht"));
        assert!(matches!(
            bad.validate(),
            Err(ItemDbError::UnknownSynergyItem { .. })
        ));
        let mut zero = db();
        let salve = zero
            .items
            .iter_mut()
            .find(|i| i.id.as_str() == "salve")
            .unwrap();
        salve.effects = vec![Effect::Active {
            kind: ActiveKind::Heal(1),
            charges: 0,
        }];
        assert_eq!(
            zero.validate(),
            Err(ItemDbError::ZeroCharges("salve".into()))
        );
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
