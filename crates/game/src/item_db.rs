//! Item-Datenbank aus `assets/data/items.ron` – mit Hot-Reload.
//!
//! Ablauf:
//! 1. Beim Start gilt `ItemDb::builtin()` – das Spiel läuft also immer.
//! 2. Fehlt `items.ron`, schreibt das Spiel die eingebauten Items hinein.
//!    Die Datei ist damit garantiert im richtigen Format (vom Serializer
//!    erzeugt, nicht von Hand).
//! 3. Ein eigener **AssetLoader** liest die Datei. Sobald sie geladen oder
//!    (mit `--features dev`) gespeichert wird, prüft `validate()` sie und
//!    übernimmt sie. Ungültige Dateien werden abgelehnt, die alten Items bleiben.
//!
//! Weil `PlayerStats` jeden Tick aus der Datenbank neu berechnet werden,
//! wirkt eine geänderte Zahl in `items.ron` sofort im laufenden Spiel.

use std::{fs, path::PathBuf};

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
    reflect::TypePath,
};
use dungeon_gen::items::{ITEMS_VERSION, ItemDb};
use serde::Deserialize;

use crate::inventory::Toast;

/// Asset-Pfad relativ zu `crates/game/assets`.
const ASSET_PATH: &str = "data/items.ron";

pub struct ItemDbPlugin;

impl Plugin for ItemDbPlugin {
    fn build(&self, app: &mut App) {
        ensure_items_file();
        app.insert_resource(ItemDatabase(ItemDb::builtin()))
            .init_asset::<ItemDbAsset>()
            .init_asset_loader::<ItemDbLoader>()
            .add_systems(Startup, start_loading)
            .add_systems(Update, apply_loaded_items);
    }
}

/// Die aktuell gültigen Items. Überall im Spiel wird hierüber nachgeschlagen.
#[derive(Resource, Debug)]
pub struct ItemDatabase(pub ItemDb);

/// Das Asset. `serde(transparent)`: Die Datei enthält direkt eine `ItemDb`
/// (`(items: [...])`), ohne zusätzliche Hülle.
#[derive(Asset, TypePath, Debug, Deserialize)]
#[serde(transparent)]
pub struct ItemDbAsset(pub ItemDb);

/// Handle festhalten: Solange es existiert, bleibt das Asset geladen und
/// Änderungen an der Datei lösen `AssetEvent::Modified` aus.
#[derive(Resource)]
struct ItemDbHandle(#[allow(dead_code)] Handle<ItemDbAsset>);

#[derive(Default, TypePath)]
struct ItemDbLoader;

/// `thiserror` erzeugt `Display` und `From` automatisch – `?` im Loader
/// wandelt IO- und RON-Fehler damit ohne Handarbeit in diesen Typ um.
#[derive(Debug, thiserror::Error)]
enum ItemDbLoadError {
    #[error("Datei nicht lesbar: {0}")]
    Io(#[from] std::io::Error),
    #[error("RON-Fehler: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

impl AssetLoader for ItemDbLoader {
    type Asset = ItemDbAsset;
    type Settings = ();
    type Error = ItemDbLoadError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(ron::de::from_bytes::<ItemDbAsset>(&bytes)?)
    }

    /// Zwei Punkte im Namen: So greift dieser Loader nur für `*.items.ron`
    /// und nicht für jede andere RON-Datei.
    fn extensions(&self) -> &[&str] {
        &["items.ron"]
    }
}

fn items_file() -> PathBuf {
    // `env!` wird beim Kompilieren ausgewertet: der Ordner dieser Crate.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(ASSET_PATH)
}

/// Schreibt die eingebauten Items als RON, falls die Datei fehlt oder aus
/// einer älteren Spielversion stammt. Eine alte Datei wird vorher gesichert –
/// eigene Änderungen gehen so nicht verloren.
fn ensure_items_file() {
    let path = items_file();
    if path.exists() {
        let old_version = fs::read_to_string(&path)
            .ok()
            .and_then(|text| ron::de::from_str::<ItemDbAsset>(&text).ok())
            .map(|db| db.0.version);
        match old_version {
            Some(v) if v < ITEMS_VERSION => {
                let backup = path.with_extension(format!("v{v}.bak"));
                if let Err(e) = fs::rename(&path, &backup) {
                    warn!("Alte items.ron konnte nicht gesichert werden: {e}");
                    return;
                }
                info!(
                    "items.ron war Version {v} – gesichert als {}, neue Datei wird angelegt",
                    backup.display()
                );
            }
            // Aktuell oder (von Hand kaputt editiert) unlesbar: nicht anfassen.
            _ => return,
        }
    }
    let text =
        match ron::ser::to_string_pretty(&ItemDb::builtin(), ron::ser::PrettyConfig::default()) {
            Ok(text) => text,
            Err(e) => {
                warn!("Konnte Items nicht serialisieren: {e}");
                return;
            }
        };
    let header = "// Item-Definitionen. Mit `cargo run --features dev` werden Änderungen\n\
                  // beim Speichern sofort ins laufende Spiel übernommen.\n";
    let result = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(&path, format!("{header}{text}\n")));
    match result {
        Ok(()) => info!("{} angelegt", path.display()),
        Err(e) => warn!("{} konnte nicht angelegt werden: {e}", path.display()),
    }
}

fn start_loading(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(ItemDbHandle(server.load(ASSET_PATH)));
}

fn apply_loaded_items(
    mut events: MessageReader<AssetEvent<ItemDbAsset>>,
    assets: Res<Assets<ItemDbAsset>>,
    mut db: ResMut<ItemDatabase>,
    mut toast: ResMut<Toast>,
) {
    for event in events.read() {
        let id = match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => *id,
            _ => continue,
        };
        let Some(asset) = assets.get(id) else {
            continue;
        };
        if asset.0.version < ITEMS_VERSION {
            warn!(
                "items.ron hat Version {} – eingebaute Items bleiben aktiv",
                asset.0.version
            );
            continue;
        }
        match asset.0.validate() {
            Ok(()) => {
                db.0 = asset.0.clone();
                info!("items.ron übernommen: {} Items", db.0.items.len());
                toast.show(format!("items.ron geladen: {} Items", db.0.items.len()));
            }
            Err(e) => {
                warn!("items.ron abgelehnt: {e} – die bisherigen Items bleiben aktiv");
                toast.show(format!("items.ron ungültig: {e}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Die erzeugte Datei muss sich exakt wieder einlesen lassen.
    #[test]
    fn builtin_items_roundtrip_through_ron() {
        let db = ItemDb::builtin();
        let text = ron::ser::to_string_pretty(&db, ron::ser::PrettyConfig::default()).unwrap();
        let back: ItemDbAsset = ron::de::from_str(&text).unwrap();
        assert_eq!(back.0, db);
    }
}
