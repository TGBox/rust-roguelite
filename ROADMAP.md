# rust-roguelite – Roadmap

Echtzeit-Action-Roguelite im Stil von *The Binding of Isaac*, gebaut mit **Bevy 0.19** (Release Juni 2026).
Ziel: Rust an einem komplexen, realistischen Projekt lernen – mit sauberer Architektur, Tests und Determinismus.

---

## 1. Leitentscheidungen

| Thema | Entscheidung | Begründung |
|---|---|---|
| Genre | Echtzeit, raumbasiert, Twin-Stick (WASD bewegen, Pfeiltasten schießen) | Projektile, Velocity, Angriffsgeschwindigkeit aus dem Ursprungsplan |
| Engine | Bevy 0.19 komplett (ECS, Rendering, Audio, Input, Assets) | Fokus auf Spiellogik statt Engine-Bau |
| Physik | **Eigene** Kollision (AABB gegen Tile-Grid, Kreis gegen Kreis) | Lerneffekt, volle Kontrolle, deterministisch; kein Physik-Crate |
| Mathe | `f32` mit Bevy-`Vec2`, **keine** Fixpoint-Math | Determinismus über festen Zeitschritt + geseedetes RNG reicht für Replays/Debugging ohne Netcode |
| Gameplay-Takt | Logik in `FixedUpdate` (64 Hz), Input-Erfassung in `Update` | Framerate-unabhängig, reproduzierbar |
| RNG | Eigener xoshiro256** in `dungeon_gen` (SplitMix64 zum Seeden), getrennte Streams pro Zweck | Stabil über alle Versionen, keine Dependency; Kampf-Zufall verändert nicht den Dungeon desselben Seeds |
| Daten | Items, Gegner, Raumvorlagen als RON-Dateien in `assets/` | Datengetrieben, ohne Neukompilieren änderbar |
| Grafik | Erst farbige Formen (`Mesh2d`), später Sprites (z. B. Kenney-Assets, CC0) | Logik zuerst, Optik zuletzt |

### Änderungen gegenüber dem Ursprungsplan

- **Workspace-Split `engine / game_logic / renderer` entfällt** – Bevy ist die Engine. Stattdessen:
  eine reine, Bevy-freie Logik-Crate (Dungeon-Generierung, Stat-Berechnung) + die Spiel-Crate mit einem Bevy-Plugin pro Fachbereich.
- **Eigenes ECS entfällt** – Phase 4 wird zu „Bevy-ECS richtig nutzen“ (Plugins, SystemSets, Observer, Messages, Change Detection).
- **Rendering (Phase 6) wird kleiner**, dafür kommt „Game Feel“ (Screenshake, Hitstop, Partikel) dazu.

---

## 2. Architektur

```
rust-roguelite/
├── Cargo.toml                 # Workspace + gemeinsame Dependency-Versionen
├── .cargo/config.toml         # schnellere Dev-Builds
├── crates/
│   ├── dungeon_gen/           # KEIN Bevy. Reine Logik, schnell testbar
│   │   └── src/{lib, rng, floor, room_template}.rs
│   └── game/                  # Bevy-App (Binary)
│       └── src/
│           ├── main.rs
│           ├── states.rs          # AppState, InGameState
│           ├── camera.rs          # 2D-Kamera (immer genau ein Raum im Bild)
│           ├── debug/             # Gizmos, Overlays (nicht `core` nennen: kollidiert mit std-Crate `core`)
│           ├── physics/           # Velocity, Collider, Kollision
│           ├── player/            # Input, Bewegung, Schießen
│           ├── combat/            # Health, Damage-Messages, Knockback, i-Frames
│           ├── enemy/             # Gegnertypen, KI, Flowfield
│           ├── dungeon/           # Floor/Raum spawnen, Türen, Raumwechsel
│           ├── items/             # Item-Defs, Pickups, Stat-Modifier, Trigger
│           ├── run/               # Seed, Run-Status, Tod, Zusammenfassung
│           ├── meta/              # Profil, Freischaltungen, Savegames
│           └── ui/                # Menüs, HUD, Minimap
└── crates/game/assets/   # Bevy sucht relativ zur Crate, nicht zum Workspace
    ├── data/{items, enemies, rooms}/*.ron
    ├── sprites/  audio/  fonts/
```

Jeder Ordner unter `game/src/` ist ein **Bevy-Plugin** (`impl Plugin for PlayerPlugin`). `main.rs` fügt nur Plugins zusammen.

### Zustände

```
AppState:     Loading → MainMenu → InGame → RunSummary → MainMenu
InGameState (SubState von InGame):  Playing ⇄ Paused
                                    Playing → RoomTransition → Playing
                                    Playing → Dying → (AppState::RunSummary)
```

### RNG-Streams

```
RunSeed (u64, anzeigbar/eingebbar)
 ├── stream("floor", n)   → Layout von Etage n
 ├── stream("items")      → Item-Pool-Ziehungen
 └── stream("combat")     → Crits, Drops, KI-Entscheidungen
```
Jeder Stream = `RunSeed::stream(name, n)` → `Rng::from_seed(mix(seed, fnv1a(name), n))`.

### Spielwelt-Modell (Isaac-artig)

- **Etage** = Raster aus Räumen (13×13, Start in der Mitte, max. 20 Räume), erzeugt per Expansion vom Startraum aus.
- **Raum** = 15×9 Kacheln (13×7 begehbar + Wandring), Inhalt aus RON-Vorlagen.
- **Spezialräume**: Boss (weitester Sackgassen-Raum), Schatz, Shop, Geheimraum (später).
- Kamera zeigt immer genau einen Raum; Raumwechsel = Kamera-Schwenk.

---

## 3. Meilensteine

Jeder Meilenstein endet mit etwas **Spielbarem oder Testbarem**. „DoD“ = Definition of Done.

### M0 – Fundament ✅
- Workspace, `bevy = "0.19"`, Dev-Profil (`opt-level = 1` für eigenen Code, `3` für Dependencies), optional `dynamic_linking`.
- Bevy-Features auf 2D reduzieren (genaue Feature-Namen für 0.19 prüfen).
- Fenster, 2D-Kamera, Log-Ausgabe, `cargo clippy` sauber.
- **DoD:** Fenster öffnet sich mit farbigem Quadrat; `cargo test` läuft (leer).
- *Lernfokus:* Workspace-Dependencies, Cargo-Profile, Plugin-Pattern.

### M1 – Core-Loop-Prototyp ✅ (ein fester Raum)
- `Velocity`, `Collider { half_extents }`, Tile-Grid-Kollision mit Wand-Sliding.
- Spieler: 8-Wege-Bewegung mit Beschleunigung/Reibung, Schießen in 4 Richtungen, Feuerrate-Cooldown.
- Projektile mit Reichweite/Lebenszeit, Zerstörung an Wänden.
- **DoD:** Man läuft in einem Raum herum und schießt; nichts geht durch Wände.
- *Lernfokus:* Components/Queries, `FixedUpdate`, SystemSets & Reihenfolge (`Input → Move → Collide`).

### M2 – Zustände & Spielfluss ✅
- `AppState` + `InGameState`, Hauptmenü, Pause (Esc), Game-Over-Screen.
- `DespawnOnExit(State)` für automatisches Aufräumen.
- **DoD:** Menü → Spiel → Pause → Tod → Menü, ohne übrig gebliebene Entities.
- *Lernfokus:* States, SubStates, `OnEnter/OnExit`, Run-Conditions.

### M3 – Prozedurale Etagen ✅ (`dungeon_gen`, Bevy-frei)
- `RunSeed`, RNG-Streams.
- Etagen-Generator: Raumanzahl abhängig von Etage, Expansion mit Nachbar-Regeln, Spezialräume zuweisen.
- Raumvorlagen als ASCII im Code (RON-Dateien folgen in M6).
- Tests: gleicher Seed ⇒ gleiche Etage; alle Räume erreichbar; Boss-Raum ist Sackgasse; Property-Tests über 10 000 Seeds (eigene Schleife statt `proptest`).
- **DoD:** `cargo test -p dungeon_gen` grün; ASCII-Ausgabe einer Etage per Beispiel-Binary.
- *Lernfokus:* reine Datenmodelle, `serde`, Trait-basierte Generator-Schritte, Property-Testing.

### M4 – Räume im Spiel ✅
- Etage aus `dungeon_gen` in Bevy spawnen (nur aktiven Raum als Entities).
- Türen: offen/zu/verschlossen; schließen beim Betreten eines ungeklärten Raums, öffnen nach letztem Gegner.
- Raumwechsel mit Kamera-Schwenk (`InGameState::RoomTransition`).
- Debug-Minimap (vorerst simpel).
- **DoD:** Komplette Etage begehbar, Türen reagieren korrekt.
- *Lernfokus:* Ressourcen vs. Entities, Relationships (Raum ↔ Inhalt), Observer.

### M5 – Gegner & Kampf ✅
- `Health`, `Damage`-Message, Knockback, Unverwundbarkeitsframes, Kontaktschaden.
- Gegnertypen: *Chaser* (läuft zum Spieler), *Shooter* (hält Abstand, schießt), *Charger* (Anlauf + Sprint).
- Pathfinding per **Flowfield** (Dijkstra-Map vom Spieler, einmal pro Tick für alle Gegner).
- KI als einfache Zustandsmaschine pro Gegner (`Idle → Chase → Attack → Cooldown`).
- Erster Boss.
- **DoD:** Räume mit Gegnern räumbar, Boss besiegbar, Spieler kann sterben.
- *Lernfokus:* Messages vs. Observer, generische Systeme, Enum-Zustandsmaschinen.

### M6 – Items, Stats & Synergien ✅ (M6a: Items als Rust-Daten; M6b offen: RON + Hot-Reload)
- Item-Definitionen in RON (`id`, Name, Seltenheit, Pool, Modifier, Trigger).
- Stat-System: `Base → +Flat → ×Mult`, neu berechnet bei Änderung (Change Detection).
- Trigger-Effekte (`OnHit`, `OnKill`, `OnRoomClear`, `OnPickupGold`) über Observer.
- Pickups: Herzen, Gold, Schlüssel, Bomben; Schatzraum, Boss-Drop, Shop.
- **DoD:** 15–20 Items, mindestens 3 spürbare Synergien.
- *Lernfokus:* Custom Asset Loader, Trait Objects vs. Enums für Effekte, datengetriebenes Design.

### M7 – Run-Management & Meta-Progression ✅ (ohne Run-Save – kommt mit serde in M6b)
- Mehrere Etagen, Schwierigkeitskurve.
- Run-Zusammenfassung (Zeit, Kills, Items, Seed).
- Persistentes Profil (Freischaltungen) als versionierte Textdatei in `%APPDATA%` (eigenes Format in `dungeon_gen::meta`, getestet).
- Run-Save zwischen Räumen (Weiterspielen nach Neustart).
- Seed-Eingabe im Menü für reproduzierbare Runs.
- **DoD:** Tod ⇒ Zusammenfassung ⇒ Freischaltung wirkt im nächsten Run.
- *Lernfokus:* Serialisierung, Versionierung von Savegames, Fehlerbehandlung mit `thiserror`/`anyhow`.

### M8 – Präsentation & Game Feel
- Sprites + Texture-Atlas, Animationen, Z-Ordnung.
- HUD (Herzen, Gold/Schlüssel/Bomben, aktives Item), echte Minimap.
- Audio: Schuss, Treffer, Tod, Musik pro Etage.
- Screenshake, Hitstop, Treffer-Flash, Partikel.
- **DoD:** fühlt sich wie ein Spiel an, nicht wie ein Prototyp.
- **M8a (umgesetzt, Test ausstehend):** Pixel-Art im Code (`pixel_art/`: ASCII-Figuren,
  prozedurale Kacheln mit Rauschen, weiße Treffer-Silhouetten), `juice.rs` mit
  `Fx`-Messages: Trauma-Screenshake, Hitstop über `Time<Virtual>`-Tempo, Partikel,
  Lauf-Wackeln, Blickrichtung, Charger-Ausholen (rot pulsierend, geduckt).
- **M8b (umgesetzt, Test ausstehend):** `audio/synth.rs` erzeugt 15 Effekte und je Etage eine
  Musikschleife als WAV im Speicher; `Sfx`-Messages, Wiederholsperre, Tonhöhen-Variation,
  Musik pausiert im Pausemenü, **M** schaltet stumm.

### M9 – Polish & Release
- Einstellungen (Lautstärke, Tastenbelegung) über Bevys `SettingsPlugin`.
- Release-Profil (LTO), Windows-Build, optional WASM.
- Performance-Check mit Diagnostics-Overlay.

---

## 4. Querschnitt

- **Tests:** `dungeon_gen` vollständig unit-/property-getestet; Bevy-Systeme headless mit `MinimalPlugins` testen.
- **Debugging:** Gizmos für Collider/Flowfield, Taste F1 schaltet Debug-Overlay.
- **Code-Qualität:** `clippy -D warnings`, `rustfmt`, später CI (GitHub Actions).
- **Git:** ein Commit pro abgeschlossenem Teilschritt, Tag pro Meilenstein.
