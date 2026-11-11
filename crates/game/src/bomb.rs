//! Bomben: mit E legen, nach kurzer Zündzeit explodieren sie.
//!
//! Eine Explosion
//! - schadet allen Figuren im Radius (auch dem Spieler!),
//! - stößt sie weg,
//! - sprengt Felsen und Pulverfässer im 3×3-Bereich; jedes Fass explodiert
//!   dabei selbst (`RoomLayout::chain_blast` in `dungeon_gen`).
//!
//! Bomben und Fässer teilen sich denselben Weg: Wer etwas hochgehen lassen
//! will, trägt die Stelle in `PendingExplosions` ein. Eine Ressource statt
//! einer Message, weil sie im einen Tick gefüllt und erst im nächsten geleert
//! wird (`terrain.rs` meldet Fass-Treffer nach der Physik).
//!
//! Gesprengte Felsen bleiben weg: Das geänderte Layout wandert in
//! `Run::layout_overrides` und wird beim nächsten Betreten wiederverwendet.

use bevy::prelude::*;
use dungeon_gen::{Tile, room::door_pos};

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    audio::{Effect, Sfx},
    combat::{Damage, Health},
    inventory::Toast,
    juice::Fx,
    physics::Position,
    pixel_art,
    player::{Player, PlayerInput},
    room::{CurrentRoom, DoorTile, RoomScoped, RoomTile},
    run::Run,
    schedule::GameSet,
    states::AppState,
};

/// Wirkradius der Explosion (Kacheln).
pub const BLAST_RADIUS_TILES: f32 = 1.6;
const FUSE_SECS: f32 = 1.5;
const ENEMY_DAMAGE: f32 = 10.0;
/// Ein ganzes Herz – Bomben sind gefährlich, aber nicht tödlich-dumm.
const PLAYER_DAMAGE: f32 = 2.0;
const KNOCKBACK: f32 = 380.0;
const FLASH_SECS: f32 = 0.2;

pub struct BombPlugin;

impl Plugin for BombPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingExplosions>()
            .add_systems(
                FixedUpdate,
                (place_bombs, tick_bombs, resolve_explosions)
                    .chain()
                    .in_set(GameSet::Control),
            )
            // Nichts in den nächsten Run mitschleppen.
            .add_systems(
                OnExit(AppState::InGame),
                |mut pending: ResMut<PendingExplosions>| pending.0.clear(),
            )
            .add_systems(Update, animate_explosions);
    }
}

/// Weltpositionen, an denen im nächsten Durchlauf von `resolve_explosions`
/// etwas explodiert.
#[derive(Resource, Default)]
pub struct PendingExplosions(pub Vec<Vec2>);

#[derive(Component)]
struct Bomb {
    fuse: f32,
}

#[derive(Component)]
struct ExplosionFlash {
    age: f32,
}

fn place_bombs(
    mut commands: Commands,
    mut input: ResMut<PlayerInput>,
    mut run: ResMut<Run>,
    assets: Res<GameAssets>,
    player: Single<&Position, With<Player>>,
) {
    if !input.place_bomb {
        return;
    }
    // Verbrauchen – egal ob wir eine Bombe haben oder nicht.
    input.place_bomb = false;
    if run.inventory.bombs == 0 {
        return;
    }
    run.inventory.bombs -= 1;

    commands.spawn((
        Name::new("Bomb"),
        Bomb { fuse: FUSE_SECS },
        RoomScoped,
        DespawnOnExit(AppState::InGame),
        Sprite {
            image: assets.sprites.bomb.clone(),
            custom_size: Some(pixel_art::display_size(pixel_art::BOMB)),
            ..default()
        },
        // z = 4: über Beute, unter Figuren.
        Transform::from_translation(player.0.extend(4.0)),
    ));
}

/// Zündschnur herunterzählen. Ist sie abgebrannt, wird die Explosion nur
/// **angemeldet** – was sie anrichtet, entscheidet `resolve_explosions`.
fn tick_bombs(
    mut commands: Commands,
    time: Res<Time>,
    mut bombs: Query<(Entity, &Transform, &mut Bomb, &mut Sprite)>,
    mut pending: ResMut<PendingExplosions>,
) {
    let dt = time.delta_secs();
    for (entity, transform, mut bomb, mut sprite) in &mut bombs {
        bomb.fuse -= dt;
        // Kurz vor der Explosion rot blinken.
        let blink = bomb.fuse < 0.6 && (bomb.fuse * 10.0) as i32 % 2 == 0;
        sprite.color = if blink {
            Color::srgb(1.0, 0.35, 0.35)
        } else {
            Color::WHITE
        };
        if bomb.fuse <= 0.0 {
            pending.0.push(transform.translation.truncate());
            commands.entity(entity).despawn();
        }
    }
}

/// Führt alle angemeldeten Explosionen aus – egal ob Bombe oder Pulverfass.
///
/// Ablauf je Explosion:
/// 1. `RoomLayout::chain_blast` räumt Felsen und Fässer ab und meldet, welche
///    Fässer mit hochgegangen sind (reine Logik in `dungeon_gen`, getestet).
/// 2. Für den Auslöser und jedes dieser Fässer: Schaden, Rückstoß, Effekte.
fn resolve_explosions(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut pending: ResMut<PendingExplosions>,
    targets: Query<(Entity, &Position, Has<Player>), With<Health>>,
    mut room: ResMut<CurrentRoom>,
    mut run: ResMut<Run>,
    mut tiles: Query<(Entity, &RoomTile, &mut Sprite)>,
    mut damage: MessageWriter<Damage>,
    mut fx: MessageWriter<Fx>,
    mut sfx: MessageWriter<Sfx>,
    mut toast: ResMut<Toast>,
) {
    // Früh raus, bevor `room` veränderlich benutzt wird: Sonst hielte die
    // Change Detection den Raum in jedem Tick für geändert.
    if pending.0.is_empty() {
        return;
    }
    let radius = BLAST_RADIUS_TILES * TILE_SIZE;
    // `mem::take` nimmt die Liste heraus und lässt eine leere zurück.
    for origin in std::mem::take(&mut pending.0) {
        // 1. Felsen und Fässer sprengen, das geänderte Layout merken.
        // Die Kachel vorher ausrechnen: `room.layout.chain_blast(room.tile_at(..))`
        // in einem Ausdruck würde `room` gleichzeitig veränderlich (für
        // `layout`) und unveränderlich (für `tile_at`) leihen.
        let tile = room.tile_at(origin);
        let chain = room.layout.chain_blast(tile);
        if !chain.destroyed.is_empty() {
            room.revision += 1;
            run.layout_overrides.insert(room.pos, room.layout.clone());
            for (_, room_tile, mut tile_sprite) in &mut tiles {
                if room_tile.room == room.pos && chain.destroyed.contains(&room_tile.tile) {
                    tile_sprite.image = assets.tiles.floor_at(room.pos, room_tile.tile);
                    fx.write(Fx::Burst {
                        at: room.tile_center(room_tile.tile),
                        color: Color::srgb(0.55, 0.52, 0.48),
                        count: 10,
                        speed: 110.0,
                    });
                }
            }
        }

        // Der Auslöser explodiert genau dort, wo er lag; mitgerissene Fässer
        // in der Mitte ihrer Kachel. `skip(1)` überspringt den Auslöser.
        let centers: Vec<Vec2> = std::iter::once(origin)
            .chain(chain.centers.iter().skip(1).map(|&c| room.tile_center(c)))
            .collect();

        for center in centers {
            // 2. Schaden und Rückstoß für alle im Radius.
            for (target, pos, is_player) in &targets {
                let offset = pos.0 - center;
                if offset.length() > radius {
                    continue;
                }
                damage.write(Damage {
                    status: crate::combat::HitStatus::NONE,
                    target,
                    amount: if is_player {
                        PLAYER_DAMAGE
                    } else {
                        ENEMY_DAMAGE
                    },
                    knockback: offset.normalize_or_zero() * KNOCKBACK,
                });
            }

            // 2b. Geheimtür freisprengen, wenn die Explosion nah genug ist.
            let hidden = room.hidden_secret;
            if let Some(dir) = hidden {
                let door = door_pos(dir);
                if room.tile_center(door).distance(center) <= radius + 0.5 * TILE_SIZE {
                    room.hidden_secret = None;
                    room.layout.set(door, Tile::Door);
                    room.revision += 1;
                    run.unlocked.insert(room.pos.neighbor(dir));
                    run.layout_overrides.insert(room.pos, room.layout.clone());
                    for (tile_entity, room_tile, mut tile_sprite) in &mut tiles {
                        if room_tile.room == room.pos && room_tile.tile == door {
                            // Ab jetzt eine normale Tür: `update_door_visuals` kümmert sich.
                            commands.entity(tile_entity).insert(DoorTile);
                            tile_sprite.image = assets.tiles.door_open.clone();
                        }
                    }
                    toast.show("Ein Geheimraum!".to_string());
                    sfx.write(Sfx(Effect::Secret));
                }
            }

            fx.write(Fx::Burst {
                at: center,
                color: Color::srgb(1.0, 0.6, 0.2),
                count: 30,
                speed: 240.0,
            });
            fx.write(Fx::Shake(0.7));
            sfx.write(Sfx(Effect::Explosion));
            fx.write(Fx::Hitstop(0.05));

            // 3. Blitz anzeigen.
            commands.spawn((
                Name::new("ExplosionFlash"),
                ExplosionFlash { age: 0.0 },
                RoomScoped,
                DespawnOnExit(AppState::InGame),
                Mesh2d(assets.explosion_mesh.clone()),
                MeshMaterial2d(assets.explosion_material.clone()),
                Transform::from_translation(center.extend(20.0)).with_scale(Vec3::splat(0.3)),
            ));
        }
    }
}

/// Der Blitz wächst schnell auf volle Größe und verschwindet dann.
fn animate_explosions(
    mut commands: Commands,
    time: Res<Time>,
    mut flashes: Query<(Entity, &mut ExplosionFlash, &mut Transform)>,
) {
    for (entity, mut flash, mut transform) in &mut flashes {
        flash.age += time.delta_secs();
        let t = (flash.age / FLASH_SECS).min(1.0);
        transform.scale = Vec3::splat(0.3 + 0.7 * t);
        if t >= 1.0 {
            commands.entity(entity).despawn();
        }
    }
}
