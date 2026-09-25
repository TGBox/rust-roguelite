//! Bewegung, Kollision mit dem Raum und Render-Interpolation.
//!
//! Die Logik arbeitet auf `Position`, nicht auf `Transform`:
//! - `Position` ändert sich nur im festen 64-Hz-Takt (deterministisch).
//! - `Transform` ist nur die Darstellung. Sie wird jedes Bild zwischen
//!   vorheriger und aktueller Position interpoliert. So ruckelt nichts, auch
//!   wenn der Monitor mit 144 Hz läuft.

use bevy::prelude::*;
use dungeon_gen::{GridPos, collision};

use crate::{
    TILE_SIZE,
    room::{CurrentRoom, tile_space_to_world, world_to_tile_space},
    schedule::GameSet,
};

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TileHit>()
            .add_systems(
                FixedUpdate,
                (
                    snapshot_positions.in_set(GameSet::Snapshot),
                    move_bodies.in_set(GameSet::Physics),
                ),
            )
            .add_systems(
                RunFixedMainLoop,
                interpolate_transforms.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop),
            );
    }
}

/// Logische Position in Weltkoordinaten (Pixel).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Deref, DerefMut)]
pub struct Position(pub Vec2);

/// Position am Ende des vorherigen Ticks – nur für die Interpolation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Deref, DerefMut)]
pub struct PreviousPosition(pub Vec2);

/// Geschwindigkeit in Pixel pro Sekunde.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Deref, DerefMut)]
pub struct Velocity(pub Vec2);

/// Hitbox gegen den Raum.
#[derive(Component, Debug, Clone, Copy)]
pub struct Body {
    /// Halbe Kantenlängen in Pixeln.
    pub half_size: Vec2,
    pub kind: BodyKind,
}

/// Bestimmt, welche Kacheln blockieren.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// Läuft: blockiert von Wand, Fels, Grube.
    Walker,
    /// Fliegt: blockiert von Wand und Fels.
    Projectile,
}

/// Wird geschrieben, wenn ein Körper gegen eine blockierende Kachel stößt.
/// Andere Module entscheiden, was daraus folgt (Projektil zerplatzt, …).
#[derive(Message, Debug, Clone, Copy)]
pub struct TileHit {
    pub entity: Entity,
}

/// Alle Physik-Komponenten mit **gleicher** Start- und Vorher-Position.
/// Sonst würde ein neues Objekt im ersten Bild vom Ursprung „einfliegen“.
///
/// Gibt `impl Bundle` zurück: Tupel aus Komponenten sind selbst Bundles
/// und lassen sich in `commands.spawn((…))` einfach verschachteln.
pub fn physics_body(position: Vec2, velocity: Vec2, body: Body, z: f32) -> impl Bundle {
    (
        Position(position),
        PreviousPosition(position),
        Velocity(velocity),
        body,
        Transform::from_translation(position.extend(z)),
    )
}

fn snapshot_positions(mut query: Query<(&Position, &mut PreviousPosition)>) {
    for (pos, mut prev) in &mut query {
        prev.0 = pos.0;
    }
}

fn move_bodies(
    time: Res<Time>,
    room: Res<CurrentRoom>,
    mut query: Query<(Entity, &mut Position, &mut Velocity, &Body)>,
    mut hits: MessageWriter<TileHit>,
) {
    // In `FixedUpdate` liefert `Time` automatisch die feste Schrittweite.
    let dt = time.delta_secs();

    for (entity, mut pos, mut vel, body) in &mut query {
        let is_solid = |p: GridPos| match body.kind {
            BodyKind::Walker => room.0.blocks_movement(p),
            BodyKind::Projectile => room.0.blocks_projectiles(p),
        };

        let result = collision::move_and_slide(
            world_to_tile_space(pos.0).to_array(),
            (body.half_size / TILE_SIZE).to_array(),
            (vel.0 * dt / TILE_SIZE).to_array(),
            is_solid,
        );

        pos.0 = tile_space_to_world(Vec2::from_array(result.center));
        // Gegen die Wand gelaufen: Geschwindigkeit auf dieser Achse vernichten,
        // sonst „klebt“ man beim Loslassen noch einen Moment an der Wand.
        if result.hit_x {
            vel.0.x = 0.0;
        }
        if result.hit_y {
            vel.0.y = 0.0;
        }
        if result.hit_any() {
            hits.write(TileHit { entity });
        }
    }
}

fn interpolate_transforms(
    fixed_time: Res<Time<Fixed>>,
    mut query: Query<(&mut Transform, &Position, &PreviousPosition)>,
) {
    // Wie weit sind wir zwischen letztem und nächstem Tick? (0.0 bis 1.0)
    let alpha = fixed_time.overstep_fraction();
    for (mut transform, pos, prev) in &mut query {
        let p = prev.0.lerp(pos.0, alpha);
        // Nur x/y überschreiben – z bestimmt die Zeichenreihenfolge.
        transform.translation.x = p.x;
        transform.translation.y = p.y;
    }
}
