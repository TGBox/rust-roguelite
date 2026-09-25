//! 2D-Kamera, die immer genau einen Raum zeigt – egal wie groß das Fenster ist.

use bevy::{camera::ScalingMode, prelude::*};
use dungeon_gen::room::{ROOM_HEIGHT, ROOM_WIDTH};

use crate::TILE_SIZE;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.07)))
            .add_systems(Startup, spawn_camera);
    }
}

/// Marker-Komponente, damit andere Systeme gezielt „die“ Spielkamera abfragen
/// können (`Query<&Transform, With<MainCamera>>`), auch wenn später weitere
/// Kameras (z. B. für die Minimap) dazukommen.
#[derive(Component)]
pub struct MainCamera;

fn spawn_camera(mut commands: Commands) {
    // Ein Rand von einer halben Kachel rundherum, damit der Raum nicht am Fensterrand klebt.
    let min_width = (ROOM_WIDTH as f32 + 1.0) * TILE_SIZE;
    let min_height = (ROOM_HEIGHT as f32 + 1.0) * TILE_SIZE;

    commands.spawn((
        Name::new("MainCamera"),
        MainCamera,
        Camera2d,
        // AutoMin: skaliert so, dass MINDESTENS diese Weltfläche sichtbar ist.
        // Bei anderem Seitenverhältnis sieht man einfach etwas mehr Hintergrund.
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width,
                min_height,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
}
