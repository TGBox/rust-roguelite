//! HUD oben links: Herzen, Münzen/Schlüssel/Bomben mit Symbolen, Items und Werte.
//!
//! Ein Herz = zwei Lebenspunkte (wie bei Isaac): voll, halb oder leer.
//! Die Symbole sind die Pixel-Grafiken aus `pixel_art.rs`.

use bevy::prelude::*;

use crate::{
    assets::GameAssets,
    combat::Health,
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
                    update_counters.run_if(resource_exists_and_changed::<Run>),
                    update_stats,
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
}

#[derive(Component)]
struct ItemsText;

#[derive(Component)]
struct StatsText;

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
                ]
            ),
            (ItemsText, hud_text(14.0, TEXT_DIM)),
            (StatsText, hud_text(14.0, TEXT_DIM)),
        ],
    ));
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
    mut counters: Query<(&Counter, &mut Text)>,
    mut items: Single<&mut Text, (With<ItemsText>, Without<Counter>)>,
) {
    let inv = &run.inventory;
    for (counter, mut text) in &mut counters {
        let value = match counter {
            Counter::Coins => inv.coins,
            Counter::Keys => inv.keys,
            Counter::Bombs => inv.bombs,
        };
        text.0 = format!("{value:02}");
    }
    let names: Vec<&str> = inv.items.iter().map(|i| i.name).collect();
    items.0 = if names.is_empty() {
        String::new()
    } else {
        format!("Items: {}", names.join(", "))
    };
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
