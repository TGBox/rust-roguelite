//! HUD: Herzen oben links.
//!
//! Ein Herz = zwei Hälften, jede Hälfte = 1 Lebenspunkt (wie bei Isaac).
//! Platzhalter aus farbigen Rechtecken; echte Grafiken folgen in M8.

use bevy::prelude::*;

use crate::{combat::Health, player::Player, states::AppState};

const HALF_WIDTH: f32 = 11.0;
const HEART_HEIGHT: f32 = 20.0;
const FULL: Color = Color::srgb(0.85, 0.15, 0.20);
const EMPTY: Color = Color::srgb(0.20, 0.08, 0.10);

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_heart_bar)
            .add_systems(Update, update_hearts.run_if(in_state(AppState::InGame)));
    }
}

#[derive(Component)]
struct HeartBar;

#[derive(Component)]
struct HeartSegment;

fn spawn_heart_bar(mut commands: Commands) {
    commands.spawn((
        Name::new("HeartBar"),
        HeartBar,
        DespawnOnExit(AppState::InGame),
        Node {
            position_type: PositionType::Absolute,
            top: px(14),
            left: px(14),
            column_gap: px(2),
            ..default()
        },
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
    for i in 0..health.max {
        let filled = i < health.current;
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
