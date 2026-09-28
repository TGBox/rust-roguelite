//! Bomben: mit E legen, nach kurzer Zündzeit explodieren sie.
//!
//! Eine Explosion
//! - schadet allen Figuren im Radius (auch dem Spieler!),
//! - stößt sie weg,
//! - sprengt Felsen im 3×3-Bereich (`RoomLayout::blast` in `dungeon_gen`).
//!
//! Gesprengte Felsen bleiben weg: Das geänderte Layout wandert in
//! `Run::layout_overrides` und wird beim nächsten Betreten wiederverwendet.

use bevy::prelude::*;

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    audio::{Effect, Sfx},
    combat::{Damage, Health},
    juice::Fx,
    physics::Position,
    pixel_art,
    player::{Player, PlayerInput},
    room::{CurrentRoom, RoomScoped, RoomTile},
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
        app.add_systems(
            FixedUpdate,
            (place_bombs, tick_bombs).chain().in_set(GameSet::Control),
        )
        .add_systems(Update, animate_explosions);
    }
}

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

fn tick_bombs(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    mut bombs: Query<(Entity, &Transform, &mut Bomb, &mut Sprite)>,
    targets: Query<(Entity, &Position, Has<Player>), With<Health>>,
    mut room: ResMut<CurrentRoom>,
    mut run: ResMut<Run>,
    mut tiles: Query<(&RoomTile, &mut Sprite), Without<Bomb>>,
    mut damage: MessageWriter<Damage>,
    mut fx: MessageWriter<Fx>,
    mut sfx: MessageWriter<Sfx>,
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
        if bomb.fuse > 0.0 {
            continue;
        }

        let center = transform.translation.truncate();
        let radius = BLAST_RADIUS_TILES * TILE_SIZE;

        // 1. Schaden und Rückstoß für alle im Radius.
        for (target, pos, is_player) in &targets {
            let offset = pos.0 - center;
            if offset.length() > radius {
                continue;
            }
            damage.write(Damage {
                target,
                amount: if is_player {
                    PLAYER_DAMAGE
                } else {
                    ENEMY_DAMAGE
                },
                knockback: offset.normalize_or_zero() * KNOCKBACK,
            });
        }

        // 2. Felsen sprengen und das geänderte Layout merken.
        let tile = room.tile_at(center);
        let destroyed = room.layout.blast(tile);
        if !destroyed.is_empty() {
            room.revision += 1;
            run.layout_overrides.insert(room.pos, room.layout.clone());
            for (room_tile, mut tile_sprite) in &mut tiles {
                if room_tile.room == room.pos && destroyed.contains(&room_tile.tile) {
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
        fx.write(Fx::Burst {
            at: center,
            color: Color::srgb(1.0, 0.6, 0.2),
            count: 30,
            speed: 240.0,
        });
        fx.write(Fx::Shake(0.7));
        sfx.write(Sfx(Effect::Explosion));
        fx.write(Fx::Hitstop(0.05));

        // 3. Blitz anzeigen, Bombe entfernen.
        commands.spawn((
            Name::new("ExplosionFlash"),
            ExplosionFlash { age: 0.0 },
            RoomScoped,
            DespawnOnExit(AppState::InGame),
            Mesh2d(assets.explosion_mesh.clone()),
            MeshMaterial2d(assets.explosion_material.clone()),
            Transform::from_translation(center.extend(20.0)).with_scale(Vec3::splat(0.3)),
        ));
        commands.entity(entity).despawn();
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
