//! Begleiter: Figuren, die Items dem Spieler an die Seite stellen.
//!
//! *Welche* Begleiter es gibt, berechnet `dungeon_gen::items::companions` aus
//! dem Inventar (inklusive Synergien). Hier werden sie nur noch **abgeglichen**
//! (`sync_companions`), bewegt und zum Angreifen gebracht.
//!
//! Begleiter haben `Position`, aber weder `Body` noch `Velocity`: Sie werden
//! von der Physik nicht angefasst und können nicht an Wänden hängen bleiben.
//! Die Interpolation aus `physics.rs` greift trotzdem, weil sie nur
//! `Position` und `PreviousPosition` verlangt.
//!
//! | Art      | Bewegung                 | Angriff                                   |
//! |----------|--------------------------|-------------------------------------------|
//! | Orbital  | kreist um den Spieler    | Berührung, fängt Gegnerschüsse ab         |
//! | Gunner   | folgt dem Spieler        | schießt mit, wenn der Spieler schießt     |
//! | Seeker   | folgt dem Spieler        | schießt selbst auf den nächsten Gegner    |
//! | Hound    | jagt Gegner in der Nähe  | Biss bei Berührung                        |

use std::f32::consts::TAU;

use bevy::prelude::*;
use dungeon_gen::items::{CompanionKind, CompanionSpec, companions};

use crate::{
    TILE_SIZE,
    assets::{ActorArt, GameAssets},
    combat::{Damage, Faction, Health, HitStatus},
    enemy::Enemy,
    item_db::ItemDatabase,
    juice::Fx,
    physics::{Body, BodyKind, Position, PreviousPosition},
    player::{Player, PlayerInput},
    projectile::{Projectile, Shot, shot_bundle},
    room::CurrentRoom,
    run::Run,
    schedule::GameSet,
    states::AppState,
};

/// Abstand der Kugeln zum Spieler (Kacheln) und ihre Drehgeschwindigkeit (rad/s).
const ORBIT_RADIUS_TILES: f32 = 1.3;
const ORBIT_SPEED: f32 = 2.6;
/// Trefferradius einer Kugel (Kacheln).
const ORB_RADIUS_TILES: f32 = 0.28;
const ORB_HIT_DELAY: f32 = 0.4;
const ORB_KNOCKBACK: f32 = 140.0;

/// Abstand der Folger zum Spieler (Kacheln) und wie zügig sie aufschließen (1/s).
const FOLLOW_DISTANCE_TILES: f32 = 0.9;
const FOLLOW_SHARPNESS: f32 = 6.0;
/// Ab dieser Entfernung wird gesprungen statt geflogen (Raumwechsel).
const SNAP_DISTANCE_TILES: f32 = 3.0;

const GUNNER_FIRE_DELAY: f32 = 0.6;
const SEEKER_FIRE_DELAY: f32 = 0.9;
const SEEKER_RANGE_TILES: f32 = 7.0;
const SHOT_SPEED_TILES: f32 = 8.0;
const SHOT_RANGE_TILES: f32 = 6.0;

/// Der Hund jagt nur Gegner, die so nah am **Spieler** sind (Kacheln).
const HOUND_LEASH_TILES: f32 = 5.0;
const HOUND_SPEED_TILES: f32 = 6.5;
const HOUND_BITE_DELAY: f32 = 0.7;
const HOUND_REACH_TILES: f32 = 0.3;

pub struct CompanionPlugin;

impl Plugin for CompanionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                sync_companions,
                // Erst bewegen, dann angreifen – die Angriffe sehen so die
                // Position dieses Ticks.
                (move_orbitals, follow_player, hound_ai),
                (orbital_contact, companions_shoot),
            )
                .chain()
                .in_set(GameSet::Control),
        );
    }
}

/// Ein Begleiter. `order` ist seine Stelle in der Liste aus
/// `dungeon_gen::items::companions` – darüber wird abgeglichen.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Companion {
    spec: CompanionSpec,
    order: usize,
    /// Verbleibende Zeit bis zum nächsten Angriff (s).
    cooldown: f32,
}

/// Kreist um den Spieler. Mehrere Kugeln verteilen sich gleichmäßig.
#[derive(Component, Debug)]
struct Orbit {
    angle: f32,
}

/// Hält sich an einem festen Platz neben dem Spieler auf.
#[derive(Component, Debug)]
struct Follower {
    /// Versatz zum Spieler (Pixel).
    offset: Vec2,
}

/// Markiert den Geisterhund (er ist zusätzlich `Follower`, solange er nicht jagt).
#[derive(Component, Debug)]
struct Hound;

fn art(kind: CompanionKind, assets: &GameAssets) -> &ActorArt {
    let a = &assets.actors;
    match kind {
        CompanionKind::Orbital => &a.orb,
        CompanionKind::Gunner => &a.gunner,
        CompanionKind::Seeker => &a.seeker,
        CompanionKind::Hound => &a.hound,
    }
}

/// Setzt eine Position. Bei einem großen Sprung (Raumwechsel) wird auch die
/// Vorher-Position mitgenommen – sonst zöge die Interpolation für einen Tick
/// einen Strich quer über den Bildschirm.
fn place(pos: &mut Position, prev: &mut PreviousPosition, target: Vec2) {
    if pos.0.distance(target) > SNAP_DISTANCE_TILES * TILE_SIZE {
        prev.0 = target;
    }
    pos.0 = target;
}

/// Gleicht die vorhandenen Begleiter mit dem Inventar ab. Stimmt die Liste
/// nicht (neues Item, neue Synergie, neuer Run), werden alle neu erzeugt –
/// das ist selten und hält die Logik simpel.
fn sync_companions(
    mut commands: Commands,
    run: Res<Run>,
    db: Res<ItemDatabase>,
    assets: Res<GameAssets>,
    player: Single<&Position, With<Player>>,
    existing: Query<(Entity, &Companion)>,
) {
    // Change Detection als billige Abkürzung: Ohne Änderung an Run oder
    // Datenbank kann sich an den Begleitern nichts geändert haben.
    if !run.is_changed() && !db.is_changed() {
        return;
    }
    let inv = &run.inventory;
    let loadout = db.0.loadout(&inv.items, inv.active.as_ref().map(|a| &a.id));
    let wanted = companions(&loadout);

    let mut current: Vec<(usize, CompanionSpec)> =
        existing.iter().map(|(_, c)| (c.order, c.spec)).collect();
    current.sort_by_key(|(order, _)| *order);
    // `Iterator::eq` vergleicht zwei Folgen Element für Element.
    if current
        .iter()
        .map(|(_, spec)| *spec)
        .eq(wanted.iter().copied())
    {
        return;
    }

    for (entity, _) in &existing {
        commands.entity(entity).despawn();
    }
    let orbitals = wanted
        .iter()
        .filter(|s| s.kind == CompanionKind::Orbital)
        .count();
    let (mut orbital_index, mut follower_index) = (0, 0);
    for (order, spec) in wanted.iter().copied().enumerate() {
        let look = art(spec.kind, &assets);
        let mut entity = commands.spawn((
            Name::new(format!("{:?}", spec.kind)),
            Companion {
                spec,
                order,
                cooldown: 0.0,
            },
            DespawnOnExit(AppState::InGame),
            Position(player.0),
            PreviousPosition(player.0),
            // z = 11: knapp über dem Spieler (10).
            Transform::from_translation(player.0.extend(11.0)),
            Sprite {
                image: look.image.clone(),
                custom_size: Some(look.size),
                // Leicht durchscheinend: Begleiter sind keine Gegner.
                color: Color::WHITE.with_alpha(0.9),
                ..default()
            },
        ));
        if spec.kind == CompanionKind::Orbital {
            entity.insert(Orbit {
                angle: TAU * orbital_index as f32 / orbitals as f32,
            });
            orbital_index += 1;
        } else {
            // Plätze rund um den Spieler; 1,9 rad Abstand verteilt auch
            // viele Folger, ohne dass zwei aufeinandersitzen.
            let angle = 2.4 + follower_index as f32 * 1.9;
            entity.insert(Follower {
                offset: Vec2::from_angle(angle) * FOLLOW_DISTANCE_TILES * TILE_SIZE,
            });
            follower_index += 1;
            if spec.kind == CompanionKind::Hound {
                entity.insert(Hound);
            }
        }
    }
}

/// `Without<Player>`: Der Spieler wird hier gelesen, die Begleiter werden
/// geschrieben. Der Filter beweist Bevy, dass sich beide Abfragen nie dieselbe
/// `Position` teilen – sonst würde das System beim Start abgelehnt.
fn move_orbitals(
    time: Res<Time>,
    player: Single<&Position, With<Player>>,
    mut orbs: Query<(&mut Position, &mut PreviousPosition, &mut Orbit), Without<Player>>,
) {
    let dt = time.delta_secs();
    for (mut pos, mut prev, mut orbit) in &mut orbs {
        orbit.angle = (orbit.angle + ORBIT_SPEED * dt) % TAU;
        let target = player.0 + Vec2::from_angle(orbit.angle) * ORBIT_RADIUS_TILES * TILE_SIZE;
        place(&mut pos, &mut prev, target);
    }
}

fn follow_player(
    time: Res<Time>,
    player: Single<&Position, With<Player>>,
    mut followers: Query<
        (&mut Position, &mut PreviousPosition, &Follower),
        (Without<Player>, Without<Hound>),
    >,
) {
    let t = 1.0 - (-FOLLOW_SHARPNESS * time.delta_secs()).exp();
    for (mut pos, mut prev, follower) in &mut followers {
        let anchor = player.0 + follower.offset;
        let target = if pos.0.distance(anchor) > SNAP_DISTANCE_TILES * TILE_SIZE {
            anchor
        } else {
            pos.0.lerp(anchor, t)
        };
        place(&mut pos, &mut prev, target);
    }
}

/// Der Hund läuft zum nächsten Gegner in der Nähe des Spielers und beißt zu.
/// Ohne Ziel kehrt er an seinen Platz zurück.
fn hound_ai(
    time: Res<Time>,
    player: Single<&Position, With<Player>>,
    mut hounds: Query<
        (
            &mut Position,
            &mut PreviousPosition,
            &mut Companion,
            &Follower,
        ),
        (With<Hound>, Without<Player>, Without<Enemy>),
    >,
    enemies: Query<(Entity, &Position, &Body, &Health), With<Enemy>>,
    mut damage: MessageWriter<Damage>,
) {
    let dt = time.delta_secs();
    let leash = HOUND_LEASH_TILES * TILE_SIZE;
    for (mut pos, mut prev, mut hound, follower) in &mut hounds {
        hound.cooldown = (hound.cooldown - dt).max(0.0);

        // Nächster lebender Gegner innerhalb der Leine – gemessen vom Spieler,
        // damit der Hund nicht quer durch den Raum davonläuft.
        let prey = enemies
            .iter()
            .filter(|(_, p, _, health)| health.current > 0.0 && p.0.distance(player.0) < leash)
            .min_by(|a, b| a.1.0.distance(pos.0).total_cmp(&b.1.0.distance(pos.0)));

        let Some((enemy, enemy_pos, body, _)) = prey else {
            let anchor = player.0 + follower.offset;
            let t = 1.0 - (-FOLLOW_SHARPNESS * dt).exp();
            let target = if pos.0.distance(anchor) > SNAP_DISTANCE_TILES * TILE_SIZE {
                anchor
            } else {
                pos.0.lerp(anchor, t)
            };
            place(&mut pos, &mut prev, target);
            continue;
        };

        let to_prey = enemy_pos.0 - pos.0;
        let reach = body.half_size.x + HOUND_REACH_TILES * TILE_SIZE;
        if to_prey.length() > reach {
            // Höchstens bis zum Ziel laufen, nicht darüber hinaus.
            let step = (HOUND_SPEED_TILES * TILE_SIZE * dt).min(to_prey.length() - reach);
            let target = pos.0 + to_prey.normalize_or_zero() * step;
            place(&mut pos, &mut prev, target);
        } else if hound.cooldown <= 0.0 {
            hound.cooldown = HOUND_BITE_DELAY;
            damage.write(Damage {
                target: enemy,
                amount: hound.spec.damage,
                knockback: to_prey.normalize_or_zero() * ORB_KNOCKBACK,
                status: HitStatus::NONE,
            });
        }
    }
}

/// Kugeln schaden Gegnern bei Berührung und löschen Gegnerschüsse aus.
fn orbital_contact(
    mut commands: Commands,
    time: Res<Time>,
    mut orbs: Query<(&Position, &mut Companion), With<Orbit>>,
    enemies: Query<(Entity, &Position, &Body, &Health), With<Enemy>>,
    shots: Query<(Entity, &Position, &Projectile)>,
    mut damage: MessageWriter<Damage>,
    mut fx: MessageWriter<Fx>,
) {
    let dt = time.delta_secs();
    let radius = ORB_RADIUS_TILES * TILE_SIZE;
    for (pos, mut orb) in &mut orbs {
        orb.cooldown = (orb.cooldown - dt).max(0.0);

        if orb.cooldown <= 0.0 {
            let touched = enemies.iter().find(|(_, enemy_pos, body, health)| {
                health.current > 0.0 && enemy_pos.0.distance(pos.0) < radius + body.half_size.x
            });
            if let Some((enemy, enemy_pos, _, _)) = touched {
                orb.cooldown = ORB_HIT_DELAY;
                damage.write(Damage {
                    target: enemy,
                    amount: orb.spec.damage,
                    knockback: (enemy_pos.0 - pos.0).normalize_or_zero() * ORB_KNOCKBACK,
                    status: HitStatus::NONE,
                });
            }
        }

        // Schüsse abfangen kostet keine Abklingzeit – das ist der Schutz,
        // für den man die Kugel mitnimmt.
        for (shot, shot_pos, projectile) in &shots {
            if projectile.faction == Faction::Enemy && shot_pos.0.distance(pos.0) < radius * 1.5 {
                commands.entity(shot).try_despawn();
                fx.write(Fx::Burst {
                    at: shot_pos.0,
                    color: Color::srgb(1.0, 0.85, 0.35),
                    count: 5,
                    speed: 70.0,
                });
            }
        }
    }
}

/// Gunner schießen in die Schussrichtung des Spielers, Seeker auf den
/// nächsten sichtbaren Gegner.
fn companions_shoot(
    mut commands: Commands,
    time: Res<Time>,
    input: Res<PlayerInput>,
    room: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    mut shooters: Query<(&Position, &mut Companion)>,
    enemies: Query<(&Position, &Health), With<Enemy>>,
) {
    let dt = time.delta_secs();
    let speed = SHOT_SPEED_TILES * TILE_SIZE;
    for (pos, mut companion) in &mut shooters {
        // Je Art: Schussrichtung (falls es ein Ziel gibt) und Pause danach.
        let (aim, delay) = match companion.spec.kind {
            CompanionKind::Gunner => (input.shoot_dir, GUNNER_FIRE_DELAY),
            CompanionKind::Seeker => (
                nearest_visible_enemy(pos.0, &room, &enemies),
                SEEKER_FIRE_DELAY,
            ),
            CompanionKind::Orbital | CompanionKind::Hound => continue,
        };
        companion.cooldown = (companion.cooldown - dt).max(0.0);
        let Some(dir) = aim else {
            continue;
        };
        if companion.cooldown > 0.0 {
            continue;
        }
        companion.cooldown = delay;
        commands.spawn(shot_bundle(
            Shot::plain(
                pos.0,
                dir * speed,
                SHOT_RANGE_TILES * TILE_SIZE / speed,
                companion.spec.damage,
                Faction::Player,
            ),
            &assets,
        ));
    }
}

/// Richtung zum nächsten lebenden Gegner in Reichweite, auf den freie Schussbahn besteht.
fn nearest_visible_enemy(
    from: Vec2,
    room: &CurrentRoom,
    enemies: &Query<(&Position, &Health), With<Enemy>>,
) -> Option<Vec2> {
    let range = SEEKER_RANGE_TILES * TILE_SIZE;
    enemies
        .iter()
        .filter(|(_, health)| health.current > 0.0)
        .map(|(pos, _)| pos.0 - from)
        .filter(|offset| offset.length() < range)
        .filter(|offset| room.line_of_sight(from, from + *offset, BodyKind::Projectile))
        .min_by(|a, b| a.length().total_cmp(&b.length()))
        .map(|offset| offset.normalize_or_zero())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_moves_keep_the_previous_position_for_interpolation() {
        let mut pos = Position(Vec2::ZERO);
        let mut prev = PreviousPosition(Vec2::ZERO);
        place(&mut pos, &mut prev, Vec2::new(10.0, 0.0));
        assert_eq!(pos.0, Vec2::new(10.0, 0.0));
        assert_eq!(prev.0, Vec2::ZERO);
    }

    #[test]
    fn big_jumps_snap_both_positions() {
        let mut pos = Position(Vec2::ZERO);
        let mut prev = PreviousPosition(Vec2::ZERO);
        let far = Vec2::new(20.0 * TILE_SIZE, 0.0);
        place(&mut pos, &mut prev, far);
        assert_eq!(pos.0, far);
        assert_eq!(prev.0, far);
    }
}
