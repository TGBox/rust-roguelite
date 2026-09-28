//! Der laufende Run: Seed, Etage, Fortschritt, Inventar und liegende Beute.
//!
//! Hier steht auch die **Startreihenfolge** eines Runs, weil sie mehrere
//! Module betrifft: erst Run anlegen, dann ersten Raum, dann Spieler.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use dungeon_gen::{
    Floor, GridPos, Rng, RoomLayout, RunSeed, floor,
    items::{self, ItemDef, ItemPools},
    loot::{Loot, START_BOMBS, START_KEYS},
    meta::{self, Outcome, RunRecord},
};

use crate::{player, profile::MetaProfile, room, states::AppState};

pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        // `.chain()` sorgt für Reihenfolge UND fügt zwischen den Systemen
        // Sync-Punkte ein: Die per `Commands` eingefügte Ressource `Run`
        // existiert, wenn `enter_first_room` läuft.
        app.init_resource::<ChosenSeed>()
            .add_systems(
                OnEnter(AppState::InGame),
                (start_run, room::enter_first_room, player::spawn_player).chain(),
            )
            // Erst auswerten, dann den Run entfernen.
            .add_systems(OnExit(AppState::InGame), (finalize_run, end_run).chain());
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
    /// Verschlossene Räume, die mit einem Schlüssel geöffnet wurden.
    pub unlocked: BTreeSet<GridPos>,
    /// Veränderte Layouts (gesprengte Felsen) – bleiben beim Zurückkommen erhalten.
    pub layout_overrides: BTreeMap<GridPos, RoomLayout>,
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
    pub stats: RunStats,
    /// Falltür nach dem Boss: (Raum, Kachel).
    pub trapdoor: Option<(GridPos, GridPos)>,
}

#[derive(Debug, Default)]
pub struct RunStats {
    pub kills: u32,
    pub bosses: u32,
    /// Echtzeit beim Start (s) – für die Dauer in der Zusammenfassung.
    pub started_at: f64,
}

/// Wie der Run endet. Fehlt die Ressource beim Verlassen von `InGame`,
/// wurde der Run abgebrochen (Pausemenü → Hauptmenü) und zählt nicht.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    Victory,
    Death,
}

/// Seed für den nächsten Run (aus der Seed-Eingabe oder „Gleicher Seed“).
#[derive(Resource, Debug, Default)]
pub struct ChosenSeed(pub Option<RunSeed>);

/// Ergebnis des letzten Runs für den Zusammenfassungs-Bildschirm.
#[derive(Resource, Debug)]
pub struct LastRun {
    pub record: RunRecord,
    pub seed: RunSeed,
    pub item_names: Vec<&'static str>,
    /// Namen der durch diesen Run neu freigeschalteten Items.
    pub unlocked: Vec<&'static str>,
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

fn start_run(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut chosen: ResMut<ChosenSeed>,
    profile: Res<MetaProfile>,
) {
    // `take()` holt den Wert heraus und hinterlässt `None`: Ein gewählter Seed
    // gilt nur für genau einen Run.
    let seed = chosen.0.take().unwrap_or_else(RunSeed::from_entropy);
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
        unlocked: BTreeSet::new(),
        layout_overrides: BTreeMap::new(),
        inventory: Inventory {
            keys: START_KEYS,
            bombs: START_BOMBS,
            ..default()
        },
        pools: ItemPools::without(&meta::locked_items(&profile.0)),
        loot: BTreeMap::new(),
        loot_prepared: BTreeSet::new(),
        item_rng: seed.stream("items", 0),
        effect_rng: seed.stream("effects", 0),
        stats: RunStats {
            started_at: time.elapsed_secs_f64(),
            ..default()
        },
        trapdoor: None,
    });
    commands.remove_resource::<RunEnd>();
}

/// Wertet den Run aus: Profil aktualisieren und speichern, Zusammenfassung vorbereiten.
fn finalize_run(
    mut commands: Commands,
    run: Res<Run>,
    end: Option<Res<RunEnd>>,
    time: Res<Time<Real>>,
    mut profile: ResMut<MetaProfile>,
) {
    let Some(end) = end else {
        info!("Run abgebrochen – wird nicht gewertet");
        return;
    };
    let record = RunRecord {
        outcome: match *end {
            RunEnd::Victory => Outcome::Victory,
            RunEnd::Death => Outcome::Death,
        },
        depth_reached: run.floor.depth,
        kills: run.stats.kills,
        bosses: run.stats.bosses,
        items: run.inventory.items.len() as u32,
        seconds: (time.elapsed_secs_f64() - run.stats.started_at).max(0.0) as u32,
    };
    let unlocked = profile
        .0
        .record(&record)
        .into_iter()
        .filter_map(|id| items::by_id(id).map(|i| i.name))
        .collect();
    crate::profile::save(&profile.0);
    info!("Run gewertet: {record:?}");

    commands.insert_resource(LastRun {
        record,
        seed: run.seed,
        item_names: run.inventory.items.iter().map(|i| i.name).collect(),
        unlocked,
    });
    commands.remove_resource::<RunEnd>();
}

fn end_run(mut commands: Commands) {
    commands.remove_resource::<Run>();
}
