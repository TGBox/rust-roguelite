//! Minimap oben rechts.
//!
//! Sichtbar sind besuchte Räume (farbig nach Art) und deren direkte Nachbarn
//! (grau, „schon mal gesehen“). Der aktuelle Raum ist hell hervorgehoben.
//! Die Karte wird komplett neu aufgebaut, sobald sich `CurrentRoom` ändert –
//! bei höchstens 20 Räumen ist das billiger als jede clevere Aktualisierung.

use bevy::prelude::*;
use dungeon_gen::{GridPos, RoomKind, floor::FLOOR_SIZE};

use crate::{room::CurrentRoom, run::Run, states::AppState};

/// Kantenlänge einer Minimap-Zelle in Pixeln.
const CELL: f32 = 12.0;
const GAP: f32 = 2.0;

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_minimap)
            .add_systems(
                Update,
                rebuild_minimap
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists_and_changed::<CurrentRoom>),
            );
    }
}

#[derive(Component)]
struct Minimap;

#[derive(Component)]
struct MinimapCell;

fn spawn_minimap(mut commands: Commands) {
    let size = FLOOR_SIZE as f32 * CELL;
    commands.spawn((
        Name::new("Minimap"),
        Minimap,
        DespawnOnExit(AppState::InGame),
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            right: px(12),
            width: px(size),
            height: px(size),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
    ));
}

fn rebuild_minimap(
    mut commands: Commands,
    run: Res<Run>,
    room: Res<CurrentRoom>,
    minimap: Single<Entity, With<Minimap>>,
    old_cells: Query<Entity, With<MinimapCell>>,
) {
    for cell in &old_cells {
        commands.entity(cell).despawn();
    }

    for (pos, info) in run.floor.rooms() {
        let visited = run.visited.contains(&pos);
        let seen = visited || pos.neighbors().any(|(_, n)| run.visited.contains(&n));
        if !seen {
            continue;
        }
        let color = if pos == room.pos {
            Color::srgb(0.95, 0.95, 0.95)
        } else if !visited {
            Color::srgb(0.30, 0.30, 0.30)
        } else {
            kind_color(info.kind)
        };
        commands.spawn((
            MinimapCell,
            // `ChildOf` hängt die Zelle an die Minimap: Sie wird relativ dazu
            // positioniert und mit ihr zusammen entfernt.
            ChildOf(*minimap),
            cell_node(pos),
            BackgroundColor(color),
        ));
    }
}

fn kind_color(kind: RoomKind) -> Color {
    match kind {
        RoomKind::Start | RoomKind::Normal => Color::srgb(0.55, 0.52, 0.48),
        RoomKind::Boss => Color::srgb(0.75, 0.20, 0.20),
        RoomKind::Treasure => Color::srgb(0.90, 0.75, 0.20),
        RoomKind::Shop => Color::srgb(0.30, 0.65, 0.35),
    }
}

/// UI-Koordinaten wachsen nach unten, das Etagenraster nach oben – daher gespiegelt.
fn cell_node(pos: GridPos) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(pos.x as f32 * CELL + GAP / 2.0),
        top: px((FLOOR_SIZE - 1 - pos.y) as f32 * CELL + GAP / 2.0),
        width: px(CELL - GAP),
        height: px(CELL - GAP),
        ..default()
    }
}
