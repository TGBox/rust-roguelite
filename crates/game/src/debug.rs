//! Debug-Werkzeuge:
//! - F1: Hitboxen anzeigen
//! - K:  Spieler sofort sterben lassen (bis es in M5 echten Schaden gibt)
//! - F4: alle Gegner im Raum entfernen (Türen öffnen sich)
//! - F5: sofort zur nächsten Etage (auf der letzten: Sieg)
//! - Beim Betreten des Hauptmenüs wird geprüft, ob Spielwelt-Entities übrig sind.
//! - Bilder über 25 ms werden mit Kontext geloggt (Ruckler-Diagnose).
//! - Alle 5 s eine Frame-Statistik; F3 schaltet VSync um.

use bevy::{
    input::common_conditions::{input_just_pressed, input_toggle_active},
    prelude::*,
    window::PresentMode,
};

use crate::{
    enemy::Enemy,
    physics::{Body, BodyKind},
    progress::Descend,
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
                clear_room
                    .run_if(in_state(InGameState::Playing))
                    .run_if(input_just_pressed(KeyCode::F4)),
                skip_floor
                    .run_if(in_state(InGameState::Playing))
                    .run_if(input_just_pressed(KeyCode::F5)),
            ),
        )
        .add_systems(OnEnter(AppState::MainMenu), log_entity_count)
        // Frame-Diagnose: zählt feste Ticks pro Bild und meldet lange Bilder.
        .init_resource::<FixedTicksThisFrame>()
        .add_systems(FixedFirst, count_fixed_tick)
        .add_systems(
            RunFixedMainLoop,
            (log_frame_hitches, log_frame_stats)
                .in_set(RunFixedMainLoopSystems::AfterFixedMainLoop)
                .run_if(in_state(InGameState::Playing)),
        )
        .add_systems(Update, toggle_vsync.run_if(input_just_pressed(KeyCode::F3)));
    }
}

/// F3: VSync an/aus. Zeigt, ob die Ruckler vom verpassten Bildschirm-Takt kommen.
fn toggle_vsync(mut window: Single<&mut Window>) {
    window.present_mode = match window.present_mode {
        PresentMode::AutoNoVsync => PresentMode::AutoVsync,
        _ => PresentMode::AutoNoVsync,
    };
    info!("Present-Modus: {:?}", window.present_mode);
}

/// Statistik über 5-Sekunden-Fenster: Durchschnitt, Maximum, Anzahl Ruckler.
#[derive(Default)]
struct FrameStats {
    elapsed: f32,
    frames: u32,
    max_ms: f32,
    hitches: u32,
}

fn log_frame_stats(time: Res<Time<Real>>, mut stats: Local<FrameStats>) {
    let dt = time.delta_secs();
    let ms = dt * 1000.0;
    stats.elapsed += dt;
    stats.frames += 1;
    stats.max_ms = stats.max_ms.max(ms);
    if ms > HITCH_THRESHOLD_MS {
        stats.hitches += 1;
    }
    if stats.elapsed >= 5.0 {
        info!(
            "Frames: Ø {:.2} ms ({:.0} FPS) | max {:.1} ms | Ruckler: {}",
            stats.elapsed * 1000.0 / stats.frames as f32,
            stats.frames as f32 / stats.elapsed,
            stats.max_ms,
            stats.hitches
        );
        *stats = FrameStats::default();
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

fn skip_floor(mut descend: MessageWriter<Descend>) {
    descend.write(Descend);
}

fn clear_room(mut commands: Commands, enemies: Query<Entity, With<Enemy>>) {
    for e in &enemies {
        commands.entity(e).despawn();
    }
}

fn kill_player(mut next: ResMut<NextState<InGameState>>) {
    next.set(InGameState::Dying);
}

/// Alles, was zur Spielwelt gehört, hat ein `Sprite` oder einen `Body`.
/// Im Hauptmenü muss diese Zahl 0 sein.
fn log_entity_count(world_entities: Query<(), Or<(With<Sprite>, With<Body>)>>) {
    let count = world_entities.iter().count();
    if count == 0 {
        info!("Hauptmenü: keine Spielwelt-Entities übrig ✔");
    } else {
        warn!("Hauptmenü: {count} Spielwelt-Entities wurden nicht aufgeräumt!");
    }
}
