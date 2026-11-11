//! Flüche im Spiel: die Teile, die Bevy brauchen.
//!
//! *Welcher* Fluch auf einer Etage liegt, entscheidet `dungeon_gen::curses`
//! (abrufbar über `Run::modifier`). Die meisten Flüche wirken dort, wo der
//! betroffene Wert ohnehin entsteht:
//!
//! | Fluch / Segen | wirkt in                                   |
//! |---------------|--------------------------------------------|
//! | Hast          | `enemy::spawn_enemy` (Tempo)               |
//! | Zähigkeit     | `enemy::spawn_enemy` (Leben)               |
//! | Verirrt       | `ui::minimap::rebuild_minimap`             |
//! | Glückssträhne | `inventory::reward_room_clear`             |
//! | Dunkelheit    | hier: `sync_darkness`                      |
//! | Blindkauf     | hier: `hide_prices`                        |

use bevy::prelude::*;
use dungeon_gen::FloorModifier;

use crate::{
    TILE_SIZE, assets::GameAssets, inventory::PriceTag, pixel_art::tiles::VIGNETTE_TILES,
    player::Player, run::Run, states::AppState,
};

pub struct CursePlugin;

impl Plugin for CursePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (sync_darkness, hide_prices)
                .run_if(in_state(AppState::InGame))
                // Zwischen `OnEnter` und dem ersten Bild existiert `Run` sicher –
                // die Bedingung schützt nur vor Überraschungen beim Verlassen.
                .run_if(resource_exists::<Run>),
        );
    }
}

/// Das dunkle Bild mit Lichtkreis, das dem Spieler folgt.
#[derive(Component)]
struct DarknessOverlay;

/// Hält die Dunkelheit im Einklang mit dem Fluch der Etage: erzeugen, wenn
/// sie fehlt; entfernen, wenn sie nicht mehr gilt; sonst dem Spieler folgen.
///
/// Läuft in `Update` und liest `Transform` (nicht `Position`): Das ist die
/// interpolierte, tatsächlich gezeichnete Stelle – so ruckelt der Lichtkreis nicht.
fn sync_darkness(
    mut commands: Commands,
    run: Res<Run>,
    assets: Res<GameAssets>,
    player: Query<&Transform, (With<Player>, Without<DarknessOverlay>)>,
    mut overlay: Query<(Entity, &mut Transform), With<DarknessOverlay>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let center = player.translation.truncate();
    let wanted = run.modifier() == Some(FloorModifier::Darkness);

    // Ein `match` über das Tupel deckt alle vier Fälle ab – der Compiler
    // prüft, dass keiner fehlt.
    match (wanted, overlay.single_mut()) {
        (true, Ok((_, mut transform))) => {
            transform.translation.x = center.x;
            transform.translation.y = center.y;
        }
        (true, Err(_)) => {
            commands.spawn((
                Name::new("Darkness"),
                DarknessOverlay,
                DespawnOnExit(AppState::InGame),
                Sprite {
                    image: assets.sprites.darkness.clone(),
                    custom_size: Some(Vec2::splat(VIGNETTE_TILES * TILE_SIZE)),
                    ..default()
                },
                // z = 50: über allem in der Spielwelt. Das HUD ist UI und
                // liegt ohnehin darüber.
                Transform::from_translation(center.extend(50.0)),
            ));
        }
        (false, Ok((entity, _))) => {
            commands.entity(entity).despawn();
        }
        (false, Err(_)) => {}
    }
}

/// Fluch „Blindkauf“: Neue Preisschilder zeigen „?“. `Added<PriceTag>`
/// trifft jedes Schild genau einmal, direkt nach dem Erzeugen.
fn hide_prices(run: Res<Run>, mut tags: Query<&mut Text2d, Added<PriceTag>>) {
    if run.modifier() != Some(FloorModifier::BlindBuy) {
        return;
    }
    for mut text in &mut tags {
        text.0 = "?".to_string();
    }
}
