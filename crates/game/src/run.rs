//! Der laufende Run: Seed, aktuelle Etage.
//!
//! In M3 wird die Etage nur erzeugt und geloggt. Ab M4 wird sie gespielt.

use bevy::prelude::*;
use dungeon_gen::{Floor, RunSeed, floor};

use crate::states::AppState;

pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), start_run)
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
    commands.insert_resource(Run { seed, floor });
}

fn end_run(mut commands: Commands) {
    commands.remove_resource::<Run>();
}
