//! Spielbare Charaktere: Startwerte, Start-Items und Freischaltung.
//!
//! Ein Charakter ist reine **Daten**: Er ändert die Grundwerte über dieselben
//! [`Modifier`] wie ein Item, bringt Items aus der normalen Item-Datenbank mit
//! und wird über dieselben [`Condition`]s freigeschaltet wie gesperrte Items.
//! Dadurch braucht das Spiel keine Sonderlogik je Charakter.

use crate::{
    loot::{START_BOMBS, START_KEYS},
    meta::{Condition, Profile},
    stats::{Modifier, Stat},
};

/// Welcher Charakter gespielt wird. `Default` ist der immer verfügbare Wanderer –
/// so lassen sich auch alte Spielstände ohne dieses Feld laden.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum CharacterId {
    #[default]
    Wanderer,
    Scout,
    Brute,
    Sapper,
    Alchemist,
}

/// Alles, was einen Charakter ausmacht. Nur `&'static`-Daten: Die Tabelle
/// [`CHARACTERS`] ist eine Konstante und braucht keinen Heap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterDef {
    pub id: CharacterId,
    pub name: &'static str,
    /// Kurz halten: steht im Hauptmenü in einer Zeile neben dem Freischalt-Hinweis.
    pub description: &'static str,
    /// Leben in halben Herzen.
    pub max_health: f32,
    /// Veränderungen der Grundwerte – verrechnet wie Item-Modifier.
    pub modifiers: &'static [Modifier],
    /// Passive Items, die von Anfang an im Inventar liegen (Item-IDs).
    pub start_items: &'static [&'static str],
    /// Aktives Item zum Start (Item-ID).
    pub active: Option<&'static str>,
    pub coins: u32,
    pub keys: u32,
    pub bombs: u32,
    /// `None` = von Anfang an spielbar.
    pub unlock: Option<Condition>,
}

/// Alle Charaktere, in der Reihenfolge von [`CharacterId::ALL`].
pub const CHARACTERS: &[CharacterDef] = &[
    CharacterDef {
        id: CharacterId::Wanderer,
        name: "Wanderer",
        description: "3 Herzen, ein Schlüssel, eine Bombe",
        max_health: 6.0,
        modifiers: &[],
        start_items: &[],
        active: None,
        coins: 0,
        keys: START_KEYS,
        bombs: START_BOMBS,
        unlock: None,
    },
    CharacterDef {
        id: CharacterId::Scout,
        name: "Späherin",
        description: "2 Herzen, Tempo +1, Feuerrate +20 %, 2 Schlüssel",
        max_health: 4.0,
        modifiers: &[
            Modifier::add(Stat::MoveSpeed, 1.0),
            Modifier::mul(Stat::FireRate, 1.2),
        ],
        start_items: &[],
        active: None,
        coins: 0,
        keys: 2,
        bombs: START_BOMBS,
        unlock: Some(Condition::BossesDefeated(1)),
    },
    CharacterDef {
        id: CharacterId::Brute,
        name: "Rohling",
        description: "4 Herzen, Schaden +50 %, Feuerrate -30 %, Tempo -0,7",
        max_health: 8.0,
        modifiers: &[
            Modifier::mul(Stat::Damage, 1.5),
            Modifier::mul(Stat::FireRate, 0.7),
            Modifier::add(Stat::MoveSpeed, -0.7),
        ],
        start_items: &[],
        active: None,
        coins: 0,
        keys: START_KEYS,
        bombs: START_BOMBS,
        unlock: Some(Condition::TotalKills(100)),
    },
    CharacterDef {
        id: CharacterId::Sapper,
        name: "Sprengmeister",
        description: "3 Herzen, 5 Bomben, Bombenbeutel, Reichweite -15 %",
        max_health: 6.0,
        modifiers: &[Modifier::mul(Stat::Range, 0.85)],
        start_items: &["bomb_bag"],
        active: None,
        coins: 0,
        keys: START_KEYS,
        bombs: 5,
        unlock: Some(Condition::ReachDepth(2)),
    },
    CharacterDef {
        id: CharacterId::Alchemist,
        name: "Alchemistin",
        description: "2,5 Herzen, Giftdrüse, Heilsalbe, Schaden -10 %",
        max_health: 5.0,
        modifiers: &[Modifier::mul(Stat::Damage, 0.9)],
        start_items: &["venom_gland"],
        active: Some("salve"),
        coins: 0,
        keys: START_KEYS,
        bombs: START_BOMBS,
        unlock: Some(Condition::Victories(1)),
    },
];

impl CharacterId {
    pub const ALL: [CharacterId; 5] = [
        CharacterId::Wanderer,
        CharacterId::Scout,
        CharacterId::Brute,
        CharacterId::Sapper,
        CharacterId::Alchemist,
    ];

    /// Die Daten dieses Charakters.
    ///
    /// # Panics
    /// Nie – ein Test stellt sicher, dass jede ID genau einen Eintrag hat.
    pub fn def(self) -> &'static CharacterDef {
        CHARACTERS
            .iter()
            .find(|c| c.id == self)
            .expect("jeder Charakter steht in CHARACTERS")
    }

    pub fn is_unlocked(self, profile: &Profile) -> bool {
        // `is_none_or`: keine Bedingung → frei, sonst muss sie erfüllt sein.
        self.def().unlock.is_none_or(|c| c.is_met(profile))
    }

    /// Der nächste freigeschaltete Charakter nach diesem (im Kreis). Gibt es
    /// keinen anderen, bleibt es bei diesem bzw. beim Wanderer.
    pub fn next_available(self, profile: &Profile) -> CharacterId {
        let start = Self::ALL.iter().position(|&c| c == self).unwrap_or(0);
        // `cycle` wiederholt die Liste endlos; `skip`/`take` schneiden genau
        // eine Runde heraus, die hinter `self` beginnt und bei `self` endet.
        Self::ALL
            .iter()
            .copied()
            .cycle()
            .skip(start + 1)
            .take(Self::ALL.len())
            .find(|c| c.is_unlocked(profile))
            .unwrap_or_default()
    }

    /// IDs aller Items, die dieser Charakter mitbringt (passiv und aktiv).
    pub fn starting_item_ids(self) -> impl Iterator<Item = &'static str> {
        let def = self.def();
        def.start_items.iter().copied().chain(def.active)
    }
}

/// Alle Charaktere, die mit diesem Profil spielbar sind.
pub fn available(profile: &Profile) -> Vec<CharacterId> {
    CharacterId::ALL
        .into_iter()
        .filter(|c| c.is_unlocked(profile))
        .collect()
}

/// Charaktere, die `after` freischaltet, `before` aber noch nicht.
pub fn newly_unlocked(before: &Profile, after: &Profile) -> Vec<CharacterId> {
    CharacterId::ALL
        .into_iter()
        .filter(|c| !c.is_unlocked(before) && c.is_unlocked(after))
        .collect()
}

/// Der erste noch gesperrte Charakter samt seiner Bedingung – als Ziel fürs Menü.
pub fn next_locked(profile: &Profile) -> Option<(&'static CharacterDef, Condition)> {
    CHARACTERS
        .iter()
        .filter(|c| !c.id.is_unlocked(profile))
        .find_map(|c| c.unlock.map(|cond| (c, cond)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{
        items::{ItemDb, Loadout, compute_stats, compute_stats_with},
        meta::{Outcome, RunRecord},
        stats::Stats,
    };

    #[test]
    fn table_matches_ids() {
        let ids: Vec<CharacterId> = CHARACTERS.iter().map(|c| c.id).collect();
        assert_eq!(ids, CharacterId::ALL);
        let names: BTreeSet<&str> = CHARACTERS.iter().map(|c| c.name).collect();
        assert_eq!(names.len(), CHARACTERS.len(), "doppelter Name");
        for id in CharacterId::ALL {
            assert_eq!(id.def().id, id);
        }
    }

    #[test]
    fn descriptions_fit_one_menu_line() {
        for c in CHARACTERS {
            let len = c.description.chars().count();
            assert!(len <= 55, "{}: {len} Zeichen", c.name);
        }
    }

    #[test]
    fn default_character_is_always_available() {
        let fresh = Profile::default();
        assert_eq!(CharacterId::default(), CharacterId::Wanderer);
        assert_eq!(available(&fresh), vec![CharacterId::Wanderer]);
        // Allein im Kreis: bleibt beim Wanderer.
        assert_eq!(
            CharacterId::Wanderer.next_available(&fresh),
            CharacterId::Wanderer
        );
    }

    #[test]
    fn start_items_exist_and_fit_their_slot() {
        let db = ItemDb::builtin();
        for c in CHARACTERS {
            for id in c.start_items {
                let item = db
                    .get_str(id)
                    .unwrap_or_else(|| panic!("{}: '{id}'", c.name));
                assert!(item.active().is_none(), "{}: '{id}' ist aktiv", c.name);
                // Herzcontainer gehören in `max_health`, nicht in ein Start-Item:
                // Beim Start wird der Bonus eines Items nicht eingerechnet.
                assert_eq!(item.max_health_bonus(), 0, "{}: '{id}'", c.name);
            }
            if let Some(id) = c.active {
                let item = db
                    .get_str(id)
                    .unwrap_or_else(|| panic!("{}: '{id}'", c.name));
                assert!(
                    item.active().is_some(),
                    "{}: '{id}' ist nicht aktiv",
                    c.name
                );
            }
            assert!(c.max_health >= 2.0, "{}: mindestens ein Herz", c.name);
        }
    }

    #[test]
    fn modifiers_change_base_stats() {
        let nothing = Loadout::default();
        let base = compute_stats(&nothing, 0, 0);
        assert_eq!(base, Stats::BASE);
        assert_eq!(
            compute_stats_with(&nothing, 0, 0, CharacterId::Wanderer.def().modifiers),
            base
        );
        let brute = compute_stats_with(&nothing, 0, 0, CharacterId::Brute.def().modifiers);
        assert!(brute.damage > base.damage);
        assert!(brute.fire_rate < base.fire_rate);
        assert!(brute.move_speed < base.move_speed);
        let scout = compute_stats_with(&nothing, 0, 0, CharacterId::Scout.def().modifiers);
        assert!(scout.move_speed > base.move_speed);
    }

    #[test]
    fn character_and_item_modifiers_combine_in_any_order() {
        let db = ItemDb::builtin();
        // Wetzstein: Schaden +1. Rohling: Schaden ×1,5. Erst addieren, dann
        // multiplizieren – egal, woher der Modifier stammt.
        let loadout = Loadout::from_items(vec![db.get_str("whetstone").unwrap()]);
        let stats = compute_stats_with(&loadout, 0, 0, CharacterId::Brute.def().modifiers);
        assert!((stats.damage - (Stats::BASE.damage + 1.0) * 1.5).abs() < 1e-4);
    }

    #[test]
    fn unlocking_and_cycling() {
        let mut p = Profile::default();
        let before = p.clone();
        p.record(&RunRecord {
            outcome: Outcome::Death,
            depth_reached: 2,
            kills: 20,
            bosses: 1,
            items: 0,
            seconds: 60,
        });
        assert_eq!(
            newly_unlocked(&before, &p),
            vec![CharacterId::Scout, CharacterId::Sapper]
        );
        assert!(newly_unlocked(&p, &p).is_empty());

        // Wanderer → Späherin → (Rohling gesperrt) → Sprengmeister → Wanderer.
        let mut c = CharacterId::Wanderer;
        let mut seen = Vec::new();
        for _ in 0..3 {
            c = c.next_available(&p);
            seen.push(c);
        }
        assert_eq!(
            seen,
            vec![
                CharacterId::Scout,
                CharacterId::Sapper,
                CharacterId::Wanderer
            ]
        );

        let (next, cond) = next_locked(&p).unwrap();
        assert_eq!(next.id, CharacterId::Brute);
        assert_eq!(cond, Condition::TotalKills(100));
    }

    #[test]
    fn starting_item_ids_include_the_active_item() {
        let ids: Vec<_> = CharacterId::Alchemist.starting_item_ids().collect();
        assert_eq!(ids, vec!["venom_gland", "salve"]);
        assert_eq!(CharacterId::Wanderer.starting_item_ids().count(), 0);
    }
}
