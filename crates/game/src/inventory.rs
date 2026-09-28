//! Inventar und Beute: Pickups und Items auf dem Boden, Einsammeln,
//! Belohnungen durch Item-Effekte, Einblendung beim Aufheben.
//!
//! Die Beute eines Raums lebt in `Run::loot` (Daten), die Entities im Raum
//! sind nur ihre Darstellung. Beim Verlassen verschwinden die Entities, die
//! Daten bleiben – beim Zurückkommen liegt alles noch da.

use bevy::prelude::*;
use dungeon_gen::{
    GridPos, RoomKind,
    items::{self, ItemDb, ItemDef, ItemId, Pool, Reward},
    loot::{self, Loot, PickupKind, Ware},
    room::CENTER,
};

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    audio::{Effect, Sfx},
    combat::{EnemyKilled, Health},
    item_db::ItemDatabase,
    physics::{Body, Position},
    pixel_art,
    player::Player,
    room::{CurrentRoom, RoomCleared, RoomScoped, unlock_when_cleared},
    run::{ActiveSlot, Inventory, Run},
    schedule::GameSet,
    states::AppState,
};

/// Halbe Kantenlänge eines Pickups (Kacheln) – für die Einsammel-Prüfung.
pub const PICKUP_HALF_TILES: f32 = 0.25;
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
pub fn prepare_room_loot(run: &mut Run, room: &CurrentRoom, db: &ItemDb) {
    if !run.loot_prepared.insert(room.pos) {
        return; // schon vorbereitet
    }
    let Some(kind) = run.floor.get(room.pos).map(|r| r.kind) else {
        return;
    };
    if kind == RoomKind::Treasure {
        // Getrennte Borrows auf verschiedene Felder von `run` – erlaubt,
        // weil `run` hier ein normales `&mut Run` ist (siehe `reward_room_clear`).
        let item = run.pools.draw(db, Pool::Treasure, &mut run.item_rng);
        add_loot(run, room, Loot::Item(item));
    }
    if kind == RoomKind::Secret {
        let mut rng = loot::room_rng(run.seed, "secret", run.floor.depth, room.pos);
        let stash = loot::secret_stash(&mut rng);
        if stash.item {
            let item = run.pools.draw(db, Pool::Treasure, &mut run.item_rng);
            add_loot(run, room, Loot::Item(item));
        }
        for kind in stash.pickups {
            add_loot(run, room, Loot::Pickup(kind));
        }
    }
    if kind == RoomKind::Sacrifice {
        run.loot
            .entry(room.pos)
            .or_default()
            .push((CENTER, Loot::Altar { uses: 0 }));
    }
    if kind == RoomKind::Shop {
        // Das Angebot kommt aus einem eigenen Stream pro Etage.
        let mut rng = loot::room_rng(run.seed, "shop", run.floor.depth, room.pos);
        let stock = loot::shop_stock(db, &mut run.pools, &mut rng);
        let entries = run.loot.entry(room.pos).or_default();
        // Waren in einer Reihe quer durch die Raummitte.
        for (i, ware) in stock.into_iter().enumerate() {
            let tile = dungeon_gen::GridPos::new(4 + 2 * i as i32, CENTER.y);
            let price = loot::price(db, &ware);
            entries.push((tile, Loot::ForSale { ware, price }));
        }
    }
}

/// Legt Beute auf die nächste freie Kachel nahe der Raummitte.
fn add_loot(run: &mut Run, room: &CurrentRoom, loot: Loot) -> Option<GridPos> {
    add_loot_near(run, room, CENTER, loot)
}

/// Legt Beute auf die nächste freie Kachel nahe `target`.
pub fn add_loot_near(
    run: &mut Run,
    room: &CurrentRoom,
    target: GridPos,
    loot: Loot,
) -> Option<GridPos> {
    let entries = run.loot.entry(room.pos).or_default();
    let occupied: Vec<_> = entries.iter().map(|(t, _)| *t).collect();
    let tile = room.layout.nearest_floor(target, &occupied)?;
    entries.push((tile, loot));
    Some(tile)
}

pub fn spawn_room_loot(
    commands: &mut Commands,
    run: &Run,
    room: &CurrentRoom,
    assets: &GameAssets,
    db: &ItemDb,
) {
    for (tile, loot) in run.loot.get(&room.pos).into_iter().flatten() {
        spawn_loot_entity(commands, room, *tile, loot, assets, db);
    }
}

pub fn spawn_loot_entity(
    commands: &mut Commands,
    room: &CurrentRoom,
    tile: GridPos,
    loot: &Loot,
    assets: &GameAssets,
    db: &ItemDb,
) {
    let (image, rows) = loot_sprite(loot, assets, db);
    let mut entity = commands.spawn((
        Name::new(format!("Loot {}", loot_label(loot))),
        LootMarker { tile },
        RoomScoped,
        DespawnOnExit(AppState::InGame),
        Sprite {
            image: image.clone(),
            custom_size: Some(pixel_art::display_size(rows)),
            ..default()
        },
        // z = 3: über dem Boden, unter Figuren.
        Transform::from_translation(room.tile_center(tile).extend(3.0)),
    ));
    if let Loot::ForSale { price, .. } = loot {
        // Preisschild als Kind-Entity: bewegt und verschwindet mit der Ware.
        entity.with_child((
            Text2d::new(format!("{price}")),
            TextFont {
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(Color::srgb(0.98, 0.85, 0.30)),
            Transform::from_xyz(0.0, -22.0, 1.0),
        ));
    }
    if let Loot::Altar { uses } = loot {
        // Verbleibende Opfer anzeigen; das Herz sagt, was es kostet.
        entity.with_child((
            AltarLabel,
            Text2d::new(altar_text(*uses)),
            TextFont {
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(Color::srgb(0.95, 0.35, 0.40)),
            Transform::from_xyz(0.0, -24.0, 1.0),
        ));
    }
}

/// Beschriftung unter dem Altar.
#[derive(Component)]
pub struct AltarLabel;

pub fn altar_text(uses: u32) -> String {
    format!("♥ opfern ({} übrig)", loot::ALTAR_USES.saturating_sub(uses))
}

/// Bild und Pixelkarte (für die Größe) zu einer Beute.
fn loot_sprite<'a>(
    loot: &Loot,
    assets: &'a GameAssets,
    db: &ItemDb,
) -> (&'a Handle<Image>, &'static [&'static str]) {
    let s = &assets.sprites;
    // Aktive Items haben einen orangen statt eines blauen Edelsteins.
    let is_active = |id: &ItemId| db.get(id).is_some_and(|i| i.active().is_some());
    match loot {
        Loot::Item(id)
        | Loot::ForSale {
            ware: Ware::Item(id),
            ..
        } if is_active(id) => (&s.active_item, pixel_art::ITEM),
        Loot::Altar { .. } => (&s.altar, pixel_art::ALTAR),
        Loot::Pickup(PickupKind::HalfHeart) => (&s.heart_half, pixel_art::HEART),
        Loot::Pickup(PickupKind::Heart) => (&s.heart_full, pixel_art::HEART),
        Loot::Pickup(PickupKind::Coin) => (&s.coin, pixel_art::COIN),
        Loot::Pickup(PickupKind::Key) => (&s.key, pixel_art::KEY),
        Loot::Pickup(PickupKind::Bomb) => (&s.bomb, pixel_art::BOMB),
        Loot::Item(_)
        | Loot::ForSale {
            ware: Ware::Item(_),
            ..
        } => (&s.item, pixel_art::ITEM),
        Loot::ForSale {
            ware: Ware::Pickup(kind),
            ..
        } => loot_sprite(&Loot::Pickup(*kind), assets, db),
    }
}

/// Kurzer Name für den Entity-Namen (statt des kompletten `Debug`-Ausdrucks).
fn loot_label(loot: &Loot) -> String {
    match loot {
        Loot::Pickup(kind) => format!("{kind:?}"),
        Loot::Item(id) => id.to_string(),
        Loot::ForSale { ware, price } => format!("ForSale {ware:?} {price}"),
        Loot::Altar { uses } => format!("Altar {uses}"),
    }
}

// --- Einsammeln ------------------------------------------------------------------

fn collect_loot(
    mut commands: Commands,
    db: Res<ItemDatabase>,
    mut run: ResMut<Run>,
    room: Res<CurrentRoom>,
    mut toast: ResMut<Toast>,
    player: Single<(&Position, &Body, &mut Health), With<Player>>,
    loot_entities: Query<(Entity, &Transform, &LootMarker)>,
    // Kachel, für die zuletzt „zu teuer“ angezeigt wurde – damit der Hinweis
    // nicht jeden Tick neu erscheint, solange man davorsteht.
    mut denied: Local<Option<dungeon_gen::GridPos>>,
    // Kachel, auf die gerade ein getauschtes aktives Item gelegt wurde: erst
    // wieder aufheben, wenn der Spieler sie einmal verlassen hat.
    mut swapped: Local<Option<GridPos>>,
    mut sfx: MessageWriter<Sfx>,
    assets: Res<GameAssets>,
) {
    let (player_pos, player_body, mut health) = player.into_inner();
    // Einmal umleihen: Ab hier ist `run` ein normales `&mut Run`, und wir dürfen
    // gleichzeitig verschiedene Felder ausleihen (`run.loot`, `run.inventory`).
    // Direkt über `ResMut` ginge das nicht, weil jeder Feldzugriff dort über
    // `DerefMut` den *ganzen* `ResMut` ausleiht.
    let run = &mut *run;
    let mut touching_denied = None;
    let mut touching_swapped = false;

    for (entity, transform, marker) in &loot_entities {
        let reach = player_body.half_size + Vec2::splat(PICKUP_HALF_TILES * TILE_SIZE);
        let d = (transform.translation.truncate() - player_pos.0).abs();
        if d.x >= reach.x || d.y >= reach.y {
            continue;
        }
        if *swapped == Some(marker.tile) {
            touching_swapped = true;
            continue;
        }
        let Some(entries) = run.loot.get_mut(&room.pos) else {
            continue;
        };
        let Some(index) = entries.iter().position(|(t, _)| *t == marker.tile) else {
            continue;
        };

        // Klang vorher bestimmen: Nach dem Aufheben ist der Eintrag weg.
        let sound = pickup_sound(&entries[index].1);
        // Ein ersetztes aktives Item landet an derselben Stelle.
        let mut dropped: Option<ItemId> = None;
        let taken = match &entries[index].1 {
            Loot::Pickup(kind) => pick_up(*kind, &mut run.inventory, &mut health),
            Loot::Item(id) => match db.0.get(id) {
                Some(item) => {
                    dropped = take_item(item, &mut run.inventory, &mut health, &mut toast, &db.0);
                    true
                }
                // Item wurde per Hot-Reload aus der Datei entfernt: liegen lassen.
                None => false,
            },
            // Altäre benutzt man, man hebt sie nicht auf (siehe `altar.rs`).
            Loot::Altar { .. } => false,
            Loot::ForSale { ware, price } => {
                if run.inventory.coins < *price {
                    touching_denied = Some(marker.tile);
                    if *denied != Some(marker.tile) {
                        toast.show(format!("Zu teuer – kostet {price} Münzen"));
                        sfx.write(Sfx(Effect::Deny));
                    }
                    false
                } else {
                    let bought = match ware {
                        Ware::Pickup(kind) => pick_up(*kind, &mut run.inventory, &mut health),
                        Ware::Item(id) => match db.0.get(id) {
                            Some(item) => {
                                dropped = take_item(
                                    item,
                                    &mut run.inventory,
                                    &mut health,
                                    &mut toast,
                                    &db.0,
                                );
                                true
                            }
                            None => false,
                        },
                    };
                    if bought {
                        run.inventory.coins -= *price;
                    }
                    bought
                }
            }
        };
        if taken {
            sfx.write(Sfx(sound));
            entries.remove(index);
            commands.entity(entity).despawn();
            if let Some(old) = dropped {
                let loot = Loot::Item(old);
                spawn_loot_entity(&mut commands, &room, marker.tile, &loot, &assets, &db.0);
                entries.push((marker.tile, loot));
                *swapped = Some(marker.tile);
                touching_swapped = true;
            }
        }
    }
    *denied = touching_denied;
    if !touching_swapped {
        *swapped = None;
    }
}

fn pickup_sound(loot: &Loot) -> Effect {
    match loot {
        Loot::Pickup(PickupKind::Coin) => Effect::Coin,
        Loot::Pickup(_) => Effect::Pickup,
        Loot::Item(_) => Effect::Item,
        Loot::ForSale {
            ware: Ware::Item(_),
            ..
        } => Effect::Item,
        // Kaufen eines Pickups klingt nach Geld.
        Loot::ForSale { .. } => Effect::Coin,
        Loot::Altar { .. } => Effect::Sacrifice,
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

/// Nimmt ein Item. Aktive Items kommen in den Q-Platz; ein vorher dort
/// liegendes wird zurückgegeben, damit es auf den Boden gelegt werden kann.
fn take_item(
    item: &ItemDef,
    inv: &mut Inventory,
    health: &mut Health,
    toast: &mut Toast,
    db: &ItemDb,
) -> Option<ItemId> {
    let synergies_before = active_synergy_names(inv, db);

    let mut dropped = None;
    if let Some((_, charges)) = item.active() {
        // Gespeicherte Ladung übernehmen – neue aktive Items starten voll.
        let charge = inv.stored_charges.remove(&item.id).unwrap_or(charges);
        let new = ActiveSlot {
            id: item.id.clone(),
            charge,
        };
        // `Option::replace` setzt den neuen Wert und gibt den alten zurück.
        if let Some(old) = inv.active.replace(new) {
            inv.stored_charges.insert(old.id.clone(), old.charge);
            dropped = Some(old.id);
        }
    } else {
        // Herzcontainer (auch negative wie bei der Glaskanone). Mindestens ein Herz bleibt.
        let bonus = item.max_health_bonus() as f32;
        health.max = (health.max + bonus).max(2.0);
        health.current = (health.current + bonus.max(0.0)).min(health.max).max(1.0);
        inv.items.push(item.id.clone());
    }

    // Neue Synergien groß ankündigen – dafür spielt man schließlich.
    let new_synergies: Vec<String> = active_synergy_names(inv, db)
        .into_iter()
        .filter(|s| !synergies_before.contains(s))
        .collect();
    let mut text = format!("{} – {}", item.name, item.description);
    for name in &new_synergies {
        text.push_str(&format!("   ✦ Synergie: {name}!"));
        info!("Synergie aktiv: {name}");
    }
    toast.show(text);
    info!("Item aufgehoben: {} ({})", item.name, item.id);
    dropped
}

fn active_synergy_names(inv: &Inventory, db: &ItemDb) -> Vec<String> {
    db.loadout(&inv.items, inv.active.as_ref().map(|a| &a.id))
        .synergies
        .iter()
        .map(|s| s.name.clone())
        .collect()
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

pub fn reward_room_clear(
    mut commands: Commands,
    db: Res<ItemDatabase>,
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
        let inv = &run.inventory;
        let rewards = items::room_clear_rewards(
            &db.0.loadout(&inv.items, inv.active.as_ref().map(|a| &a.id)),
        );
        for reward in rewards {
            apply_reward(reward, &mut run.inventory, &mut health);
        }
        // Beute: Boss → Item, normaler Raum → Zufallsdrop.
        let loot = match run.floor.get(room.pos).map(|r| r.kind) {
            Some(RoomKind::Boss) => Some(Loot::Item(run.pools.draw(
                &db.0,
                Pool::Boss,
                &mut run.item_rng,
            ))),
            Some(RoomKind::Normal) => {
                let mut rng = loot::room_rng(run.seed, "drops", run.floor.depth, room.pos);
                loot::room_clear_drop(&mut rng).map(Loot::Pickup)
            }
            // Herausforderung geschafft: Item plus ein Herz als Trost.
            Some(RoomKind::Challenge) => {
                if let Some(tile) = add_loot(run, &room, Loot::Pickup(PickupKind::Heart)) {
                    let heart = Loot::Pickup(PickupKind::Heart);
                    spawn_loot_entity(&mut commands, &room, tile, &heart, &assets, &db.0);
                }
                Some(Loot::Item(run.pools.draw(
                    &db.0,
                    Pool::Treasure,
                    &mut run.item_rng,
                )))
            }
            _ => None,
        };
        if let Some(loot) = loot
            && let Some(tile) = add_loot(run, &room, loot.clone())
        {
            spawn_loot_entity(&mut commands, &room, tile, &loot, &assets, &db.0);
        }
    }
}

fn reward_kills(
    mut commands: Commands,
    db: Res<ItemDatabase>,
    mut messages: MessageReader<EnemyKilled>,
    mut run: ResMut<Run>,
    room: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    mut health: Single<&mut Health, With<Player>>,
) {
    let run = &mut *run;
    for kill in messages.read() {
        let inv = &run.inventory;
        let loadout = db.0.loadout(&inv.items, inv.active.as_ref().map(|a| &a.id));
        for reward in items::kill_rewards(&loadout, &mut run.effect_rng) {
            apply_reward(reward, &mut run.inventory, &mut health);
        }
        // Champions lassen immer etwas fallen – dort, wo sie starben.
        if kill.champion {
            let kind = if run.effect_rng.chance(0.5) {
                PickupKind::Coin
            } else {
                *run.effect_rng
                    .choose(&[PickupKind::HalfHeart, PickupKind::Key, PickupKind::Bomb])
                    .unwrap_or(&PickupKind::Coin)
            };
            let loot = Loot::Pickup(kind);
            if let Some(tile) = add_loot_near(run, &room, room.tile_at(kill.pos), loot.clone()) {
                spawn_loot_entity(&mut commands, &room, tile, &loot, &assets, &db.0);
            }
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
