//! HUD oben links: Herzen, Münzen/Schlüssel/Bomben mit Symbolen, Items und Werte.
//!
//! Ein Herz = zwei Lebenspunkte (wie bei Isaac): voll, halb oder leer.
//! Die Symbole sind die Pixel-Grafiken aus `pixel_art.rs`.
//!
//! Dazu: aktives Item mit Ladebalken, aktive Synergien und oben in der Mitte
//! der Lebensbalken des Bosses.

use bevy::prelude::*;

use dungeon_gen::EnemyKind;

use crate::{
    assets::GameAssets,
    combat::Health,
    enemy::{Boss, EnemyType},
    item_db::ItemDatabase,
    pixel_art,
    player::{Player, PlayerStats},
    run::Run,
    states::AppState,
};

const TEXT: Color = Color::srgb(0.92, 0.89, 0.82);
const TEXT_DIM: Color = Color::srgb(0.62, 0.60, 0.56);

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_hud)
            .add_systems(
                Update,
                (
                    update_hearts,
                    // Auch nach einem Hot-Reload: Item-Namen können sich geändert haben.
                    update_counters.run_if(
                        resource_exists_and_changed::<Run>
                            .or_else(resource_changed::<ItemDatabase>),
                    ),
                    update_stats,
                    update_boss_bar,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Component)]
struct HeartBar;

#[derive(Component)]
struct HeartSegment;

#[derive(Component, Clone, Copy)]
enum Counter {
    Coins,
    Keys,
    Bombs,
    Depth,
}

#[derive(Component)]
struct ItemsText;

#[derive(Component)]
struct StatsText;

#[derive(Component)]
struct SynergyText;

/// Zeile mit dem aktiven Item (nur sichtbar, wenn man eines hat).
#[derive(Component)]
struct ActiveRow;

#[derive(Component)]
struct ActiveText;

#[derive(Component)]
struct BossBar;

#[derive(Component)]
struct BossBarFill;

#[derive(Component)]
struct BossName;

fn hud_text(size: f32, color: Color) -> impl Bundle {
    (
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

/// Pixel-Symbol als UI-Bild in Originalproportionen.
fn icon(image: &Handle<Image>, rows: &[&str]) -> impl Bundle {
    let size = pixel_art::display_size(rows);
    (
        ImageNode::new(image.clone()),
        Node {
            width: px(size.x),
            height: px(size.y),
            margin: UiRect::left(px(10)),
            ..default()
        },
    )
}

fn spawn_hud(mut commands: Commands, assets: Res<GameAssets>) {
    let s = &assets.sprites;
    commands.spawn((
        Name::new("Hud"),
        DespawnOnExit(AppState::InGame),
        Node {
            position_type: PositionType::Absolute,
            top: px(14),
            left: px(14),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        },
        children![
            (
                HeartBar,
                Node {
                    column_gap: px(3),
                    ..default()
                }
            ),
            (
                Node {
                    align_items: AlignItems::Center,
                    column_gap: px(4),
                    ..default()
                },
                children![
                    icon(&s.coin, pixel_art::COIN),
                    (Counter::Coins, hud_text(18.0, TEXT)),
                    icon(&s.key, pixel_art::KEY),
                    (Counter::Keys, hud_text(18.0, TEXT)),
                    icon(&s.bomb, pixel_art::BOMB),
                    (Counter::Bombs, hud_text(18.0, TEXT)),
                    (
                        Counter::Depth,
                        hud_text(18.0, TEXT_DIM),
                        Node {
                            margin: UiRect::left(px(18)),
                            ..default()
                        }
                    ),
                ]
            ),
            (
                ActiveRow,
                Visibility::Hidden,
                Node {
                    align_items: AlignItems::Center,
                    column_gap: px(6),
                    ..default()
                },
                children![
                    icon(&s.active_item, pixel_art::ITEM),
                    (ActiveText, hud_text(16.0, TEXT)),
                ]
            ),
            (ItemsText, hud_text(14.0, TEXT_DIM)),
            (SynergyText, hud_text(14.0, Color::srgb(0.95, 0.80, 0.40))),
            (StatsText, hud_text(14.0, TEXT_DIM)),
        ],
    ));

    // Boss-Lebensbalken: oben in der Mitte, versteckt bis ein Boss auftaucht.
    commands.spawn((
        Name::new("BossBar"),
        BossBar,
        DespawnOnExit(AppState::InGame),
        Visibility::Hidden,
        Node {
            position_type: PositionType::Absolute,
            top: px(14),
            width: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(4),
            ..default()
        },
        children![
            (BossName, hud_text(16.0, TEXT)),
            (
                Node {
                    width: px(360),
                    height: px(12),
                    padding: UiRect::all(px(2)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                children![(
                    BossBarFill,
                    Node {
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.80, 0.15, 0.20)),
                )],
            ),
        ],
    ));
}

fn boss_name(kind: EnemyKind) -> &'static str {
    match kind {
        EnemyKind::BroodMother => "Brutmutter",
        EnemyKind::Warden => "Der Wächter",
        _ => "Klumpenkönig",
    }
}

/// Summe aller Boss-Leben (die Brutmutter hat keine Boss-Kinder, aber so
/// funktioniert es auch für mehrere Bosse).
fn update_boss_bar(
    bosses: Query<(&Health, &EnemyType), With<Boss>>,
    mut bar: Single<&mut Visibility, With<BossBar>>,
    mut fill: Single<&mut Node, With<BossBarFill>>,
    mut name: Single<&mut Text, With<BossName>>,
) {
    let (current, max) = bosses
        .iter()
        .fold((0.0, 0.0), |(c, m), (h, _)| (c + h.current, m + h.max));
    if max <= 0.0 {
        bar.set_if_neq(Visibility::Hidden);
        return;
    }
    bar.set_if_neq(Visibility::Inherited);
    // Nur bei echter Änderung schreiben: Jede Änderung an `Node` löst eine
    // neue Layout-Berechnung aus.
    let width = percent(100.0 * current / max);
    if fill.width != width {
        fill.width = width;
    }
    if let Some((_, kind)) = bosses.iter().next() {
        let wanted = boss_name(kind.0);
        if name.0 != wanted {
            name.0 = wanted.to_string();
        }
    }
}

/// Läuft nur, wenn sich das Leben des Spielers geändert hat
/// (beim ersten Mal zählt das Hinzufügen der Komponente als Änderung).
fn update_hearts(
    mut commands: Commands,
    assets: Res<GameAssets>,
    player: Query<&Health, (With<Player>, Changed<Health>)>,
    bar: Single<Entity, With<HeartBar>>,
    old: Query<Entity, With<HeartSegment>>,
) {
    let Ok(health) = player.single() else {
        return;
    };
    for e in &old {
        commands.entity(e).despawn();
    }
    let s = &assets.sprites;
    let size = pixel_art::display_size(pixel_art::HEART);
    let hearts = (health.max / 2.0).ceil() as i32;
    for i in 0..hearts {
        // Wie viele Lebenspunkte entfallen auf dieses Herz? (2 = voll)
        let fill = health.current - (i * 2) as f32;
        let image = if fill >= 2.0 {
            &s.heart_full
        } else if fill >= 1.0 {
            &s.heart_half
        } else {
            &s.heart_empty
        };
        commands.spawn((
            HeartSegment,
            ChildOf(*bar),
            ImageNode::new(image.clone()),
            Node {
                width: px(size.x),
                height: px(size.y),
                ..default()
            },
        ));
    }
}

fn update_counters(
    run: Res<Run>,
    db: Res<ItemDatabase>,
    mut counters: Query<(&Counter, &mut Text)>,
    mut items: Single<&mut Text, (With<ItemsText>, Without<Counter>)>,
    mut synergy_text: Single<&mut Text, (With<SynergyText>, Without<ItemsText>, Without<Counter>)>,
    mut active_text: Single<
        &mut Text,
        (
            With<ActiveText>,
            Without<SynergyText>,
            Without<ItemsText>,
            Without<Counter>,
        ),
    >,
    mut active_row: Single<&mut Visibility, With<ActiveRow>>,
) {
    let inv = &run.inventory;
    for (counter, mut text) in &mut counters {
        text.0 = match counter {
            Counter::Coins => format!("{:02}", inv.coins),
            Counter::Keys => format!("{:02}", inv.keys),
            Counter::Bombs => format!("{:02}", inv.bombs),
            Counter::Depth => format!("Etage {}/{}", run.floor.depth, dungeon_gen::meta::MAX_DEPTH),
        };
    }
    // Eigene Variable: `names` leiht sich Strings aus `owned`, das deshalb
    // mindestens so lange leben muss (ein Temporärwert wäre sofort weg).
    let owned = db.0.resolve(&inv.items);
    let names: Vec<&str> = owned.iter().map(|i| i.name.as_str()).collect();
    items.0 = if names.is_empty() {
        String::new()
    } else {
        format!("Items: {}", names.join(", "))
    };

    let active_id = inv.active.as_ref().map(|a| &a.id);
    let synergies = db.0.synergies_for(
        &inv.items
            .iter()
            .chain(active_id)
            .cloned()
            .collect::<Vec<_>>(),
    );
    synergy_text.0 = if synergies.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = synergies.iter().map(|s| s.name.as_str()).collect();
        format!("✦ Synergien: {}", names.join(", "))
    };

    // Aktives Item: Name plus Ladebalken aus Blöcken, z. B. „■■□ (Q)“.
    let active = inv
        .active
        .as_ref()
        .and_then(|slot| db.0.get(&slot.id).map(|item| (slot, item)))
        .and_then(|(slot, item)| item.active().map(|(_, max)| (slot, item, max)));
    match active {
        Some((slot, item, max)) => {
            active_row.set_if_neq(Visibility::Inherited);
            let filled = slot.charge.min(max) as usize;
            let bar = "■".repeat(filled) + &"□".repeat(max as usize - filled);
            let hint = if slot.charge >= max {
                "  bereit (Q)"
            } else {
                ""
            };
            active_text.0 = format!("{}  {bar}{hint}", item.name);
        }
        None => {
            active_row.set_if_neq(Visibility::Hidden);
        }
    }
}

fn update_stats(
    stats: Query<&PlayerStats, Changed<PlayerStats>>,
    mut text: Single<&mut Text, With<StatsText>>,
) {
    let Ok(PlayerStats { stats: s, pattern }) = stats.single() else {
        return;
    };
    let mut extras = Vec::new();
    if pattern.count > 1 {
        extras.push(format!("{}× Schuss", pattern.count));
    }
    if pattern.piercing {
        extras.push("durchschlagend".to_string());
    }
    if pattern.homing {
        extras.push("zielsuchend".to_string());
    }
    if pattern.bounces > 0 {
        extras.push(format!("{}× Abpraller", pattern.bounces));
    }
    if let Some(p) = pattern.poison {
        extras.push(format!("Gift {:.0} %", p.chance * 100.0));
    }
    if let Some(f) = pattern.freeze {
        extras.push(format!("Frost {:.0} %", f.chance * 100.0));
    }
    if let Some(c) = pattern.crit {
        extras.push(format!("Krit {:.0} %", c.chance * 100.0));
    }
    let extras = if extras.is_empty() {
        String::new()
    } else {
        format!("   ({})", extras.join(", "))
    };
    text.0 = format!(
        "Schaden {:.1}   Feuerrate {:.2}/s   Tempo {:.1}   Reichweite {:.1}   Schussgeschw. {:.1}{extras}",
        s.damage, s.fire_rate, s.move_speed, s.range, s.shot_speed
    );
}
