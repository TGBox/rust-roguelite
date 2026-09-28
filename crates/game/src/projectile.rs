//! Projektile („Tears“): fliegen geradeaus, verschwinden nach Ablauf
//! ihrer Lebenszeit oder beim Aufprall auf Wand/Fels. Treffer auf
//! Spieler/Gegner behandelt `combat.rs`.

use bevy::prelude::*;

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    combat::{Faction, Health, HitStatus},
    juice::Fx,
    physics::{Body, BodyKind, Position, TileHit, Velocity, physics_body},
    room::RoomScoped,
    schedule::GameSet,
    states::AppState,
};

pub const TEAR_RADIUS_TILES: f32 = 0.15;

pub struct ProjectilePlugin;

impl Plugin for ProjectilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (tick_lifetime, despawn_on_tile_hit).in_set(GameSet::Cleanup),
        )
        .add_systems(FixedUpdate, steer_homing_shots.in_set(GameSet::Control));
    }
}

#[derive(Component, Debug)]
pub struct Projectile {
    /// Verbleibende Flugzeit (s).
    pub remaining: f32,
    pub damage: f32,
    /// Wer geschossen hat. Trifft nur die jeweils andere Seite.
    pub faction: Faction,
    /// Fliegt durch Ziele hindurch.
    pub piercing: bool,
    /// Lenkt zum nächsten Ziel.
    pub homing: bool,
    /// Bei durchschlagenden Schüssen: schon getroffene Ziele.
    pub already_hit: Vec<Entity>,
    /// Verbleibende Abpraller an Wänden.
    pub bounces: u32,
    /// Gift/Frost, die dieser Schuss beim Treffer auslöst.
    pub status: HitStatus,
}

/// Alle Parameter eines Schusses. Eine Struktur statt sechs Funktionsargumenten:
/// Die Aufrufer benennen jeden Wert, Verwechslungen sind ausgeschlossen.
#[derive(Debug, Clone, Copy)]
pub struct Shot {
    pub position: Vec2,
    pub velocity: Vec2,
    pub lifetime: f32,
    pub damage: f32,
    pub faction: Faction,
    pub piercing: bool,
    pub homing: bool,
    pub bounces: u32,
    pub status: HitStatus,
    /// Kritischer Treffer (nur Optik – der Schaden ist schon eingerechnet).
    pub crit: bool,
}

impl Shot {
    /// Ein schlichter Schuss ohne Extras (Gegner).
    pub fn plain(
        position: Vec2,
        velocity: Vec2,
        lifetime: f32,
        damage: f32,
        faction: Faction,
    ) -> Self {
        Self {
            position,
            velocity,
            lifetime,
            damage,
            faction,
            piercing: false,
            homing: false,
            bounces: 0,
            status: HitStatus::NONE,
            crit: false,
        }
    }
}

/// Farbe und Größe verraten, was ein Schuss kann.
fn shot_look(shot: &Shot) -> (Color, f32) {
    let color = if shot.status.poison.is_some() {
        Color::srgb(0.55, 1.0, 0.45)
    } else if shot.status.freeze.is_some() {
        Color::srgb(0.75, 1.0, 1.0)
    } else if shot.crit {
        Color::srgb(1.0, 0.9, 0.45)
    } else {
        Color::WHITE
    };
    (color, if shot.crit { 1.5 } else { 1.0 })
}

pub fn shot_bundle(shot: Shot, assets: &GameAssets) -> impl Bundle {
    let art = match shot.faction {
        Faction::Player => &assets.actors.tear,
        Faction::Enemy => &assets.actors.enemy_shot,
    };
    let (color, scale) = shot_look(&shot);
    (
        Name::new("Shot"),
        DespawnOnExit(AppState::InGame),
        RoomScoped,
        Projectile {
            remaining: shot.lifetime,
            damage: shot.damage,
            faction: shot.faction,
            piercing: shot.piercing,
            homing: shot.homing,
            already_hit: Vec::new(),
            bounces: shot.bounces,
            status: shot.status,
        },
        physics_body(
            shot.position,
            shot.velocity,
            Body {
                half_size: Vec2::splat(TEAR_RADIUS_TILES * TILE_SIZE),
                kind: BodyKind::Projectile,
            },
            5.0,
        ),
        Sprite {
            image: art.image.clone(),
            custom_size: Some(art.size * scale),
            color,
            ..default()
        },
    )
}

fn tick_lifetime(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Projectile)>,
) {
    let dt = time.delta_secs();
    for (entity, mut projectile) in &mut query {
        projectile.remaining -= dt;
        if projectile.remaining <= 0.0 {
            // `try_despawn`: kein Warn-Log, falls dasselbe Projektil im selben
            // Tick auch durch einen Treffer entfernt wurde.
            commands.entity(entity).try_despawn();
        }
    }
}

fn despawn_on_tile_hit(
    mut commands: Commands,
    mut hits: MessageReader<TileHit>,
    mut projectiles: Query<(&Position, &mut Projectile, &mut Velocity)>,
    mut fx: MessageWriter<Fx>,
) {
    for hit in hits.read() {
        // Der Physik-Code meldet Treffer für ALLE Körper. Hier interessieren nur Projektile.
        let Ok((pos, mut projectile, mut velocity)) = projectiles.get_mut(hit.entity) else {
            continue;
        };
        if projectile.bounces > 0 {
            // Abprallen: die getroffene Achse spiegeln. Die Physik hat sie auf
            // 0 gesetzt – deshalb rechnen wir mit der Geschwindigkeit davor.
            projectile.bounces -= 1;
            let mut v = hit.velocity;
            if hit.hit_x {
                v.x = -v.x;
            }
            if hit.hit_y {
                v.y = -v.y;
            }
            velocity.0 = v;
            // Nach dem Abprallen darf ein durchschlagender Schuss dasselbe Ziel erneut treffen.
            projectile.already_hit.clear();
            continue;
        }
        commands.entity(hit.entity).try_despawn();
        // Kleiner Spritzer an der Wand.
        fx.write(Fx::Burst {
            at: pos.0,
            color: splash_color(projectile.faction),
            count: 5,
            speed: 60.0,
        });
    }
}

fn splash_color(faction: Faction) -> Color {
    match faction {
        Faction::Player => Color::srgb(0.55, 0.78, 0.98),
        Faction::Enemy => Color::srgb(0.95, 0.35, 0.30),
    }
}

/// Zielsuchende Schüsse drehen sich langsam zum nächsten gegnerischen Ziel.
/// Die Geschwindigkeit (Betrag) bleibt gleich, nur die Richtung ändert sich.
fn steer_homing_shots(
    time: Res<Time>,
    mut shots: Query<(&Position, &mut Velocity, &Projectile)>,
    targets: Query<(&Position, &Faction), (With<Health>, Without<Projectile>)>,
) {
    const SEARCH_RADIUS_TILES: f32 = 5.0;
    const TURN_RATE: f32 = 6.0;

    let dt = time.delta_secs();
    let radius = SEARCH_RADIUS_TILES * TILE_SIZE;
    for (pos, mut vel, projectile) in &mut shots {
        if !projectile.homing {
            continue;
        }
        let nearest = targets
            .iter()
            .filter(|(_, f)| **f != projectile.faction)
            .map(|(p, _)| p.0 - pos.0)
            .filter(|d| d.length() < radius)
            .min_by(|a, b| a.length().total_cmp(&b.length()));
        let Some(to_target) = nearest else {
            continue;
        };
        let speed = vel.0.length();
        let current = vel.0.normalize_or_zero();
        let wanted = to_target.normalize_or_zero();
        let turned = current
            .lerp(wanted, (TURN_RATE * dt).min(1.0))
            .normalize_or_zero();
        vel.0 = turned * speed;
    }
}
