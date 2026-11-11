//! Flüche und Segen: Modifikatoren, die für eine ganze Etage gelten.
//!
//! Der Modifikator einer Etage ist eine **reine Funktion** aus Seed und Tiefe
//! ([`for_floor`]). Er muss deshalb nirgends gespeichert werden – weder im
//! Run noch im Spielstand: Wer Seed und Etage kennt, kennt auch den Fluch.
//! Ein eigener RNG-Stream sorgt dafür, dass das Würfeln hier weder das Layout
//! noch die Beute desselben Seeds verschiebt.

use crate::rng::RunSeed;

/// Was auf einer Etage anders ist als sonst.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FloorModifier {
    /// Man sieht nur einen Lichtkreis um den Spieler.
    Darkness,
    /// Die Minimap bleibt leer.
    Lost,
    /// Gegner sind schneller.
    Haste,
    /// Gegner halten mehr aus.
    Toughness,
    /// Die Preisschilder im Shop sind verdeckt.
    BlindBuy,
    /// Segen: eine Münze extra für jeden geräumten Raum.
    Fortune,
}

/// Die erste Etage ist immer frei – zum Reinkommen.
pub const FIRST_CURSED_DEPTH: u32 = 2;
/// Chance auf einen Segen (wird zuerst gewürfelt).
pub const BLESSING_CHANCE: f64 = 0.10;
/// Chance auf einen Fluch, wenn es keinen Segen gab.
pub const CURSE_CHANCE: f64 = 0.55;

impl FloorModifier {
    pub const CURSES: [FloorModifier; 5] = [
        FloorModifier::Darkness,
        FloorModifier::Lost,
        FloorModifier::Haste,
        FloorModifier::Toughness,
        FloorModifier::BlindBuy,
    ];

    pub fn is_blessing(self) -> bool {
        self == FloorModifier::Fortune
    }

    pub fn name(self) -> &'static str {
        match self {
            FloorModifier::Darkness => "Dunkelheit",
            FloorModifier::Lost => "Verirrt",
            FloorModifier::Haste => "Hast",
            FloorModifier::Toughness => "Zähigkeit",
            FloorModifier::BlindBuy => "Blindkauf",
            FloorModifier::Fortune => "Glückssträhne",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            FloorModifier::Darkness => "Du siehst nur deine nächste Umgebung",
            FloorModifier::Lost => "Die Karte bleibt leer",
            FloorModifier::Haste => "Gegner sind schneller",
            FloorModifier::Toughness => "Gegner halten mehr aus",
            FloorModifier::BlindBuy => "Im Shop sind die Preise verdeckt",
            FloorModifier::Fortune => "Eine Münze extra für jeden geräumten Raum",
        }
    }

    /// „Fluch: Hast – Gegner sind schneller“ bzw. „Segen: …“.
    pub fn announcement(self) -> String {
        let kind = if self.is_blessing() { "Segen" } else { "Fluch" };
        format!("{kind}: {} – {}", self.name(), self.description())
    }

    /// Kurzform fürs HUD: „Fluch: Hast“.
    pub fn label(self) -> String {
        let kind = if self.is_blessing() { "Segen" } else { "Fluch" };
        format!("{kind}: {}", self.name())
    }

    /// Faktor auf das Tempo aller Gegner.
    pub fn enemy_speed_factor(self) -> f32 {
        match self {
            FloorModifier::Haste => 1.25,
            _ => 1.0,
        }
    }

    /// Faktor auf das Leben aller Gegner.
    pub fn enemy_health_factor(self) -> f32 {
        match self {
            FloorModifier::Toughness => 1.35,
            _ => 1.0,
        }
    }

    /// Zusätzliche Münzen für jeden geräumten Raum.
    pub fn room_clear_coins(self) -> u32 {
        match self {
            FloorModifier::Fortune => 1,
            _ => 0,
        }
    }
}

/// Der Modifikator der Etage `depth` – oder `None`, wenn sie normal ist.
pub fn for_floor(seed: RunSeed, depth: u32) -> Option<FloorModifier> {
    if depth < FIRST_CURSED_DEPTH {
        return None;
    }
    let mut rng = seed.stream("curse", u64::from(depth));
    if rng.chance(BLESSING_CHANCE) {
        return Some(FloorModifier::Fortune);
    }
    if !rng.chance(CURSE_CHANCE) {
        return None;
    }
    // `choose` liefert `Option<&T>` (leere Liste → `None`); `copied()` macht
    // daraus `Option<T>`.
    rng.choose(&FloorModifier::CURSES).copied()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn first_floor_is_never_cursed() {
        for s in 0..2_000 {
            assert_eq!(for_floor(RunSeed(s), 1), None);
            assert_eq!(for_floor(RunSeed(s), 0), None);
        }
    }

    #[test]
    fn same_seed_same_modifier() {
        for s in 0..200 {
            let seed = RunSeed(s);
            assert_eq!(for_floor(seed, 2), for_floor(seed, 2));
            assert_eq!(for_floor(seed, 3), for_floor(seed, 3));
        }
    }

    #[test]
    fn floors_of_one_run_are_rolled_independently() {
        // Wäre der Stream nicht nach Etage getrennt, hätten Etage 2 und 3
        // immer denselben Fluch.
        let differing = (0..500)
            .filter(|&s| for_floor(RunSeed(s), 2) != for_floor(RunSeed(s), 3))
            .count();
        assert!(differing > 200, "{differing}");
    }

    #[test]
    fn rates_match_the_constants() {
        const RUNS: u64 = 20_000;
        let mut counts: BTreeMap<Option<FloorModifier>, u32> = BTreeMap::new();
        for s in 0..RUNS {
            *counts.entry(for_floor(RunSeed(s), 2)).or_default() += 1;
        }
        let share = |m: Option<FloorModifier>| {
            f64::from(counts.get(&m).copied().unwrap_or(0)) / RUNS as f64
        };

        assert!((share(Some(FloorModifier::Fortune)) - BLESSING_CHANCE).abs() < 0.02);
        let expected_none = (1.0 - BLESSING_CHANCE) * (1.0 - CURSE_CHANCE);
        assert!((share(None) - expected_none).abs() < 0.02);
        // Jeder Fluch kommt vor, und zwar ungefähr gleich oft.
        let per_curse = (1.0 - BLESSING_CHANCE) * CURSE_CHANCE / 5.0;
        for curse in FloorModifier::CURSES {
            assert!(
                (share(Some(curse)) - per_curse).abs() < 0.02,
                "{curse:?}: {counts:?}"
            );
        }
    }

    #[test]
    fn only_the_matching_modifier_changes_a_value() {
        assert!(FloorModifier::Haste.enemy_speed_factor() > 1.0);
        assert_eq!(FloorModifier::Haste.enemy_health_factor(), 1.0);
        assert!(FloorModifier::Toughness.enemy_health_factor() > 1.0);
        assert_eq!(FloorModifier::Toughness.enemy_speed_factor(), 1.0);
        assert_eq!(FloorModifier::Fortune.room_clear_coins(), 1);
        for curse in FloorModifier::CURSES {
            assert!(!curse.is_blessing());
            assert_eq!(curse.room_clear_coins(), 0);
        }
        assert!(FloorModifier::Fortune.is_blessing());
    }

    #[test]
    fn texts_name_the_kind() {
        assert_eq!(FloorModifier::Haste.label(), "Fluch: Hast");
        assert!(
            FloorModifier::Fortune
                .announcement()
                .starts_with("Segen: Glückssträhne")
        );
    }
}
