//! Dauerhaftes Spielerprofil auf der Festplatte.
//!
//! Ort: `%APPDATA%\rust-roguelite\profile.txt` (Windows) bzw.
//! `~/.local/share/rust-roguelite/profile.txt` (Linux). Das Format selbst
//! (lesen/schreiben/prüfen) steckt getestet in `dungeon_gen::meta`.

use std::{fs, io, path::PathBuf};

use bevy::prelude::*;
use dungeon_gen::meta::Profile;

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MetaProfile(load()));
    }
}

/// Wrapper wegen der Orphan Rule (siehe `run::Run`).
#[derive(Resource, Debug, Default)]
pub struct MetaProfile(pub Profile);

pub fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("rust-roguelite").join("profile.txt")
}

/// Lädt das Profil. Fehlt die Datei, beginnt man mit einem leeren Profil.
/// Ist sie kaputt, wird sie als `.bak` beiseitegelegt statt überschrieben.
fn load() -> Profile {
    let path = path();
    match fs::read_to_string(&path) {
        Ok(text) => match Profile::from_text(&text) {
            Ok(profile) => {
                info!("Profil geladen: {}", path.display());
                profile
            }
            Err(e) => {
                let backup = path.with_extension("txt.bak");
                warn!(
                    "Profil unlesbar ({e}) – Sicherung unter {}",
                    backup.display()
                );
                if let Err(e) = fs::rename(&path, &backup) {
                    warn!("Sicherung fehlgeschlagen: {e}");
                }
                Profile::default()
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Profile::default(),
        Err(e) => {
            warn!("Profil konnte nicht gelesen werden: {e}");
            Profile::default()
        }
    }
}

/// Speichert das Profil. Erst in eine Temp-Datei, dann umbenennen: So bleibt
/// bei einem Absturz mitten im Schreiben die alte Datei heil.
pub fn save(profile: &Profile) {
    // Ein `?`-fähiger Block: Fehler werden gesammelt behandelt statt einzeln.
    let result = (|| -> io::Result<PathBuf> {
        let path = path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("txt.tmp");
        fs::write(&tmp, profile.to_text())?;
        fs::rename(&tmp, &path)?;
        Ok(path)
    })();
    match result {
        Ok(path) => info!("Profil gespeichert: {}", path.display()),
        Err(e) => warn!("Profil konnte nicht gespeichert werden: {e}"),
    }
}
