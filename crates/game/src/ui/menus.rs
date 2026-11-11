//! Die Menü-Screens. Jeder wird beim Betreten seines Zustands gespawnt
//! und dank `DespawnOnExit` beim Verlassen automatisch wieder entfernt –
//! es gibt keinen Aufräum-Code.

use bevy::{
    input::{
        common_conditions::input_just_pressed,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
};
use dungeon_gen::{
    CharacterId, RunSeed, characters,
    meta::{self, Outcome, Profile},
};

use super::{MenuAction, button, button_with, hint, perform, screen_root, title};
use crate::{
    profile::MetaProfile,
    run::{ChosenCharacter, ChosenSeed, LastRun, Run},
    save,
    states::{AppState, InGameState},
};

pub struct MenusPlugin;

impl Plugin for MenusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SeedInput>()
            .add_systems(OnEnter(AppState::MainMenu), spawn_main_menu)
            .add_systems(OnEnter(InGameState::Paused), spawn_pause_menu)
            .add_systems(OnEnter(AppState::RunSummary), spawn_summary)
            .add_systems(OnEnter(AppState::SeedEntry), spawn_seed_entry)
            .add_systems(
                Update,
                (
                    // Enter startet einen Run – aus dem Hauptmenü und von der Zusammenfassung.
                    start_run_shortcut
                        .run_if(
                            in_state(AppState::MainMenu).or_else(in_state(AppState::RunSummary)),
                        )
                        .run_if(input_just_pressed(KeyCode::Enter)),
                    seed_entry_input.run_if(in_state(AppState::SeedEntry)),
                    // C wechselt den Charakter – dasselbe wie der Button.
                    cycle_character
                        .run_if(in_state(AppState::MainMenu))
                        .run_if(input_just_pressed(KeyCode::KeyC)),
                    // Nur wenn sich die Wahl geändert hat, nicht jedes Bild.
                    refresh_character_choice
                        .run_if(in_state(AppState::MainMenu))
                        .run_if(resource_changed::<ChosenCharacter>),
                ),
            );
    }
}

/// Markiert den Text des Charakter-Buttons.
#[derive(Component)]
struct CharacterLabel;

/// Markiert die Beschreibung unter dem Charakter-Button.
#[derive(Component)]
struct CharacterHint;

/// Beschreibung des gewählten Charakters und – als Ansporn – der nächste,
/// der sich freischalten lässt.
fn character_hint(id: CharacterId, profile: &Profile) -> String {
    let mut text = format!("Charakter – {}", id.def().description);
    if let Some((next, condition)) = characters::next_locked(profile) {
        text.push_str(&format!(
            "   ·   Gesperrt: {} ({})",
            next.name,
            condition.describe()
        ));
    }
    text
}

fn cycle_character(mut chosen: ResMut<ChosenCharacter>, profile: Res<MetaProfile>) {
    chosen.0 = chosen.0.next_available(&profile.0);
}

/// Zwei Abfragen auf `&mut Text` dürfen sich nicht überschneiden – die
/// `Without`-Filter beweisen Bevy, dass sie verschiedene Entities treffen.
fn refresh_character_choice(
    chosen: Res<ChosenCharacter>,
    profile: Res<MetaProfile>,
    mut labels: Query<&mut Text, (With<CharacterLabel>, Without<CharacterHint>)>,
    mut hints: Query<&mut Text, (With<CharacterHint>, Without<CharacterLabel>)>,
) {
    for mut text in &mut labels {
        text.0 = chosen.0.def().name.to_string();
    }
    for mut text in &mut hints {
        text.0 = character_hint(chosen.0, &profile.0);
    }
}

fn spawn_main_menu(
    mut commands: Commands,
    profile: Res<MetaProfile>,
    chosen: Res<ChosenCharacter>,
) {
    let p = &profile.0;
    let character = chosen.0;
    let locked = meta::locked_items(p).len();
    let stats = format!(
        "Runs {}   ·   Siege {}   ·   Beste Etage {}   ·   Kills {}   ·   Gesperrte Items {locked}",
        p.runs, p.victories, p.best_depth, p.total_kills
    );
    // `with_children` statt `children![]`: Der „Fortsetzen“-Button gibt es nur
    // manchmal, und das Makro kennt keine Bedingungen.
    commands
        .spawn((
            Name::new("MainMenu"),
            DespawnOnExit(AppState::MainMenu),
            screen_root(Color::NONE),
        ))
        .with_children(|menu| {
            menu.spawn(title("Rust Roguelite"));
            if save::exists() {
                menu.spawn(button("Fortsetzen", MenuAction::Continue));
            }
            menu.spawn(button("Neuer Run", MenuAction::StartRun));
            menu.spawn(button_with(
                character.def().name,
                MenuAction::NextCharacter,
                CharacterLabel,
            ));
            menu.spawn((CharacterHint, hint(&character_hint(character, p))));
            menu.spawn(button("Seed eingeben", MenuAction::EnterSeed));
            menu.spawn(button("Beenden", MenuAction::Quit));
            menu.spawn(hint(&stats));
            menu.spawn(hint(
                "Enter: neuer Run   ·   C: Charakter   ·   WASD: laufen   ·   Pfeiltasten: schießen   ·   E: Bombe   ·   Esc: Pause",
            ));
        });
}

fn spawn_pause_menu(mut commands: Commands, run: Res<Run>) {
    let info = format!(
        "Seed {}   ·   Etage {}/{} ({} Räume)   ·   Esc: weiter",
        run.seed,
        run.floor.depth,
        meta::MAX_DEPTH,
        run.floor.len()
    );
    commands.spawn((
        Name::new("PauseMenu"),
        DespawnOnExit(InGameState::Paused),
        // Halbtransparent: Das eingefrorene Spiel bleibt dahinter sichtbar.
        screen_root(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        // Über allem anderen zeichnen.
        GlobalZIndex(10),
        children![
            title("Pause"),
            button("Weiter", MenuAction::Resume),
            button("Hauptmenü", MenuAction::ToMainMenu),
            hint(&info),
        ],
    ));
}

/// Formatiert Sekunden als `m:ss`.
fn format_duration(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn spawn_summary(mut commands: Commands, last: Option<Res<LastRun>>) {
    // Ohne `LastRun` (sollte nicht vorkommen) zeigen wir eine schlichte Variante.
    let (heading, details, items, unlocked) = match last.as_deref() {
        Some(l) => {
            let r = &l.record;
            let heading = match r.outcome {
                Outcome::Victory => "Geschafft!",
                Outcome::Death => "Gestorben",
            };
            let details = format!(
                "{}   ·   Etage {}/{}   ·   {} Kills   ·   {} Boss(e)   ·   {} Items   ·   {}   ·   Seed {}",
                l.character,
                r.depth_reached,
                meta::MAX_DEPTH,
                r.kills,
                r.bosses,
                r.items,
                format_duration(r.seconds),
                l.seed
            );
            let items = if l.item_names.is_empty() {
                "Keine Items".to_string()
            } else {
                format!("Items: {}", l.item_names.join(", "))
            };
            let unlocked = if l.unlocked.is_empty() {
                String::new()
            } else {
                format!("Neu freigeschaltet: {}", l.unlocked.join(", "))
            };
            (heading, details, items, unlocked)
        }
        None => ("Run beendet", String::new(), String::new(), String::new()),
    };

    commands.spawn((
        Name::new("RunSummary"),
        DespawnOnExit(AppState::RunSummary),
        screen_root(Color::NONE),
        children![
            title(heading),
            hint(&details),
            hint(&items),
            hint(&unlocked),
            button("Neuer Run", MenuAction::StartRun),
            button("Gleicher Seed", MenuAction::RetrySeed),
            button("Hauptmenü", MenuAction::ToMainMenu),
        ],
    ));
}

// --- Seed-Eingabe ----------------------------------------------------------------

/// Bisher eingetippte Hex-Ziffern (ohne Bindestriche).
#[derive(Resource, Default)]
struct SeedInput(String);

#[derive(Component)]
struct SeedText;

#[derive(Component)]
struct SeedHint;

fn spawn_seed_entry(mut commands: Commands, mut input: ResMut<SeedInput>) {
    input.0.clear();
    commands.spawn((
        Name::new("SeedEntry"),
        DespawnOnExit(AppState::SeedEntry),
        screen_root(Color::NONE),
        children![
            title("Seed eingeben"),
            (SeedText, title(&render_seed(""))),
            (SeedHint, hint("16 Zeichen: 0–9 und A–F")),
            hint("Enter: starten   ·   Rücktaste: löschen   ·   Esc: zurück"),
        ],
    ));
}

/// `1A2B3` → `1A2B-3___-____-____`
fn render_seed(digits: &str) -> String {
    let padded: Vec<char> = digits
        .chars()
        .chain(std::iter::repeat('_'))
        .take(16)
        .collect();
    padded
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// Liest Tastatur-Messages direkt: `KeyboardInput` liefert das *logische*
/// Zeichen (Tastaturlayout berücksichtigt) – anders als `KeyCode`, das nur
/// die physische Taste kennt.
fn seed_entry_input(
    mut keys: MessageReader<KeyboardInput>,
    mut input: ResMut<SeedInput>,
    mut chosen: ResMut<ChosenSeed>,
    mut next: ResMut<NextState<AppState>>,
    mut seed_text: Single<&mut Text, (With<SeedText>, Without<SeedHint>)>,
    mut hint_text: Single<&mut Text, (With<SeedHint>, Without<SeedText>)>,
) {
    let mut changed = false;
    for event in keys.read() {
        if !event.state.is_pressed() {
            continue;
        }
        match &event.logical_key {
            Key::Character(chars) => {
                for c in chars.chars().filter(char::is_ascii_hexdigit) {
                    if input.0.len() < 16 {
                        input.0.push(c.to_ascii_uppercase());
                        changed = true;
                    }
                }
            }
            Key::Backspace => {
                changed |= input.0.pop().is_some();
            }
            Key::Enter => match input.0.parse::<RunSeed>() {
                Ok(seed) => {
                    chosen.0 = Some(seed);
                    next.set(AppState::InGame);
                }
                Err(e) => hint_text.0 = e.to_string(),
            },
            Key::Escape => next.set(AppState::MainMenu),
            _ => {}
        }
    }
    if changed {
        seed_text.0 = render_seed(&input.0);
    }
}

/// Nutzt `perform`, damit Tastenkürzel und Button garantiert dasselbe tun.
/// `&mut app_state` ist ein `&mut ResMut<…>` – Rust wandelt es per
/// Deref-Coercion automatisch in das erwartete `&mut NextState<…>` um.
fn start_run_shortcut(
    mut app_state: ResMut<NextState<AppState>>,
    mut in_game: ResMut<NextState<InGameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    perform(
        MenuAction::StartRun,
        &mut app_state,
        &mut in_game,
        &mut exit,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_rendering() {
        assert_eq!(render_seed(""), "____-____-____-____");
        assert_eq!(render_seed("1A2B3"), "1A2B-3___-____-____");
        assert_eq!(render_seed("0123456789ABCDEF"), "0123-4567-89AB-CDEF");
    }

    #[test]
    fn character_texts() {
        let fresh = Profile::default();
        // Frisches Profil: Es gibt noch etwas freizuschalten – das steht im Hinweis.
        let hint = character_hint(CharacterId::Wanderer, &fresh);
        assert!(hint.contains(CharacterId::Wanderer.def().description));
        assert!(hint.contains("Gesperrt: Späherin"), "{hint}");
    }

    #[test]
    fn durations() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(252), "4:12");
    }
}
