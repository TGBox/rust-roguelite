//! Reihenfolge der Spiellogik innerhalb eines festen Zeitschritts.
//!
//! Module hängen ihre Systeme nur in ein `GameSet` ein, statt sich
//! gegenseitig per `.before()/.after()` zu kennen. Die globale Reihenfolge
//! und die Bedingung „nur während des Spielens“ stehen an genau einer Stelle.

use bevy::prelude::*;

use crate::states::InGameState;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    /// Positionen des letzten Ticks sichern (für die Interpolation).
    Snapshot,
    /// Spieler- und KI-Entscheidungen: Geschwindigkeit setzen, Schüsse erzeugen.
    Control,
    /// Bewegen und Kollision mit dem Raum auflösen.
    Physics,
    /// Treffer erkennen, Schaden anwenden, Tode behandeln.
    Combat,
    /// Aufräumen und Folgen (Lebenszeit, Türen, Raumwechsel).
    Cleanup,
}

pub struct SchedulePlugin;

impl Plugin for SchedulePlugin {
    fn build(&self, app: &mut App) {
        const ALL: [GameSet; 5] = [
            GameSet::Snapshot,
            GameSet::Control,
            GameSet::Physics,
            GameSet::Combat,
            GameSet::Cleanup,
        ];

        // `.chain()` gibt es für Tupel, nicht für Arrays – deshalb hier ausgeschrieben.
        app.configure_sets(
            FixedUpdate,
            (
                GameSet::Snapshot,
                GameSet::Control,
                GameSet::Physics,
                GameSet::Combat,
                GameSet::Cleanup,
            )
                .chain(),
        );

        // Doppelt abgesichert zur angehaltenen virtuellen Zeit:
        // In Menüs, Pause, Raumwechsel und beim Sterben laufen diese Systeme nie.
        for set in ALL {
            app.configure_sets(FixedUpdate, set.run_if(in_state(InGameState::Playing)));
        }
    }
}
