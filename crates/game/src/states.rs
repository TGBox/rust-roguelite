//! Spielzustände und Übergänge.
//!
//! ```text
//! AppState:    MainMenu ──▶ InGame ──▶ GameOver ──▶ InGame (neuer Run)
//!                 ▲            │           │
//!                 └────────────┴───────────┘  (Hauptmenü)
//!
//! InGameState (existiert nur während AppState::InGame):
//!              Playing ⇄ Paused
//!              Playing ──▶ RoomTransition ──▶ Playing
//!              Playing ──▶ Dying ──(1 s)──▶ AppState::GameOver
//! ```
//!
//! Solange nicht `Playing` gilt, ist die **virtuelle Zeit** angehalten.
//! Dadurch laufen keine festen Ticks mehr, die Interpolation bleibt stehen
//! und alles friert sauber ein – ohne dass jedes System es selbst prüfen muss.

use bevy::prelude::*;

pub struct StatesPlugin;

impl Plugin for StatesPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .add_sub_state::<InGameState>()
            .add_systems(OnEnter(InGameState::Playing), unpause_virtual_time)
            .add_systems(OnExit(InGameState::Playing), pause_virtual_time)
            .add_systems(OnEnter(InGameState::Dying), start_death_timer)
            .add_systems(
                Update,
                (
                    toggle_pause.run_if(in_state(AppState::InGame)),
                    tick_death_timer.run_if(in_state(InGameState::Dying)),
                ),
            );
    }
}

/// `scoped_entities` aktiviert `DespawnOnExit(Zustand)`: Entities mit dieser
/// Komponente werden beim Verlassen des Zustands automatisch entfernt.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[states(scoped_entities)]
pub enum AppState {
    #[default]
    MainMenu,
    InGame,
    GameOver,
}

/// Sub-State: wird beim Betreten von `AppState::InGame` automatisch mit dem
/// Default (`Playing`) angelegt und beim Verlassen wieder entfernt.
#[derive(SubStates, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[source(AppState = AppState::InGame)]
#[states(scoped_entities)]
pub enum InGameState {
    #[default]
    Playing,
    Paused,
    /// Kamera schwenkt zum Nachbarraum. Spiellogik steht still.
    RoomTransition,
    /// Kurze Pause nach dem Tod, bevor der Game-Over-Screen kommt.
    Dying,
}

const DEATH_DELAY_SECS: f32 = 1.0;

#[derive(Resource)]
struct DeathTimer(f32);

fn pause_virtual_time(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

fn unpause_virtual_time(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<InGameState>>,
    mut next: ResMut<NextState<InGameState>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    match state.get() {
        InGameState::Playing => next.set(InGameState::Paused),
        InGameState::Paused => next.set(InGameState::Playing),
        // Während Raumwechsel und Sterben gibt es kein Pausemenü.
        InGameState::RoomTransition | InGameState::Dying => {}
    }
}

fn start_death_timer(mut commands: Commands) {
    commands.insert_resource(DeathTimer(DEATH_DELAY_SECS));
}

/// Nutzt `Time<Real>`, weil die virtuelle Zeit während `Dying` angehalten ist.
fn tick_death_timer(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut timer: ResMut<DeathTimer>,
    mut app_state: ResMut<NextState<AppState>>,
) {
    timer.0 -= time.delta_secs();
    if timer.0 <= 0.0 {
        commands.remove_resource::<DeathTimer>();
        app_state.set(AppState::GameOver);
    }
}
