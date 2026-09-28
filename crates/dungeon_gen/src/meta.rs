//! Meta-Progression: was über einen Run hinaus erhalten bleibt.
//!
//! - `RunRecord`: Ergebnis eines einzelnen Runs.
//! - `Profile`: Summe aller Runs, wird dauerhaft gespeichert.
//! - Freischaltungen: Manche Items erscheinen erst, wenn das Profil eine
//!   Bedingung erfüllt (z. B. „ersten Boss besiegt“).
//!
//! Das Speicherformat ist bewusst simpel (`schlüssel=wert` pro Zeile) und
//! **versioniert**: Ältere Dateien lassen sich später gezielt migrieren,
//! unbekannte Schlüssel werden ignoriert (vorwärtskompatibel).

use std::fmt;

/// Letzte Etage. Wer ihren Boss besiegt und die Falltür nimmt, gewinnt.
pub const MAX_DEPTH: u32 = 3;

/// Gegner werden pro Etage zäher: +20 % Leben je Etage.
pub fn enemy_health_multiplier(depth: u32) -> f32 {
    1.0 + 0.2 * depth.saturating_sub(1) as f32
}

/// Und etwas schneller: +6 % je Etage.
pub fn enemy_speed_multiplier(depth: u32) -> f32 {
    1.0 + 0.06 * depth.saturating_sub(1) as f32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Victory,
    Death,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub outcome: Outcome,
    pub depth_reached: u32,
    pub kills: u32,
    pub bosses: u32,
    pub items: u32,
    pub seconds: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub runs: u32,
    pub victories: u32,
    pub best_depth: u32,
    pub total_kills: u32,
    pub bosses_defeated: u32,
}

// --- Freischaltungen ------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    RunsPlayed(u32),
    BossesDefeated(u32),
    ReachDepth(u32),
    TotalKills(u32),
    Victories(u32),
}

impl Condition {
    pub fn is_met(self, p: &Profile) -> bool {
        match self {
            Condition::RunsPlayed(n) => p.runs >= n,
            Condition::BossesDefeated(n) => p.bosses_defeated >= n,
            Condition::ReachDepth(n) => p.best_depth >= n,
            Condition::TotalKills(n) => p.total_kills >= n,
            Condition::Victories(n) => p.victories >= n,
        }
    }

    pub fn describe(self) -> String {
        match self {
            Condition::RunsPlayed(n) => format!("{n} Runs spielen"),
            Condition::BossesDefeated(n) => format!("{n} Boss(e) besiegen"),
            Condition::ReachDepth(n) => format!("Etage {n} erreichen"),
            Condition::TotalKills(n) => format!("{n} Gegner besiegen (insgesamt)"),
            Condition::Victories(n) => format!("{n}-mal gewinnen"),
        }
    }
}

/// Item-ID → Bedingung. Items, die hier nicht stehen, sind immer verfügbar.
pub const UNLOCKS: &[(&str, Condition)] = &[
    ("shotgun", Condition::BossesDefeated(1)),
    ("golden_heart", Condition::RunsPlayed(3)),
    ("berserker", Condition::TotalKills(50)),
    ("tree_of_life", Condition::ReachDepth(3)),
];

/// Noch gesperrte Items für dieses Profil.
pub fn locked_items(profile: &Profile) -> Vec<&'static str> {
    UNLOCKS
        .iter()
        .filter(|(_, c)| !c.is_met(profile))
        .map(|(id, _)| *id)
        .collect()
}

impl Profile {
    /// Nimmt einen Run auf und gibt die dadurch **neu** freigeschalteten Items zurück.
    pub fn record(&mut self, run: &RunRecord) -> Vec<&'static str> {
        let before = locked_items(self);
        self.runs += 1;
        if run.outcome == Outcome::Victory {
            self.victories += 1;
        }
        self.best_depth = self.best_depth.max(run.depth_reached);
        self.total_kills += run.kills;
        self.bosses_defeated += run.bosses;
        let after = locked_items(self);
        before
            .into_iter()
            .filter(|id| !after.contains(id))
            .collect()
    }
}

// --- Speicherformat ---------------------------------------------------------------

pub const PROFILE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    MissingVersion,
    UnsupportedVersion(u32),
    InvalidNumber { key: String, value: String },
    InvalidLine(usize),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::MissingVersion => f.write_str("Profil ohne Versionsangabe"),
            ProfileError::UnsupportedVersion(v) => {
                write!(f, "Profilversion {v} wird nicht unterstützt")
            }
            ProfileError::InvalidNumber { key, value } => {
                write!(f, "'{key}': '{value}' ist keine gültige Zahl")
            }
            ProfileError::InvalidLine(n) => write!(f, "Zeile {n}: erwartet 'schlüssel=wert'"),
        }
    }
}

impl std::error::Error for ProfileError {}

impl Profile {
    pub fn to_text(&self) -> String {
        format!(
            "# rust-roguelite Profil\n\
             version={PROFILE_VERSION}\n\
             runs={}\n\
             victories={}\n\
             best_depth={}\n\
             total_kills={}\n\
             bosses_defeated={}\n",
            self.runs, self.victories, self.best_depth, self.total_kills, self.bosses_defeated
        )
    }

    /// Liest ein Profil. Fehlende Werte sind 0, unbekannte Schlüssel werden ignoriert.
    pub fn from_text(text: &str) -> Result<Self, ProfileError> {
        let mut profile = Profile::default();
        let mut version = None;

        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or(ProfileError::InvalidLine(n + 1))?;
            let (key, value) = (key.trim(), value.trim());
            let number = || {
                value
                    .parse::<u32>()
                    .map_err(|_| ProfileError::InvalidNumber {
                        key: key.to_string(),
                        value: value.to_string(),
                    })
            };
            match key {
                "version" => version = Some(number()?),
                "runs" => profile.runs = number()?,
                "victories" => profile.victories = number()?,
                "best_depth" => profile.best_depth = number()?,
                "total_kills" => profile.total_kills = number()?,
                "bosses_defeated" => profile.bosses_defeated = number()?,
                _ => {} // Unbekannt: vielleicht aus einer neueren Version.
            }
        }

        match version {
            None => Err(ProfileError::MissingVersion),
            Some(PROFILE_VERSION) => Ok(profile),
            Some(v) => Err(ProfileError::UnsupportedVersion(v)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items;

    fn record(outcome: Outcome, depth: u32, kills: u32, bosses: u32) -> RunRecord {
        RunRecord {
            outcome,
            depth_reached: depth,
            kills,
            bosses,
            items: 0,
            seconds: 60,
        }
    }

    #[test]
    fn text_roundtrip() {
        let p = Profile {
            runs: 7,
            victories: 2,
            best_depth: 3,
            total_kills: 180,
            bosses_defeated: 5,
        };
        assert_eq!(Profile::from_text(&p.to_text()), Ok(p));
    }

    #[test]
    fn unknown_keys_and_comments_are_ignored() {
        let text = "# Kommentar\nversion=1\nruns = 4\nfuture_feature=yes\n\n";
        let p = Profile::from_text(text).unwrap();
        assert_eq!(p.runs, 4);
        assert_eq!(p.victories, 0);
    }

    #[test]
    fn errors_are_reported() {
        assert_eq!(
            Profile::from_text("runs=1"),
            Err(ProfileError::MissingVersion)
        );
        assert_eq!(
            Profile::from_text("version=99"),
            Err(ProfileError::UnsupportedVersion(99))
        );
        assert!(matches!(
            Profile::from_text("version=1\nruns=viele"),
            Err(ProfileError::InvalidNumber { .. })
        ));
        assert_eq!(
            Profile::from_text("version=1\nkaputt"),
            Err(ProfileError::InvalidLine(2))
        );
    }

    #[test]
    fn record_updates_totals() {
        let mut p = Profile::default();
        p.record(&record(Outcome::Death, 2, 30, 1));
        p.record(&record(Outcome::Victory, 3, 40, 3));
        p.record(&record(Outcome::Death, 1, 5, 0));
        assert_eq!(p.runs, 3);
        assert_eq!(p.victories, 1);
        assert_eq!(p.best_depth, 3);
        assert_eq!(p.total_kills, 75);
        assert_eq!(p.bosses_defeated, 4);
    }

    #[test]
    fn unlocks_happen_once_and_in_order() {
        let mut p = Profile::default();
        assert_eq!(locked_items(&p).len(), UNLOCKS.len());

        let first = p.record(&record(Outcome::Death, 1, 10, 1));
        assert_eq!(first, vec!["shotgun"]);
        // Zweimal denselben Boss besiegen schaltet nichts Neues frei.
        let second = p.record(&record(Outcome::Death, 1, 10, 1));
        assert!(second.is_empty(), "{second:?}");

        let third = p.record(&record(Outcome::Death, 3, 40, 0));
        assert!(third.contains(&"golden_heart"), "3. Run: {third:?}");
        assert!(third.contains(&"berserker"), "60 Kills: {third:?}");
        assert!(third.contains(&"tree_of_life"), "Etage 3: {third:?}");
        assert!(locked_items(&p).is_empty());
    }

    #[test]
    fn unlock_ids_exist() {
        for (id, _) in UNLOCKS {
            assert!(
                items::ItemDb::builtin().get_str(id).is_some(),
                "unbekanntes Item '{id}'"
            );
        }
    }

    #[test]
    fn difficulty_grows_with_depth() {
        assert_eq!(enemy_health_multiplier(1), 1.0);
        assert!(enemy_health_multiplier(3) > enemy_health_multiplier(2));
        assert!(enemy_speed_multiplier(3) > 1.0);
    }
}
