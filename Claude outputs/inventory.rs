//! Inventar und Beute: Pickups und Items auf dem Boden, Einsammeln,
//! Belohnungen durch Item-Effekte, Einblendung beim Aufheben.
//!
//! Die Beute eines Raums lebt in `Run::loot` (Daten), die Entities im Raum
//! sind nur ihre Darstellung. Beim Verlassen verschwinden die Entities, die
//! Daten bleiben – beim Zurückkommen liegt alles noch da.

use bevy::prelude::*;
use dungeon_gen::{
    RoomKind,
    items::{self, ItemDef, Pool, Reward},
    loot::{self, Loot, PickupKind},
    room::CENTER,
};

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    combat::{EnemyKilled, Health},
    physics::{Body, Position},
    player::Player,
    room::{CurrentRoom, RoomCleared, RoomScoped, unlock_when_cleared},
    run::{Inventory, Run},
    schedule::GameSet,
    states::AppState,
};

/// Halbe Kantenlänge eines Pickups (Kacheln) – für die Einsammel-Prüfung.
pub const PICKUP_HALF_TILES: f32 = 0.25;
/// Halbe Kantenlänge eines Item-Sockels (Kacheln).
pub const ITEM_HALF_TILES: f32 = 0.3;
const MAX_COINS: u32 = 99;
const MAX_KEYS_BOMBS: u32 = 99;
const TOAST_SECS: f32 = 3.0;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Toast>()
            .add_systems(
                FixedUpdate,
                (
                    // Erst nach dem Öffnen der Türen, damit `RoomCleared` im selben Tick ankommt.
                    reward_room_clear.after(unlock_when_cleared),
                    reward_kills,
                    collect_loot,
                )
                    .in_set(GameSet::Cleanup),
            )
            .add_systems(OnEnter(AppState::InGame), spawn_toast)
            .add_systems(Update, update_toast.run_if(in_state(AppState::InGame)));
    }
}

/// Verbindet eine Entity mit ihrem Eintrag in `Run::loot`.
#[derive(Component, Debug)]
pub struct LootMarker {
    pub tile: dungeon_gen::GridPos,
}

// --- Beute festlegen und spawnen ---------------------------------------------

/// Legt beim ersten Betreten fest, was in Spezialräumen liegt.
/// Normale Räume bekommen ihre Beute erst beim Räumen.
pub fn prepare_room_loot(run: &mut Run, room: &CurrentRoom) {
    if !run.loot_prepared.insert(room.pos) {
        return; // schon vorbereitet
    }
    let Some(kind) = run.floor.get(room.pos).map(|r| r.kind) else {
        return;
    };
    if kind == RoomKind::Treasure {
        // Getrennte Borrows auf verschiedene Felder von `run` – erlaubt,
        // weil `run` hier ein normales `&mut Run` ist (siehe `reward_room_clear`).
        let item = run.pools.draw(Pool::Treasure, &mut run.item_rng);
        add_loot(run, room, Loot::Item(item));
    }
}

/// Legt Beute auf die nächste freie Kachel nahe der Raummitte.
fn add_loot(run: &mut Run, room: &CurrentRoom, loot: Loot) -> Option<dungeon_gen::GridPos> {
    let entries = run.loot.entry(room.pos).or_default();
    let occupied: Vec<_> = entries.iter().map(|(t, _)| *t).collect();
    let tile = room.layout.nearest_floor(CENTER, &occupied)?;
    entries.push((tile, loot));
    Some(tile)
}

pub fn spawn_room_loot(commands: &mut Commands, run: &Run, room: &CurrentRoom, assets: &GameAssets) {
    for (tile, loot) in run.loot.get(&room.pos).into_iter().flatten() {
        spawn_loot_entity(commands, room, *tile, loot, assets);
    }
}

fn spawn_loot_entity(
    commands: &mut Commands,
    room: &CurrentRoom,
    tile: dungeon_gen::GridPos,
    loot: &Loot,
    assets: &GameAssets,
) {
    let (mesh, material) = match loot {
        Loot::Pickup(kind) => (&assets.pickup_mesh, pickup_material(*kind, assets)),
        Loot::Item(_) | Loot::ForSale { .. } => (&assets.item_mesh, &assets.item_material),
    };
    commands.spawn((
        Name::new(format!("Loot {loot:?}")),
        LootMarker { tile },
        RoomScoped,
        DespawnOnExit(AppState::InGame),
        Mesh2d(mesh.clone()),
        MeshMaterial2d(material.clone()),
        // z = 3: über dem Boden, unter Figuren.
        Transform::from_translation(room.tile_center(tile).extend(3.0)),
    ));
}

fn pickup_material(kind: PickupKind, assets: &GameAssets) -> &Handle<ColorMaterial> {
    match kind {
        PickupKind::HalfHeart => &assets.half_heart_material,
        PickupKind::Heart => &assets.heart_material,
        PickupKind::Coin => &assets.coin_material,
        PickupKind::Key => &assets.key_material,
        PickupKind::Bomb => &assets.bomb_material,
    }
}

// --- Einsammeln ------------------------------------------------------------------

fn collect_loot(
    mut commands: Commands,
    mut run: ResMut<Run>,
    room: Res<CurrentRoom>,
    mut toast: ResMut<Toast>,
    player: Single<(&Position, &Body, &mut Health), With<Player>>,
    loot_entities: Query<(Entity, &Transform, &LootMarker)>,
) {
    let (player_pos, player_body, mut health) = player.into_inner();
    // Einmal umleihen: Ab hier ist `run` ein normales `&mut Run`, und wir dürfen
    // gleichzeitig verschiedene Felder ausleihen (`run.loot`, `run.inventory`).
    // Direkt über `ResMut` ginge das nicht, weil jeder Feldzugriff dort über
    // `DerefMut` den *ganzen* `ResMut` ausleiht.
    let run = &mut *run;

    for (entity, transform, marker) in &loot_entities {
        let reach = player_body.half_size + Vec2::splat(PICKUP_HALF_TILES * TILE_SIZE);
        let d = (transform.translation.truncate() - player_pos.0).abs();
        if d.x >= reach.x || d.y >= reach.y {
            continue;
        }
        let Some(entries) = run.loot.get_mut(&room.pos) else {
            continue;
        };
        let Some(index) = entries.iter().position(|(t, _)| *t == marker.tile) else {
            continue;
        };

        let taken = match &entries[index].1 {
            Loot::Pickup(kind) => pick_up(*kind, &mut run.inventory, &mut health),
            Loot::Item(item) => {
                take_item(*item, &mut run.inventory, &mut health, &mut toast);
                true
            }
            // Kaufen kommt in M6a-2.
            Loot::ForSale { .. } => false,
        };
        if taken {
            entries.remove(index);
            commands.entity(entity).despawn();
        }
    }
}

/// Gibt `false` zurück, wenn das Pickup liegen bleibt (z. B. Herz bei vollem Leben).
fn pick_up(kind: PickupKind, inv: &mut Inventory, health: &mut Health) -> bool {
    match kind {
        PickupKind::HalfHeart | PickupKind::Heart if health.current >= health.max => false,
        PickupKind::HalfHeart => {
            apply_reward(Reward::Heal(1), inv, health);
            true
        }
        PickupKind::Heart => {
            apply_reward(Reward::Heal(2), inv, health);
            true
        }
        PickupKind::Coin => {
            apply_reward(Reward::Coins(1), inv, health);
            true
        }
        PickupKind::Key => {
            apply_reward(Reward::Keys(1), inv, health);
            true
        }
        PickupKind::Bomb => {
            apply_reward(Reward::Bombs(1), inv, health);
            true
        }
    }
}

fn take_item(item: &'static ItemDef, inv: &mut Inventory, health: &mut Health, toast: &mut Toast) {
    let bonus = items::max_health_bonus(&[item]) as f32;
    health.max += bonus;
    health.current += bonus;
    inv.items.push(item);
    toast.show(format!("{} – {}", item.name, item.description));
    info!("Item aufgehoben: {} ({})", item.name, item.id);
}

fn apply_reward(reward: Reward, inv: &mut Inventory, health: &mut Health) {
    match reward {
        Reward::Heal(n) => health.current = (health.current + n as f32).min(health.max),
        Reward::Coins(n) => inv.coins = (inv.coins + n).min(MAX_COINS),
        Reward::Keys(n) => inv.keys = (inv.keys + n).min(MAX_KEYS_BOMBS),
        Reward::Bombs(n) => inv.bombs = (inv.bombs + n).min(MAX_KEYS_BOMBS),
    }
}

// --- Belohnungen ---------------------------------------------------------------

fn reward_room_clear(
    mut commands: Commands,
    mut messages: MessageReader<RoomCleared>,
    mut run: ResMut<Run>,
    room: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    mut health: Single<&mut Health, With<Player>>,
) {
    let run = &mut *run;
    for cleared in messages.read() {
        if cleared.room != room.pos {
            continue;
        }
        // Item-Effekte „pro geräumtem Raum“.
        for reward in items::room_clear_rewards(&run.inventory.items) {
            apply_reward(reward, &mut run.inventory, &mut health);
        }
        // Beute: Boss → Item, normaler Raum → Zufallsdrop.
        let loot = match run.floor.get(room.pos).map(|r| r.kind) {
            Some(RoomKind::Boss) => Some(Loot::Item(run.pools.draw(Pool::Boss, &mut run.item_rng))),
            Some(RoomKind::Normal) => {
                let mut rng = loot::room_rng(run.seed, "drops", run.floor.depth, room.pos);
                loot::room_clear_drop(&mut rng).map(Loot::Pickup)
            }
            _ => None,
        };
        if let Some(loot) = loot
            && let Some(tile) = add_loot(run, &room, loot.clone())
        {
            spawn_loot_entity(&mut commands, &room, tile, &loot, &assets);
        }
    }
}

fn reward_kills(
    mut messages: MessageReader<EnemyKilled>,
    mut run: ResMut<Run>,
    mut health: Single<&mut Health, With<Player>>,
) {
    let run = &mut *run;
    for _ in messages.read() {
        for reward in items::kill_rewards(&run.inventory.items, &mut run.effect_rng) {
            apply_reward(reward, &mut run.inventory, &mut health);
        }
    }
}

// --- Einblendung („Toast“) -------------------------------------------------------

#[derive(Resource, Default)]
pub struct Toast {
    text: String,
    remaining: f32,
    dirty: bool,
}

impl Toast {
    pub fn show(&mut self, text: String) {
        self.text = text;
        self.remaining = TOAST_SECS;
        self.dirty = true;
    }
}

#[derive(Component)]
struct ToastText;

fn spawn_toast(mut commands: Commands, mut toast: ResMut<Toast>) {
    *toast = Toast::default();
    commands.spawn((
        Name::new("Toast"),
        DespawnOnExit(AppState::InGame),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(28),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            ToastText,
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(24.0),
                ..default()
            },
            TextColor(Color::srgb(0.95, 0.90, 0.70)),
        )],
    ));
}

/// Echtzeit, damit die Einblendung auch im Pausemenü weiterläuft.
fn update_toast(
    time: Res<Time<Real>>,
    mut toast: ResMut<Toast>,
    mut text: Single<&mut Text, With<ToastText>>,
) {
    if toast.dirty {
        toast.dirty = false;
        text.0 = toast.text.clone();
    }
    if toast.remaining > 0.0 {
        toast.remaining -= time.delta_secs();
        if toast.remaining <= 0.0 {
            text.0.clear();
        }
    }
}
