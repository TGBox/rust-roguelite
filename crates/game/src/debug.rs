//! Debug-Werkzeuge:
//! - F1: Hitboxen anzeigen
//! - K:  Spieler sofort sterben lassen (bis es in M5 echten Schaden gibt)
//! - Beim Betreten des Hauptmenüs wird geprüft, ob Spielwelt-Entities übrig sind.
//! - Bilder über 25 ms werden mit Kontext geloggt (Ruckler-Diagnose).

use bevy::{
    input::common_conditions::{input_just_pressed, input_toggle_active},
    prelude::*,
};

use crate::{
    physics::{Body, BodyKind},
    projectile::Projectile,
    states::{AppState, InGameState},
};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                draw_hitboxes.run_if(input_toggle_active(false, KeyCode::F1)),
                // Zwei `run_if` hintereinander = beide Bedingungen müssen gelten.
                kill_player
                    .run_if(in_state(InGameState::Playing))
                    .run_if(input_just_pressed(KeyCode::KeyK)),
            ),
        )
        .add_systems(OnEnter(AppState::MainMenu), log_entity_count)
        // Frame-Diagnose: zählt feste Ticks pro Bild und meldet lange Bilder.
        .init_resource::<FixedTicksThisFrame>()
        .add_systems(FixedFirst, count_fixed_tick)
        .add_systems(
            RunFixedMainLoop,
            log_frame_hitches
                .in_set(RunFixedMainLoopSystems::AfterFixedMainLoop)
                .run_if(in_state(InGameState::Playing)),
        );
    }
}

/// Ab dieser Dauer gilt ein Bild als „Ruckler“ (60 Hz ≈ 16,7 ms).
const HITCH_THRESHOLD_MS: f32 = 25.0;

#[derive(Resource, Default)]
struct FixedTicksThisFrame(u32);

fn count_fixed_tick(mut ticks: ResMut<FixedTicksThisFrame>) {
    ticks.0 += 1;
}

fn log_frame_hitches(
    time: Res<Time<Real>>,
    mut ticks: ResMut<FixedTicksThisFrame>,
    projectiles: Query<(), With<Projectile>>,
) {
    let frame_ms = time.delta_secs() * 1000.0;
    if frame_ms > HITCH_THRESHOLD_MS {
        warn!(
            "Langes Bild: {frame_ms:.1} ms | feste Ticks in diesem Bild: {} | Projektile: {}",
            ticks.0,
            projectiles.iter().count()
        );
    }
    ticks.0 = 0;
}

/// Zeichnet an der interpolierten `Transform`-Position, also genau dort,
/// wo das Objekt gerade zu sehen ist.
fn draw_hitboxes(mut gizmos: Gizmos, query: Query<(&Transform, &Body)>) {
    for (transform, body) in &query {
        let color = match body.kind {
            BodyKind::Walker => Color::srgb(0.2, 1.0, 0.3),
            BodyKind::Projectile => Color::srgb(1.0, 0.3, 0.3),
        };
        gizmos.rect_2d(
            Isometry2d::from_translation(transform.translation.truncate()),
            body.half_size * 2.0,
            color,
        );
    }
}

fn kill_player(mut next: ResMut<NextState<InGameState>>) {
    next.set(InGameState::Dying);
}

/// Alles, was zur Spielwelt gehört, hat ein `Mesh2d` oder einen `Body`.
/// Im Hauptmenü muss diese Zahl 0 sein.
fn log_entity_count(world_entities: Query<(), Or<(With<Mesh2d>, With<Body>)>>) {
    let count = world_entities.iter().count();
    if count == 0 {
        info!("Hauptmenü: keine Spielwelt-Entities übrig ✔");
    } else {
        warn!("Hauptmenü: {count} Spielwelt-Entities wurden nicht aufgeräumt!");
    }
}
