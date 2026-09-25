//! Einstiegspunkt: baut die Bevy-App nur aus Plugins zusammen.
//! Jede Fachlogik lebt in ihrem eigenen Modul mit eigenem Plugin.

use bevy::prelude::*;

mod assets;
mod camera;
mod debug;
mod physics;
mod player;
mod projectile;
mod room;
mod run;
mod schedule;
mod states;
mod ui;

/// Kantenlänge einer Kachel in Weltkoordinaten (Pixel bei Zoom 1).
pub const TILE_SIZE: f32 = 32.0;

/// Takt der Spiellogik. Unabhängig von der Bildrate des Monitors.
const FIXED_HZ: f64 = 64.0;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Rust Roguelite".into(),
                        resolution: (1280, 720).into(),
                        ..default()
                    }),
                    ..default()
                })
                // Pixel-Art später scharf statt verschwommen skalieren.
                .set(ImagePlugin::default_nearest()),
        )
        .insert_resource(Time::<Fixed>::from_hz(FIXED_HZ))
        .add_plugins((
            // Zuerst: andere Plugins hängen Systeme an diese Zustände.
            states::StatesPlugin,
            schedule::SchedulePlugin,
            assets::GameAssetsPlugin,
            camera::CameraPlugin,
            run::RunPlugin,
            room::RoomPlugin,
            physics::PhysicsPlugin,
            player::PlayerPlugin,
            projectile::ProjectilePlugin,
            ui::UiPlugin,
            debug::DebugPlugin,
        ))
        .run();
}
