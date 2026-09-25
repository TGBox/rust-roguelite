//! Der laufende Run: Seed, aktuelle Etage, besuchte und geräumte Räume.
//!
//! Hier steht auch die **Startreihenfolge** eines Runs, weil sie mehrere
//! Module betrifft: erst Run anlegen, dann ersten Raum, dann Spieler.

use std::collections::BTreeSet;

use bevy::prelude::*;
use dungeon_gen::{Floor, GridPos, RunSeed, floor};

use crate::{player, room, states::AppState};

pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        // `.chain()` sorgt für Reihenfolge UND fügt zwischen den Systemen
        // Sync-Punkte ein: Die per `Commands` eingefügte Ressource `Run`
        // existiert, wenn `enter_first_room` läuft.
        app.add_systems(
            OnEnter(AppState::InGame),
            (start_run, room::enter_first_room, player::spawn_player).chain(),
        )
        .add_systems(OnExit(AppState::InGame), end_run);
    }
}

/// Zustand des aktuellen Runs.
///
/// Warum nicht einfach `impl Resource for Floor`? Die **Orphan Rule** verbietet
/// es, ein fremdes Trait (`Resource` aus Bevy) für einen fremden Typ (`Floor`
/// aus `dungeon_gen`) zu implementieren. Eine eigene Wrapper-Struktur löst das –
/// und hält `dungeon_gen` frei von Bevy.
#[derive(Resource, Debug)]
pub struct Run {
    pub seed: RunSeed,
    pub floor: Floor,
    /// Räume, die der Spieler betreten hat (für die Minimap).
    pub visited: BTreeSet<GridPos>,
    /// Räume ohne verbleibende Gegner.
    pub cleared: BTreeSet<GridPos>,
}

fn start_run(mut commands: Commands) {
    let seed = RunSeed::from_entropy();
    let depth = 1;
    let floor = floor::generate(seed, depth);

    info!(
        "Neuer Run – Seed {seed}, Etage {depth}, {} Räume\n{}",
        floor.len(),
        floor.to_ascii()
    );
    commands.insert_resource(Run {
        seed,
        floor,
        visited: BTreeSet::new(),
        cleared: BTreeSet::new(),
    });
}

fn end_run(mut commands: Commands) {
    commands.remove_resource::<Run>();
}
