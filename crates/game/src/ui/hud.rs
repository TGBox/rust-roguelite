//! HUD oben links: Herzen, darunter Münzen/Schlüssel/Bomben und die aktuellen Werte.
//!
//! Ein Herz = zwei Hälften, jede Hälfte = 1 Lebenspunkt (wie bei Isaac).
//! Platzhalter aus farbigen Rechtecken und Text; echte Grafiken folgen in M8.

use bevy::prelude::*;

use crate::{
    combat::Health,
    player::{Player, PlayerStats},
    run::Run,
    states::AppState,
};

const HALF_WIDTH: f32 = 11.0;
const HEART_HEIGHT: f32 = 20.0;
const FULL: Color = Color::srgb(0.85, 0.15, 0.20);
const EMPTY: Color = Color::srgb(0.20, 0.08, 0.10);
const TEXT: Color = Color::srgb(0.90, 0.87, 0.80);
const TEXT_DIM: Color = Color::srgb(0.60, 0.58, 0.54);

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

#[derive(Component)]
struct CounterText;

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

fn spawn_hud(mut commands: Commands) {
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
                    column_gap: px(2),
                    ..default()
                }
            ),
            (CounterText, hud_text(18.0, TEXT)),
            (StatsText, hud_text(14.0, TEXT_DIM)),
        ],
    ));
}

/// Läuft nur, wenn sich das Leben des Spielers geändert hat
/// (beim ersten Mal zählt das Hinzufügen der Komponente als Änderung).
fn update_hearts(
    mut commands: Commands,
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
    for i in 0..health.max.round() as i32 {
        let filled = (i as f32) < health.current;
        // Nach jedem vollen Herz (2 Hälften) etwas Abstand.
        let gap = if i % 2 == 1 { 6.0 } else { 0.0 };
        commands.spawn((
            HeartSegment,
            ChildOf(*bar),
            Node {
                width: px(HALF_WIDTH),
                height: px(HEART_HEIGHT),
                margin: UiRect::right(px(gap)),
                ..default()
            },
            BackgroundColor(if filled { FULL } else { EMPTY }),
        ));
    }
}

fn update_counters(run: Res<Run>, mut text: Single<&mut Text, With<CounterText>>) {
    let inv = &run.inventory;
    let items = if inv.items.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = inv.items.iter().map(|i| i.name).collect();
        format!("\nItems: {}", names.join(", "))
    };
    text.0 = format!(
        "Münzen {}   Schlüssel {}   Bomben {}{items}",
        inv.coins, inv.keys, inv.bombs
    );
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
