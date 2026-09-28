//! Der laufende Run: Seed, Etage, Fortschritt, Inventar und liegende Beute.
//!
//! Hier steht auch die **Startreihenfolge** eines Runs, weil sie mehrere
//! Module betrifft: erst Run anlegen, dann ersten Raum, dann Spieler.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use dungeon_gen::{
    Floor, GridPos, Rng, RunSeed, floor,
    items::{ItemDef, ItemPools},
    loot::{Loot, START_BOMBS, START_KEYS},
};

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
    pub inventory: Inventory,
    /// Noch nicht vergebene Items.
    pub pools: ItemPools,
    /// Beute, die in den Räumen liegt: Raum → (Kachel, Beute).
    /// So bleibt Liegengelassenes erhalten, wenn man den Raum verlässt.
    pub loot: BTreeMap<GridPos, Vec<(GridPos, Loot)>>,
    /// Räume, deren Beute schon festgelegt wurde.
    pub loot_prepared: BTreeSet<GridPos>,
    /// Eigene Zufallsfolgen, damit z. B. Kill-Effekte nicht die Item-Auswahl verschieben.
    pub item_rng: Rng,
    pub effect_rng: Rng,
}

/// Was der Spieler bei sich trägt.
#[derive(Debug, Default)]
pub struct Inventory {
    pub coins: u32,
    pub keys: u32,
    pub bombs: u32,
    /// Eingesammelte Items in Reihenfolge des Aufhebens.
    pub items: Vec<&'static ItemDef>,
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
        inventory: Inventory {
            keys: START_KEYS,
            bombs: START_BOMBS,
            ..default()
        },
        pools: ItemPools::default(),
        loot: BTreeMap::new(),
        loot_prepared: BTreeSet::new(),
        item_rng: seed.stream("items", 0),
        effect_rng: seed.stream("effects", 0),
    });
}

fn end_run(mut commands: Commands) {
    commands.remove_resource::<Run>();
}
