//! Die drei Menü-Screens. Jeder wird beim Betreten seines Zustands gespawnt
//! und dank `DespawnOnExit` beim Verlassen automatisch wieder entfernt –
//! es gibt keinen Aufräum-Code.

use bevy::{input::common_conditions::input_just_pressed, prelude::*};

use super::{MenuAction, button, hint, perform, screen_root, title};
use crate::{
    run::Run,
    states::{AppState, InGameState},
};

pub struct MenusPlugin;

impl Plugin for MenusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), spawn_main_menu)
            .add_systems(OnEnter(InGameState::Paused), spawn_pause_menu)
            .add_systems(OnEnter(AppState::GameOver), spawn_game_over)
            // Enter startet einen Run – aus dem Hauptmenü und vom Game-Over-Screen.
            .add_systems(
                Update,
                start_run_shortcut
                    .run_if(in_state(AppState::MainMenu).or_else(in_state(AppState::GameOver)))
                    .run_if(input_just_pressed(KeyCode::Enter)),
            );
    }
}

fn spawn_main_menu(mut commands: Commands) {
    commands.spawn((
        Name::new("MainMenu"),
        DespawnOnExit(AppState::MainMenu),
        screen_root(Color::NONE),
        children![
            title("Rust Roguelite"),
            button("Neuer Run", MenuAction::StartRun),
            button("Beenden", MenuAction::Quit),
            hint("Enter: Start   ·   WASD: laufen   ·   Pfeiltasten: schießen   ·   E: Bombe   ·   Esc: Pause"),
        ],
    ));
}

fn spawn_pause_menu(mut commands: Commands, run: Res<Run>) {
    let info = format!(
        "Seed {}   ·   Etage {} ({} Räume)   ·   Esc: weiter",
        run.seed,
        run.floor.depth,
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

fn spawn_game_over(mut commands: Commands) {
    commands.spawn((
        Name::new("GameOver"),
        DespawnOnExit(AppState::GameOver),
        screen_root(Color::NONE),
        children![
            title("Game Over"),
            button("Neuer Run", MenuAction::StartRun),
            button("Hauptmenü", MenuAction::ToMainMenu),
            hint("Enter: neuer Run"),
        ],
    ));
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
