//! Benutzeroberfläche: Menüs, Minimap und Herz-Anzeige.
//!
//! Jeder Button trägt eine `MenuAction`. Ein einziges System reagiert auf
//! alle Buttons – die Screens beschreiben nur, *was* ein Button tut,
//! nicht *wie* es ausgeführt wird.

use bevy::prelude::*;

use crate::{
    assets::GameAssets,
    states::{AppState, InGameState},
};

mod hud;
mod menus;
mod minimap;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((menus::MenusPlugin, minimap::MinimapPlugin, hud::HudPlugin))
            .add_systems(Update, (apply_game_font, button_visuals, button_actions));
    }
}

/// Was ein Button auslöst.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    StartRun,
    Resume,
    ToMainMenu,
    Quit,
}

const BUTTON_NORMAL: Color = Color::srgb(0.18, 0.16, 0.15);
const BUTTON_HOVERED: Color = Color::srgb(0.28, 0.25, 0.22);
const BUTTON_PRESSED: Color = Color::srgb(0.45, 0.38, 0.28);
const TEXT_COLOR: Color = Color::srgb(0.92, 0.88, 0.80);
const TEXT_DIM: Color = Color::srgb(0.55, 0.52, 0.48);

// --- Bausteine -------------------------------------------------------------

/// Bildschirmfüllender Container, der seinen Inhalt zentriert untereinander stapelt.
fn screen_root(background: Color) -> impl Bundle {
    (
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(16),
            ..default()
        },
        BackgroundColor(background),
    )
}

fn title(text: &str) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(64.0),
            ..default()
        },
        TextColor(TEXT_COLOR),
        Node {
            margin: UiRect::bottom(px(24)),
            ..default()
        },
    )
}

fn hint(text: &str) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(TEXT_DIM),
        Node {
            margin: UiRect::top(px(24)),
            ..default()
        },
    )
}

/// `children![]` erzeugt die Kind-Entities direkt beim Spawnen des Buttons.
fn button(label: &str, action: MenuAction) -> impl Bundle {
    (
        Button,
        action,
        Node {
            width: px(280),
            height: px(56),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(BUTTON_NORMAL),
        children![(
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(26.0),
                ..default()
            },
            TextColor(TEXT_COLOR),
        )],
    )
}

// --- Systeme ---------------------------------------------------------------

/// Gibt jedem neu erzeugten Text die Spielschrift. So muss kein Spawn-Code
/// an die Schrift denken, und `button()`/`title()` bleiben ohne Parameter.
/// Die eingebaute Bevy-Schrift ist nur ein kleiner Ausschnitt ohne Umlaute.
fn apply_game_font(assets: Res<GameAssets>, mut texts: Query<&mut TextFont, Added<TextFont>>) {
    for mut font in &mut texts {
        font.font = assets.font.clone().into();
    }
}

/// `Changed<Interaction>`: Nur Buttons, deren Zustand sich seit dem letzten
/// Durchlauf geändert hat – nicht jedes Bild alle Buttons.
fn button_visuals(
    mut query: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<Button>)>,
) {
    for (interaction, mut background) in &mut query {
        background.0 = match interaction {
            Interaction::Pressed => BUTTON_PRESSED,
            Interaction::Hovered => BUTTON_HOVERED,
            Interaction::None => BUTTON_NORMAL,
        };
    }
}

fn button_actions(
    query: Query<(&Interaction, &MenuAction), Changed<Interaction>>,
    mut app_state: ResMut<NextState<AppState>>,
    mut in_game: ResMut<NextState<InGameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, action) in &query {
        if *interaction == Interaction::Pressed {
            perform(*action, &mut app_state, &mut in_game, &mut exit);
        }
    }
}

/// Auch von Tastenkürzeln aus aufrufbar (siehe `menus.rs`).
fn perform(
    action: MenuAction,
    app_state: &mut NextState<AppState>,
    in_game: &mut NextState<InGameState>,
    exit: &mut MessageWriter<AppExit>,
) {
    match action {
        MenuAction::StartRun => app_state.set(AppState::InGame),
        MenuAction::Resume => in_game.set(InGameState::Playing),
        MenuAction::ToMainMenu => app_state.set(AppState::MainMenu),
        MenuAction::Quit => {
            exit.write(AppExit::Success);
        }
    }
}
