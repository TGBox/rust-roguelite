//! Spielstand eines laufenden Runs: automatisch speichern, im Hauptmenü fortsetzen.
//!
//! Gespeichert wird als RON in `…/rust-roguelite/run.ron` (neben dem Profil):
//! - nach jedem Raumwechsel,
//! - nach jedem geräumten Raum,
//! - nach jedem Etagenwechsel.
//!
//! Die Etage selbst wird **nicht** gespeichert – Seed und Tiefe reichen, um
//! sie deterministisch neu zu erzeugen. Gespeichert wird nur, was sich
//! während des Spielens ändert. Stirbt man oder gewinnt, wird der Stand gelöscht.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::PathBuf,
};

use bevy::prelude::*;
use dungeon_gen::{
    GridPos, Rng, RoomLayout, RunSeed,
    items::{ItemId, ItemPools},
    loot::Loot,
};
use serde::{Deserialize, Serialize};

use crate::{
    combat::Health,
    inventory::reward_room_clear,
    player::Player,
    profile,
    progress::place_trapdoor,
    room::{CurrentRoom, RoomCleared},
    run::Run,
    schedule::GameSet,
};

/// Bei inkompatiblen Änderungen erhöhen – alte Stände werden dann ignoriert.
pub const SAVE_VERSION: u32 = 1;

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Autosave>().add_systems(
            FixedUpdate,
            autosave
                // Erst speichern, wenn die Boss-Belohnung und die Falltür feststehen.
                .after(reward_room_clear)
                .after(place_trapdoor)
                .in_set(GameSet::Cleanup),
        );
    }
}

/// Bitte um Speichern (Raumwechsel, Etagenwechsel).
#[derive(Message, Debug, Clone, Copy)]
pub struct Autosave;

/// Alles, was nötig ist, um einen Run exakt fortzusetzen.
///
/// `derive(Serialize, Deserialize)` genügt: Alle Felder – auch die Typen aus
/// `dungeon_gen` mit aktiviertem `serde`-Feature – sind serialisierbar.
/// `BTreeMap<GridPos, …>` wird in RON zu einer Map mit Struct-Schlüsseln.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSave {
    pub version: u32,
    pub seed: RunSeed,
    pub depth: u32,
    pub room: GridPos,
    pub visited: BTreeSet<GridPos>,
    pub cleared: BTreeSet<GridPos>,
    pub unlocked: BTreeSet<GridPos>,
    pub layout_overrides: BTreeMap<GridPos, RoomLayout>,
    pub loot: BTreeMap<GridPos, Vec<(GridPos, Loot)>>,
    pub loot_prepared: BTreeSet<GridPos>,
    pub trapdoor: Option<(GridPos, GridPos)>,
    pub coins: u32,
    pub keys: u32,
    pub bombs: u32,
    pub items: Vec<ItemId>,
    pub pools: ItemPools,
    pub item_rng: Rng,
    pub effect_rng: Rng,
    pub kills: u32,
    pub bosses: u32,
    /// Bisher gespielte Zeit (s).
    pub elapsed: f64,
    pub health: f32,
    pub max_health: f32,
}

impl RunSave {
    pub fn capture(run: &Run, room: GridPos, health: &Health, now: f64) -> Self {
        Self {
            version: SAVE_VERSION,
            seed: run.seed,
            depth: run.floor.depth,
            room,
            visited: run.visited.clone(),
            cleared: run.cleared.clone(),
            unlocked: run.unlocked.clone(),
            layout_overrides: run.layout_overrides.clone(),
            loot: run.loot.clone(),
            loot_prepared: run.loot_prepared.clone(),
            trapdoor: run.trapdoor,
            coins: run.inventory.coins,
            keys: run.inventory.keys,
            bombs: run.inventory.bombs,
            items: run.inventory.items.clone(),
            pools: run.pools.clone(),
            item_rng: run.item_rng.clone(),
            effect_rng: run.effect_rng.clone(),
            kills: run.stats.kills,
            bosses: run.stats.bosses,
            elapsed: (now - run.stats.started_at).max(0.0),
            health: health.current,
            max_health: health.max,
        }
    }
}

pub fn path() -> PathBuf {
    profile::data_dir().join("run.ron")
}

pub fn exists() -> bool {
    path().exists()
}

/// Lädt den Stand oder `None` (fehlt, kaputt oder alte Version – dann mit Warnung).
pub fn load() -> Option<RunSave> {
    let text = fs::read_to_string(path()).ok()?;
    match ron::de::from_str::<RunSave>(&text) {
        Ok(save) if save.version == SAVE_VERSION => Some(save),
        Ok(save) => {
            warn!(
                "Spielstand hat Version {}, erwartet {SAVE_VERSION} – ignoriert",
                save.version
            );
            None
        }
        Err(e) => {
            warn!("Spielstand unlesbar: {e}");
            None
        }
    }
}

pub fn delete() {
    match fs::remove_file(path()) {
        Ok(()) => info!("Spielstand gelöscht"),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => warn!("Spielstand konnte nicht gelöscht werden: {e}"),
    }
}

fn write(save: &RunSave) -> Result<(), Box<dyn std::error::Error>> {
    let path = path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = ron::ser::to_string_pretty(save, ron::ser::PrettyConfig::default())?;
    // Erst Temp-Datei, dann umbenennen – wie beim Profil.
    let tmp = path.with_extension("ron.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn autosave(
    mut requests: MessageReader<Autosave>,
    mut cleared: MessageReader<RoomCleared>,
    run: Res<Run>,
    room: Res<CurrentRoom>,
    health: Single<&Health, With<Player>>,
    time: Res<Time<Real>>,
) {
    // Beide Reader leeren; mehrere Anlässe im selben Tick = einmal speichern.
    let wanted = requests.read().count() + cleared.read().count();
    if wanted == 0 {
        return;
    }
    let save = RunSave::capture(&run, room.pos, &health, time.elapsed_secs_f64());
    match write(&save) {
        Ok(()) => debug!("Spielstand gespeichert (Raum {:?})", room.pos),
        Err(e) => warn!("Spielstand konnte nicht gespeichert werden: {e}"),
    }
}
