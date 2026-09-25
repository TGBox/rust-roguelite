//! Reihenfolge der Spiellogik innerhalb eines festen Zeitschritts.
//!
//! Module hängen ihre Systeme nur in ein `GameSet` ein, statt sich
//! gegenseitig per `.before()/.after()` zu kennen. Die globale Reihenfolge
//! steht dadurch an genau einer Stelle.

use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    /// Positionen des letzten Ticks sichern (für die Interpolation).
    Snapshot,
    /// Spieler- und KI-Entscheidungen: Geschwindigkeit setzen, Schüsse erzeugen.
    Control,
    /// Bewegen und Kollision auflösen.
    Physics,
    /// Auf Ereignisse reagieren und Aufräumen (Lebenszeit, Treffer).
    Cleanup,
}

pub struct SchedulePlugin;

impl Plugin for SchedulePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedUpdate,
            (
                GameSet::Snapshot,
                GameSet::Control,
                GameSet::Physics,
                GameSet::Cleanup,
            )
                .chain(),
        );
    }
}
