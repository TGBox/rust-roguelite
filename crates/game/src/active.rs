//! Aktive Items: Taste Q benutzt das Item, geräumte Räume laden es auf.
//!
//! Die *Daten* (was ein Item tut, wie viele Ladungen) stehen in
//! `dungeon_gen::items::ActiveKind`. Hier wird nur ausgeführt.

use bevy::prelude::*;
use dungeon_gen::items::ActiveKind;

use crate::{
    audio::{Effect, Sfx},
    combat::{Damage, Faction, Frozen, Health, HitStatus, Invulnerable},
    enemy::{Asleep, Boss, Enemy},
    inventory::Toast,
    item_db::ItemDatabase,
    juice::Fx,
    physics::Position,
    player::{Player, PlayerInput},
    projectile::Projectile,
    room::{RoomCleared, unlock_when_cleared},
    run::Run,
    schedule::GameSet,
};

/// Rückstoß der Schockwelle (Pixel/s).
const SHOCKWAVE_KNOCKBACK: f32 = 420.0;

pub struct ActivePlugin;

impl Plugin for ActivePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, use_active_item.in_set(GameSet::Control))
            .add_systems(
                FixedUpdate,
                charge_active_item
                    .in_set(GameSet::Cleanup)
                    .after(unlock_when_cleared),
            );
    }
}

/// Jeder geräumte Raum bringt eine Ladung.
fn charge_active_item(
    mut cleared: MessageReader<RoomCleared>,
    mut run: ResMut<Run>,
    db: Res<ItemDatabase>,
    mut toast: ResMut<Toast>,
    mut sfx: MessageWriter<Sfx>,
) {
    for _ in cleared.read() {
        let Some(slot) = run.inventory.active.as_mut() else {
            continue;
        };
        let Some((item, max)) = db.0.get(&slot.id).and_then(|i| Some((i, i.active()?.1))) else {
            continue;
        };
        if slot.charge < max {
            slot.charge += 1;
            if slot.charge == max {
                toast.show(format!("{} ist aufgeladen – Q drücken", item.name));
                sfx.write(Sfx(Effect::Charged));
            }
        }
    }
}

fn use_active_item(
    mut commands: Commands,
    mut input: ResMut<PlayerInput>,
    mut run: ResMut<Run>,
    db: Res<ItemDatabase>,
    mut toast: ResMut<Toast>,
    mut player: Single<(&Position, &mut Health, &mut Invulnerable), With<Player>>,
    enemies: Query<(Entity, &Position, Has<Boss>), With<Enemy>>,
    projectiles: Query<(Entity, &Position, &Projectile)>,
    mut damage: MessageWriter<Damage>,
    mut fx: MessageWriter<Fx>,
    mut sfx: MessageWriter<Sfx>,
) {
    // Eingerastete Eingabe verbrauchen – auch wenn nichts passiert.
    if !std::mem::take(&mut input.use_active) {
        return;
    }
    let Some(slot) = run.inventory.active.as_mut() else {
        return;
    };
    let Some(item) = db.0.get(&slot.id) else {
        return;
    };
    let Some((kind, max)) = item.active() else {
        return;
    };
    if slot.charge < max {
        toast.show(format!("{} lädt noch ({}/{max})", item.name, slot.charge));
        sfx.write(Sfx(Effect::Deny));
        return;
    }
    slot.charge = 0;
    sfx.write(Sfx(Effect::PowerUp));

    let (player_pos, health, invulnerable) = &mut *player;
    match kind {
        ActiveKind::Heal(n) => {
            health.current = (health.current + n as f32).min(health.max);
            fx.write(Fx::Burst {
                at: player_pos.0,
                color: Color::srgb(1.0, 0.4, 0.5),
                count: 16,
                speed: 100.0,
            });
        }
        ActiveKind::Shockwave { damage: amount } => {
            for (enemy, pos, _) in &enemies {
                damage.write(Damage {
                    target: enemy,
                    amount,
                    knockback: (pos.0 - player_pos.0).normalize_or_zero() * SHOCKWAVE_KNOCKBACK,
                    status: HitStatus::NONE,
                });
            }
            fx.write(Fx::Burst {
                at: player_pos.0,
                color: Color::srgb(1.0, 0.9, 0.5),
                count: 40,
                speed: 320.0,
            });
            fx.write(Fx::Shake(0.8));
            sfx.write(Sfx(Effect::Explosion));
        }
        ActiveKind::FreezeAll { secs } => {
            for (enemy, pos, is_boss) in &enemies {
                // Bosse bleiben nur kurz stehen, frieren aber nicht ein.
                if is_boss {
                    commands.entity(enemy).try_insert(Asleep(1.0));
                } else {
                    commands
                        .entity(enemy)
                        .try_insert((Frozen(secs), Asleep(secs)));
                }
                fx.write(Fx::Burst {
                    at: pos.0,
                    color: Color::srgb(0.7, 0.95, 1.0),
                    count: 8,
                    speed: 80.0,
                });
            }
            // Feindliche Schüsse zerspringen zu Eis.
            for (shot, pos, projectile) in &projectiles {
                if projectile.faction == Faction::Enemy {
                    commands.entity(shot).try_despawn();
                    fx.write(Fx::Burst {
                        at: pos.0,
                        color: Color::srgb(0.7, 0.95, 1.0),
                        count: 3,
                        speed: 50.0,
                    });
                }
            }
        }
        ActiveKind::Shield { secs } => {
            invulnerable.0 = invulnerable.0.max(secs);
        }
        ActiveKind::Coins(n) => {
            run.inventory.coins = (run.inventory.coins + n).min(99);
            fx.write(Fx::Burst {
                at: player_pos.0,
                color: Color::srgb(1.0, 0.85, 0.3),
                count: 14,
                speed: 110.0,
            });
        }
    }
    info!("Aktives Item benutzt: {}", item.name);
}
