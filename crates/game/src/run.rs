//! Der laufende Run: Seed, Etage, Fortschritt, Inventar und liegende Beute.
//!
//! Hier steht auch die **Startreihenfolge** eines Runs, weil sie mehrere
//! Module betrifft: erst Run anlegen, dann ersten Raum, dann Spieler.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use dungeon_gen::{
    CharacterId, Floor, GridPos, Rng, RoomLayout, RunSeed, characters, floor,
    items::{ItemDb, ItemId, ItemPools},
    loot::Loot,
    meta::{self, Outcome, RunRecord},
};
use serde::{Deserialize, Serialize};

use crate::{
    item_db::ItemDatabase,
    player,
    profile::MetaProfile,
    room,
    save::{self, RunSave},
    states::AppState,
};

pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        // `.chain()` sorgt für Reihenfolge UND fügt zwischen den Systemen
        // Sync-Punkte ein: Die per `Commands` eingefügte Ressource `Run`
        // existiert, wenn `enter_first_room` läuft.
        app.init_resource::<ChosenSeed>()
            .init_resource::<ChosenCharacter>()
            .add_systems(
                OnEnter(AppState::InGame),
                (
                    start_run,
                    room::enter_first_room,
                    player::spawn_player,
                    clear_resume,
                )
                    .chain(),
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
    /// Der gespielte Charakter – bestimmt Grundwerte und Startausrüstung.
    pub character: CharacterId,
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

/// Charakter für den nächsten Run. Anders als der Seed bleibt die Wahl
/// bestehen, bis man sie im Hauptmenü ändert.
#[derive(Resource, Debug, Default)]
pub struct ChosenCharacter(pub CharacterId);

/// Gesetzt, wenn im Hauptmenü „Fortsetzen“ gewählt wurde.
#[derive(Resource, Debug)]
pub struct PendingResume(pub RunSave);

/// Während des Run-Starts: wo und mit wie viel Leben es weitergeht.
/// Wird am Ende der Startkette wieder entfernt.
#[derive(Resource, Debug, Clone, Copy)]
pub struct ResumeInfo {
    pub room: GridPos,
    pub health: f32,
    pub max_health: f32,
}

/// Ergebnis des letzten Runs für den Zusammenfassungs-Bildschirm.
#[derive(Resource, Debug)]
pub struct LastRun {
    pub record: RunRecord,
    pub seed: RunSeed,
    /// Name des gespielten Charakters.
    pub character: &'static str,
    pub item_names: Vec<String>,
    /// Namen der durch diesen Run neu freigeschalteten Items und Charaktere.
    pub unlocked: Vec<String>,
}

/// Was der Spieler bei sich trägt.
#[derive(Debug, Default)]
pub struct Inventory {
    pub coins: u32,
    pub keys: u32,
    pub bombs: u32,
    /// Eingesammelte passive Items in Reihenfolge des Aufhebens.
    pub items: Vec<ItemId>,
    /// Höchstens ein aktives Item (Taste Q).
    pub active: Option<ActiveSlot>,
    /// Ladung abgelegter aktiver Items – hebt man sie wieder auf, geht es dort
    /// weiter (sonst ließe sich durch Tauschen gratis aufladen).
    pub stored_charges: BTreeMap<ItemId, u32>,
}

/// Das aktive Item und seine aktuelle Ladung (geräumte Räume seit der letzten Nutzung).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSlot {
    pub id: ItemId,
    pub charge: u32,
}

/// Startausrüstung eines Charakters. Items, die es in der Datenbank nicht
/// (mehr) gibt – etwa weil jemand `items.ron` bearbeitet hat –, werden mit
/// einer Warnung übersprungen statt den Run-Start scheitern zu lassen.
fn starting_inventory(character: CharacterId, db: &ItemDb) -> Inventory {
    let def = character.def();
    let known = |id: &&str| {
        let found = db.get_str(id).is_some();
        if !found {
            warn!("{}: Start-Item '{id}' fehlt in der Datenbank", def.name);
        }
        found
    };
    Inventory {
        coins: def.coins,
        keys: def.keys,
        bombs: def.bombs,
        // `copied()` macht aus `&&str` ein `&str`; `filter` reicht seinem
        // Prädikat dann wieder eine Referenz darauf – daher `&&str` oben.
        items: def
            .start_items
            .iter()
            .copied()
            .filter(known)
            .map(ItemId::from)
            .collect(),
        // Aktive Items starten voll geladen. `?` in einer Closure, die
        // `Option` zurückgibt: fehlt etwas, wird das Ganze `None`.
        active: def.active.and_then(|id| {
            let (_, charges) = db.get_str(id)?.active()?;
            Some(ActiveSlot {
                id: ItemId::from(id),
                charge: charges,
            })
        }),
        stored_charges: BTreeMap::new(),
    }
}

fn start_run(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut chosen: ResMut<ChosenSeed>,
    character: Res<ChosenCharacter>,
    profile: Res<MetaProfile>,
    db: Res<ItemDatabase>,
    resume: Option<Res<PendingResume>>,
) {
    let now = time.elapsed_secs_f64();

    // --- Fortsetzen: Run aus dem Spielstand wiederherstellen ---
    if let Some(resume) = resume {
        let s = &resume.0;
        let floor = floor::generate(s.seed, s.depth);
        info!(
            "Run fortgesetzt – Seed {}, Etage {}, Raum {:?}",
            s.seed, s.depth, s.room
        );
        commands.insert_resource(Run {
            seed: s.seed,
            character: s.character,
            floor,
            visited: s.visited.clone(),
            cleared: s.cleared.clone(),
            unlocked: s.unlocked.clone(),
            layout_overrides: s.layout_overrides.clone(),
            inventory: Inventory {
                coins: s.coins,
                keys: s.keys,
                bombs: s.bombs,
                items: s.items.clone(),
                active: s.active.clone(),
                stored_charges: s.stored_charges.clone(),
            },
            pools: s.pools.clone(),
            loot: s.loot.clone(),
            loot_prepared: s.loot_prepared.clone(),
            item_rng: s.item_rng.clone(),
            effect_rng: s.effect_rng.clone(),
            stats: RunStats {
                kills: s.kills,
                bosses: s.bosses,
                // So tun, als hätte der Run entsprechend früher begonnen.
                started_at: now - s.elapsed,
            },
            trapdoor: s.trapdoor,
        });
        commands.insert_resource(ResumeInfo {
            room: s.room,
            health: s.health,
            max_health: s.max_health,
        });
        commands.remove_resource::<RunEnd>();
        return;
    }

    // --- Neuer Run ---
    // Ein alter Spielstand wird durch den neuen Run ersetzt.
    save::delete();
    // `take()` holt den Wert heraus und hinterlässt `None`: Ein gewählter Seed
    // gilt nur für genau einen Run.
    let seed = chosen.0.take().unwrap_or_else(RunSeed::from_entropy);
    let depth = 1;
    let floor = floor::generate(seed, depth);
    // Sicherheitsnetz: Ein (noch) gesperrter Charakter wird zum Wanderer.
    let character = if character.0.is_unlocked(&profile.0) {
        character.0
    } else {
        CharacterId::default()
    };
    // Was der Charakter mitbringt, soll später nicht noch einmal auftauchen:
    // Die Start-Items kommen zu den gesperrten Items auf die Ausschlussliste.
    let mut excluded = meta::locked_items(&profile.0);
    excluded.extend(character.starting_item_ids());

    info!(
        "Neuer Run – {}, Seed {seed}, Etage {depth}, {} Räume\n{}",
        character.def().name,
        floor.len(),
        floor.to_ascii()
    );
    commands.insert_resource(Run {
        seed,
        character,
        floor,
        visited: BTreeSet::new(),
        cleared: BTreeSet::new(),
        unlocked: BTreeSet::new(),
        layout_overrides: BTreeMap::new(),
        inventory: starting_inventory(character, &db.0),
        pools: ItemPools::new(&db.0, &excluded),
        loot: BTreeMap::new(),
        loot_prepared: BTreeSet::new(),
        item_rng: seed.stream("items", 0),
        effect_rng: seed.stream("effects", 0),
        stats: RunStats {
            started_at: now,
            ..default()
        },
        trapdoor: None,
    });
    commands.remove_resource::<RunEnd>();
}

/// Letzter Schritt der Startkette: Fortsetzen-Daten wurden verbraucht.
fn clear_resume(mut commands: Commands) {
    commands.remove_resource::<PendingResume>();
    commands.remove_resource::<ResumeInfo>();
}

/// Wertet den Run aus: Profil aktualisieren und speichern, Zusammenfassung vorbereiten.
fn finalize_run(
    mut commands: Commands,
    run: Res<Run>,
    end: Option<Res<RunEnd>>,
    time: Res<Time<Real>>,
    mut profile: ResMut<MetaProfile>,
    db: Res<ItemDatabase>,
) {
    let Some(end) = end else {
        info!("Run unterbrochen – Spielstand bleibt für „Fortsetzen“ erhalten");
        return;
    };
    // Sieg oder Tod: Dieser Run lässt sich nicht fortsetzen.
    save::delete();

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
    // Profil vor dem Run merken: Der Vergleich danach zeigt, welche
    // Charaktere dieser Run freigeschaltet hat.
    let before = profile.0.clone();
    let mut unlocked: Vec<String> = profile
        .0
        .record(&record)
        .into_iter()
        .map(|id| db.0.get_str(id).map_or(id.to_string(), |i| i.name.clone()))
        .collect();
    unlocked.extend(
        characters::newly_unlocked(&before, &profile.0)
            .into_iter()
            .map(|c| format!("{} (Charakter)", c.def().name)),
    );
    crate::profile::save(&profile.0);
    info!("Run gewertet: {record:?}");

    commands.insert_resource(LastRun {
        record,
        seed: run.seed,
        character: run.character.def().name,
        item_names: db
            .0
            .resolve(&run.inventory.items)
            .iter()
            .map(|i| i.name.clone())
            // Das aktive Item gehört in die Liste – mit Kennzeichnung.
            .chain(
                run.inventory
                    .active
                    .as_ref()
                    .and_then(|a| db.0.get(&a.id))
                    .map(|i| format!("{} (aktiv)", i.name)),
            )
            .collect(),
        unlocked,
    });
    commands.remove_resource::<RunEnd>();
}

fn end_run(mut commands: Commands) {
    commands.remove_resource::<Run>();
}
