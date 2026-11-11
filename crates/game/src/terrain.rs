//! Terrain, das mehr tut als im Weg zu stehen: Stacheln und Pulverfässer.
//!
//! Die Regeln, *welche* Kachel was blockiert, stehen in `dungeon_gen::room`
//! (`Tile`). Hier passiert nur, was Bevy braucht:
//! - **Stacheln** verletzen alles, was darauf läuft – Spieler wie Gegner.
//!   Flieger und Bosse sind ausgenommen.
//! - **Pulverfässer** explodieren, wenn ein Schuss sie trifft. Die Explosion
//!   selbst übernimmt `bomb.rs` (inklusive Kettenreaktion).

use bevy::prelude::*;
use dungeon_gen::{GridPos, Tile};

use crate::{
    TILE_SIZE,
    bomb::PendingExplosions,
    combat::{Damage, Health, HitStatus},
    enemy::Boss,
    physics::{Body, BodyKind, Position, TileHit},
    player::Player,
    projectile::{Projectile, despawn_on_tile_hit},
    room::CurrentRoom,
    schedule::GameSet,
};

/// Ein halbes Herz für den Spieler.
const SPIKE_DAMAGE_PLAYER: f32 = 1.0;
const SPIKE_DAMAGE_ENEMY: f32 = 2.0;
/// Abstand zwischen zwei Stichen (s). Gleich lang wie die Unverwundbarkeit
/// des Spielers – wer stehen bleibt, verliert ein halbes Herz pro Sekunde.
const SPIKE_INTERVAL_SECS: f32 = 1.0;
/// Wie weit hinter der Kante eines Schusses nach einem Fass getastet wird (Kacheln).
const BARREL_PROBE_TILES: f32 = 0.15;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, spike_damage.in_set(GameSet::Control))
            .add_systems(
                FixedUpdate,
                // Vor dem Aufräumen der Schüsse: Danach gäbe es den Schuss
                // (und damit seine Position) womöglich nicht mehr.
                shots_hit_barrels
                    .in_set(GameSet::Cleanup)
                    .before(despawn_on_tile_hit),
            );
    }
}

/// Verbleibende Schonzeit nach einem Stich (s).
#[derive(Component, Debug)]
struct SpikeCooldown(f32);

/// Wer mit der Körpermitte auf Stacheln steht, nimmt Schaden – höchstens
/// einmal pro `SPIKE_INTERVAL_SECS`.
///
/// Läuft in `Control`, also vor `Combat`: Die `Damage`-Message wird noch im
/// selben Tick von `combat::apply_damage` verarbeitet.
fn spike_damage(
    mut commands: Commands,
    time: Res<Time>,
    room: Res<CurrentRoom>,
    mut walkers: Query<
        (
            Entity,
            &Position,
            &Body,
            Has<Player>,
            Option<&mut SpikeCooldown>,
        ),
        (With<Health>, Without<Boss>),
    >,
    mut damage: MessageWriter<Damage>,
) {
    let dt = time.delta_secs();
    for (entity, pos, body, is_player, cooldown) in &mut walkers {
        // Schonzeit läuft immer ab, auch abseits der Stacheln.
        let ready = match cooldown {
            Some(mut cooldown) => {
                cooldown.0 -= dt;
                cooldown.0 <= 0.0
            }
            None => true,
        };
        if !ready || body.kind != BodyKind::Walker {
            continue;
        }
        if !room.layout.is_hazard(room.tile_at(pos.0)) {
            continue;
        }
        damage.write(Damage {
            target: entity,
            amount: if is_player {
                SPIKE_DAMAGE_PLAYER
            } else {
                SPIKE_DAMAGE_ENEMY
            },
            knockback: Vec2::ZERO,
            status: HitStatus::NONE,
        });
        // `insert` ersetzt eine vorhandene Komponente. `try_insert`, falls
        // das Ziel in diesem Tick schon anderweitig entfernt wurde.
        commands
            .entity(entity)
            .try_insert(SpikeCooldown(SPIKE_INTERVAL_SECS));
    }
}

/// Prallt ein Schuss gegen ein Pulverfass, wird dessen Explosion angemeldet.
///
/// `TileHit` sagt nur, *dass* und auf welcher Achse ein Körper angestoßen ist,
/// nicht woran. Deshalb tasten wir direkt hinter der Vorderkante des Schusses
/// nach, welche Kacheln dort liegen.
fn shots_hit_barrels(
    mut hits: MessageReader<TileHit>,
    room: Res<CurrentRoom>,
    shots: Query<(&Position, &Body), With<Projectile>>,
    mut pending: ResMut<PendingExplosions>,
) {
    for hit in hits.read() {
        let Ok((pos, body)) = shots.get(hit.entity) else {
            continue;
        };
        for tile in tiles_ahead(&room, pos.0, body.half_size, hit) {
            if room.layout.get(tile) != Some(Tile::Barrel) {
                continue;
            }
            let center = room.tile_center(tile);
            // Zwei Schüsse im selben Tick sollen das Fass nicht doppelt zünden.
            if !pending.0.contains(&center) {
                pending.0.push(center);
            }
        }
    }
}

/// Kacheln direkt vor einem Schuss, der gerade angestoßen ist: je getroffener
/// Achse drei Tastpunkte entlang der Vorderkante (beide Ecken und die Mitte).
/// Nur die Mitte zu prüfen reicht nicht – ein Schuss kann ein Fass auch mit
/// der Ecke streifen.
fn tiles_ahead(room: &CurrentRoom, pos: Vec2, half: Vec2, hit: &TileHit) -> Vec<GridPos> {
    let reach = BARREL_PROBE_TILES * TILE_SIZE;
    let mut tiles = Vec::new();
    let mut probe = |point: Vec2| {
        let tile = room.tile_at(point);
        if !tiles.contains(&tile) {
            tiles.push(tile);
        }
    };
    // Etwas nach innen gerückt, damit eine Ecke nicht versehentlich die
    // Nachbarkachel neben der Flugbahn erwischt.
    const SIDES: [f32; 3] = [-0.9, 0.0, 0.9];
    if hit.hit_x {
        let x = pos.x + hit.velocity.x.signum() * (half.x + reach);
        for side in SIDES {
            probe(Vec2::new(x, pos.y + side * half.y));
        }
    }
    if hit.hit_y {
        let y = pos.y + hit.velocity.y.signum() * (half.y + reach);
        for side in SIDES {
            probe(Vec2::new(pos.x + side * half.x, y));
        }
    }
    tiles
}

#[cfg(test)]
mod tests {
    use super::*;
    use dungeon_gen::{RoomLayout, floor::START_POS};

    fn room_with(tile: GridPos, kind: Tile) -> CurrentRoom {
        let mut layout = RoomLayout::empty();
        layout.set(tile, kind);
        CurrentRoom {
            pos: START_POS,
            layout,
            locked: false,
            key_locked: Vec::new(),
            revision: 0,
            wave: 0,
            hidden_secret: None,
        }
    }

    fn hit(velocity: Vec2, hit_x: bool, hit_y: bool) -> TileHit {
        TileHit {
            entity: Entity::PLACEHOLDER,
            hit_x,
            hit_y,
            velocity,
        }
    }

    #[test]
    fn shot_flying_right_finds_the_barrel_in_front() {
        let barrel = GridPos::new(6, 4);
        let room = room_with(barrel, Tile::Barrel);
        let half = Vec2::splat(0.15 * TILE_SIZE);
        // Schuss liegt bündig links am Fass an.
        let center = room.tile_center(barrel);
        let pos = Vec2::new(center.x - 0.5 * TILE_SIZE - half.x, center.y);
        let ahead = tiles_ahead(&room, pos, half, &hit(Vec2::new(300.0, 0.0), true, false));
        assert_eq!(ahead, vec![barrel]);
    }

    #[test]
    fn grazing_shot_still_finds_the_barrel() {
        let barrel = GridPos::new(6, 4);
        let room = room_with(barrel, Tile::Barrel);
        let half = Vec2::splat(0.15 * TILE_SIZE);
        let center = room.tile_center(barrel);
        // Von oben kommend, seitlich so versetzt, dass nur die rechte Ecke
        // des Schusses über dem Fass liegt.
        let pos = Vec2::new(
            center.x - 0.5 * TILE_SIZE - 0.5 * half.x,
            center.y + 0.5 * TILE_SIZE + half.y,
        );
        let ahead = tiles_ahead(&room, pos, half, &hit(Vec2::new(0.0, -300.0), false, true));
        assert!(ahead.contains(&barrel), "{ahead:?}");
    }

    #[test]
    fn nothing_ahead_without_a_hit_axis() {
        let room = room_with(GridPos::new(6, 4), Tile::Barrel);
        let ahead = tiles_ahead(
            &room,
            Vec2::ZERO,
            Vec2::splat(4.0),
            &hit(Vec2::new(100.0, 100.0), false, false),
        );
        assert!(ahead.is_empty());
    }
}
