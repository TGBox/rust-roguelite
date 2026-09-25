//! Debug-Overlay: F1 zeigt die Hitboxen aller Körper.

use bevy::{input::common_conditions::input_toggle_active, prelude::*};

use crate::physics::{Body, BodyKind};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            draw_hitboxes.run_if(input_toggle_active(false, KeyCode::F1)),
        );
    }
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
