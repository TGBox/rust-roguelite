//! Opferaltar: Berühren kostet ein Herz und bringt eine Belohnung.
//!
//! Jedes weitere Opfer bringt mehr (siehe `dungeon_gen::loot::altar_reward`),
//! nach drei Opfern zerfällt der Altar. Man muss den Altar zwischendurch
//! verlassen – sonst würde man ihn im Sekundentakt „leersaugen“.

use bevy::prelude::*;
use dungeon_gen::loot::{self, AltarReward, Loot};

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    audio::{Effect, Sfx},
    combat::{Damage, HitStatus, Invulnerable},
    inventory::{
        AltarLabel, LootMarker, PICKUP_HALF_TILES, Toast, add_loot_near, altar_text,
        spawn_loot_entity,
    },
    item_db::ItemDatabase,
    juice::Fx,
    physics::{Body, Position},
    player::Player,
    room::CurrentRoom,
    run::Run,
    schedule::GameSet,
};

pub struct AltarPlugin;

impl Plugin for AltarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, use_altars.in_set(GameSet::Cleanup));
    }
}

fn use_altars(
    mut commands: Commands,
    db: Res<ItemDatabase>,
    mut run: ResMut<Run>,
    room: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    player: Single<(Entity, &Position, &Body, &Invulnerable), With<Player>>,
    altars: Query<(Entity, &Transform, &LootMarker)>,
    mut labels: Query<&mut Text2d, With<AltarLabel>>,
    mut damage: MessageWriter<Damage>,
    mut fx: MessageWriter<Fx>,
    mut sfx: MessageWriter<Sfx>,
    mut toast: ResMut<Toast>,
    // Berührt der Spieler gerade einen Altar? Ausgelöst wird nur beim Betreten.
    mut was_touching: Local<bool>,
) {
    let (player_entity, player_pos, player_body, invulnerable) = player.into_inner();
    let run = &mut *run;
    let mut touching = false;

    for (entity, transform, marker) in &altars {
        let Some(entries) = run.loot.get_mut(&room.pos) else {
            continue;
        };
        let Some(index) = entries
            .iter()
            .position(|(t, l)| *t == marker.tile && matches!(l, Loot::Altar { .. }))
        else {
            continue;
        };
        let reach = player_body.half_size + Vec2::splat(PICKUP_HALF_TILES * TILE_SIZE);
        let d = (transform.translation.truncate() - player_pos.0).abs();
        if d.x >= reach.x || d.y >= reach.y {
            continue;
        }
        touching = true;
        // Nur beim Betreten und nicht, während man ohnehin unverwundbar ist
        // (ein Opfer muss wirklich etwas kosten).
        if *was_touching || invulnerable.0 > 0.0 {
            continue;
        }

        let Loot::Altar { uses } = entries[index].1 else {
            continue;
        };
        // Das Opfer: echter Schaden über die normale Kampflogik – man kann
        // daran also auch sterben.
        damage.write(Damage {
            target: player_entity,
            amount: loot::ALTAR_COST,
            knockback: Vec2::ZERO,
            status: HitStatus::NONE,
        });
        fx.write(Fx::Burst {
            at: transform.translation.truncate(),
            color: Color::srgb(0.85, 0.1, 0.15),
            count: 20,
            speed: 140.0,
        });
        sfx.write(Sfx(Effect::Sacrifice));

        // Belohnung.
        let reward = loot::altar_reward(uses, &mut run.item_rng);
        let drop = match reward {
            AltarReward::Coins(n) => {
                run.inventory.coins = (run.inventory.coins + n).min(99);
                toast.show(format!("Der Altar gibt dir {n} Münzen"));
                None
            }
            AltarReward::Pickup(kind) => Some(Loot::Pickup(kind)),
            AltarReward::Item(pool) => {
                toast.show("Der Altar gewährt ein Geschenk …".to_string());
                Some(Loot::Item(run.pools.draw(&db.0, pool, &mut run.item_rng)))
            }
        };

        // Zählen oder zerfallen lassen.
        let uses = uses + 1;
        if uses >= loot::ALTAR_USES {
            entries.remove(index);
            commands.entity(entity).despawn();
            toast.show("Der Altar zerfällt zu Staub".to_string());
        } else {
            entries[index].1 = Loot::Altar { uses };
            // Es gibt höchstens einen Altar pro Raum – also alle Beschriftungen setzen.
            for mut text in &mut labels {
                text.0 = altar_text(uses);
            }
        }

        if let Some(loot) = drop {
            // Unterhalb des Altars ablegen, damit man nicht gleich wieder opfert.
            let below = dungeon_gen::GridPos::new(marker.tile.x, marker.tile.y - 2);
            if let Some(tile) = add_loot_near(run, &room, below, loot.clone()) {
                spawn_loot_entity(&mut commands, &room, tile, &loot, &assets, &db.0);
            }
        }
    }
    *was_touching = touching;
}
