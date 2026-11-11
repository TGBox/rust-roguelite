//! Taktische Gegner: Zünder, Geschützturm und Wespe.
//!
//! Alle drei zwingen den Spieler, sich zu bewegen statt nur zu schießen:
//! - Der **Zünder** will nah heran – wer stehen bleibt, fliegt mit in die Luft.
//! - Der **Geschützturm** sperrt Linien im Raum, abwechselnd gerade und diagonal.
//! - Die **Wespe** kreist außer Reichweite und stößt dann geradlinig zu.
//!
//! Wie in `advanced.rs` gilt: eine Komponente je Verhalten, der Zustand als
//! privates Enum, nach außen nur lesende Abfragen für die Optik (`juice.rs`).

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use bevy::prelude::*;

use super::{
    Asleep, Mobility, PlayerFlow, chase_direction, chase_direction_for, enemy_shot, steer,
};
use crate::{
    TILE_SIZE,
    assets::GameAssets,
    audio::{Effect, Sfx},
    bomb::PendingExplosions,
    combat::Health,
    physics::{BodyKind, Position, Velocity},
    player::Player,
    room::CurrentRoom,
};

// --- Zünder ------------------------------------------------------------------------

/// Ab diesem Abstand zum Spieler beginnt die Lunte zu brennen (Kacheln).
/// Kleiner als der Explosionsradius: Wer nicht wegläuft, wird getroffen.
const BOMBER_TRIGGER_TILES: f32 = 1.4;
/// Zeit zum Weglaufen (s).
const BOMBER_FUSE_SECS: f32 = 0.8;

// Prüfung zur Compile-Zeit: Ändert jemand eine der beiden Konstanten so,
// dass der Zünder außerhalb seiner eigenen Explosion auslöst, baut das Spiel
// nicht mehr. `const _` ist eine namenlose Konstante nur für diesen Zweck.
const _: () = assert!(BOMBER_TRIGGER_TILES < crate::bomb::BLAST_RADIUS_TILES);

#[derive(Component, Debug, Default)]
pub struct Bomber {
    state: BomberState,
}

/// `#[default]` markiert die Variante, die `Default::default()` liefert –
/// so lässt sich `Default` auch für Enums ableiten.
#[derive(Debug, Clone, Copy, Default)]
enum BomberState {
    #[default]
    Chasing,
    /// Verbleibende Zeit bis zur Explosion.
    Fuse(f32),
}

impl Bomber {
    /// 0.0 (Lunte brennt gerade an) … 1.0 (explodiert gleich), sonst `None`.
    pub fn fuse_progress(&self) -> Option<f32> {
        match self.state {
            BomberState::Fuse(remaining) => Some(1.0 - remaining / BOMBER_FUSE_SECS),
            BomberState::Chasing => None,
        }
    }
}

/// Verfolgen → Lunte → Explosion. Wer den Zünder vorher tötet, verhindert sie.
///
/// Die Explosion läuft über denselben Weg wie Bomben und Fässer
/// (`PendingExplosions`): Sie trifft also auch andere Gegner, sprengt Felsen
/// und zündet Fässer in der Nähe.
pub fn bomber_ai(
    time: Res<Time>,
    room: Res<CurrentRoom>,
    flow: Option<Res<PlayerFlow>>,
    player: Single<&Position, With<Player>>,
    mut pending: ResMut<PendingExplosions>,
    mut query: Query<
        (
            &Position,
            &mut Velocity,
            &Mobility,
            &mut Health,
            &mut Bomber,
        ),
        Without<Asleep>,
    >,
) {
    let dt = time.delta_secs();
    for (pos, mut vel, mob, mut health, mut bomber) in &mut query {
        let current = bomber.state;
        bomber.state = match current {
            BomberState::Chasing => {
                let dir = chase_direction(flow.as_deref(), &room, pos.0, player.0);
                steer(&mut vel, dir * mob.speed * TILE_SIZE, mob.acceleration, dt);
                if pos.0.distance(player.0) < BOMBER_TRIGGER_TILES * TILE_SIZE {
                    BomberState::Fuse(BOMBER_FUSE_SECS)
                } else {
                    BomberState::Chasing
                }
            }
            BomberState::Fuse(remaining) => {
                steer(&mut vel, Vec2::ZERO, 12.0, dt);
                if remaining - dt > 0.0 {
                    BomberState::Fuse(remaining - dt)
                } else {
                    pending.0.push(pos.0);
                    // Selbst sterben, statt auf den Explosionsschaden zu
                    // vertrauen: Ein Champion hätte dafür zu viel Leben.
                    // `combat::handle_deaths` räumt ihn noch in diesem Tick ab
                    // (es reagiert auf `Changed<Health>`).
                    health.current = 0.0;
                    BomberState::Chasing
                }
            }
        };
    }
}

// --- Geschützturm ------------------------------------------------------------------

const TURRET_FIRE_DELAY: f32 = 2.2;
/// So lange vor der Salve leuchtet der Turm auf (s).
const TURRET_WARN_SECS: f32 = 0.5;
const TURRET_SHOT_SPEED: f32 = 4.0;

#[derive(Component, Debug)]
pub struct Turret {
    cooldown: f32,
    /// Nächste Salve diagonal statt gerade?
    diagonal: bool,
}

impl Default for Turret {
    fn default() -> Self {
        Self {
            cooldown: 1.2,
            diagonal: false,
        }
    }
}

impl Turret {
    /// 0.0 … 1.0 kurz vor der Salve, sonst `None`.
    pub fn windup_progress(&self) -> Option<f32> {
        (self.cooldown < TURRET_WARN_SECS).then(|| 1.0 - self.cooldown.max(0.0) / TURRET_WARN_SECS)
    }
}

/// Steht still und feuert vier Schüsse – abwechselnd als „+“ und als „ד.
/// Der Wechsel macht aus sicheren Positionen unsichere.
pub fn turret_ai(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    mut query: Query<(&Position, &mut Velocity, &Mobility, &mut Turret), Without<Asleep>>,
    mut sfx: MessageWriter<Sfx>,
) {
    let dt = time.delta_secs();
    for (pos, mut vel, mob, mut turret) in &mut query {
        // Rückstoß von Treffern sofort abbremsen.
        steer(&mut vel, Vec2::ZERO, mob.acceleration, dt);

        turret.cooldown -= dt;
        if turret.cooldown > 0.0 {
            continue;
        }
        turret.cooldown = TURRET_FIRE_DELAY;
        let offset = if turret.diagonal { FRAC_PI_4 } else { 0.0 };
        turret.diagonal = !turret.diagonal;
        for i in 0..4 {
            let dir = Vec2::from_angle(offset + i as f32 * FRAC_PI_2);
            commands.spawn(enemy_shot(pos.0, dir, TURRET_SHOT_SPEED, &assets));
        }
        sfx.write(Sfx(Effect::EnemyShoot));
    }
}

// --- Wespe -------------------------------------------------------------------------

/// Abstand, in dem die Wespe kreist (Kacheln).
const WASP_ORBIT_TILES: f32 = 3.5;
const WASP_CIRCLE_SECS: f32 = 2.2;
const WASP_WINDUP_SECS: f32 = 0.45;
const WASP_DIVE_SPEED_TILES: f32 = 10.0;
const WASP_DIVE_SECS: f32 = 0.55;
const WASP_RECOVER_SECS: f32 = 0.7;

#[derive(Component, Debug)]
pub struct Wasp {
    state: WaspState,
    /// +1.0 = gegen den Uhrzeigersinn, -1.0 = im Uhrzeigersinn.
    spin: f32,
}

#[derive(Debug, Clone, Copy)]
enum WaspState {
    /// Kreist um den Spieler; verbleibende Zeit bis zum nächsten Angriff.
    Circling(f32),
    /// Steht in der Luft und zielt. Die Richtung steht ab hier fest –
    /// wer jetzt zur Seite geht, wird verfehlt.
    Windup {
        remaining: f32,
        dir: Vec2,
    },
    Diving {
        remaining: f32,
        dir: Vec2,
    },
    Recovering(f32),
}

impl Wasp {
    /// Die Drehrichtung hängt von der Startposition ab: deterministisch,
    /// aber nicht bei allen Wespen gleich.
    pub fn new(spawn_pos: Vec2) -> Self {
        let column = (spawn_pos.x / TILE_SIZE).floor() as i32;
        Self {
            state: WaspState::Circling(WASP_CIRCLE_SECS),
            spin: if column % 2 == 0 { 1.0 } else { -1.0 },
        }
    }

    /// 0.0 … 1.0 beim Zielen, sonst `None`.
    pub fn windup_progress(&self) -> Option<f32> {
        match self.state {
            WaspState::Windup { remaining, .. } => Some(1.0 - remaining / WASP_WINDUP_SECS),
            _ => None,
        }
    }
}

/// Kreisen → zielen → zustoßen → erholen → kreisen.
pub fn wasp_ai(
    time: Res<Time>,
    room: Res<CurrentRoom>,
    flow: Option<Res<PlayerFlow>>,
    player: Single<&Position, With<Player>>,
    mut query: Query<(&Position, &mut Velocity, &Mobility, &mut Wasp), Without<Asleep>>,
) {
    let dt = time.delta_secs();
    let orbit = WASP_ORBIT_TILES * TILE_SIZE;
    let dive_speed = WASP_DIVE_SPEED_TILES * TILE_SIZE;

    for (pos, mut vel, mob, mut wasp) in &mut query {
        let current = wasp.state;
        wasp.state = match current {
            WaspState::Circling(remaining) => {
                let to_player = player.0 - pos.0;
                let dist = to_player.length();
                let sight = room.line_of_sight(pos.0, player.0, BodyKind::Flyer);

                let dir = if !sight || dist > orbit + 2.0 * TILE_SIZE {
                    // Zu weit weg oder Fels dazwischen: erst einmal hinfliegen.
                    chase_direction_for(BodyKind::Flyer, flow.as_deref(), &room, pos.0, player.0)
                } else {
                    let radial = to_player.normalize_or_zero();
                    // `perp()` dreht um 90°: die Richtung entlang des Kreises.
                    let tangent = radial.perp() * wasp.spin;
                    // Zu weit draußen → nach innen ziehen, zu nah → nach außen.
                    let correction = ((dist - orbit) / orbit).clamp(-1.0, 1.0);
                    (tangent + radial * correction).normalize_or_zero()
                };
                steer(&mut vel, dir * mob.speed * TILE_SIZE, mob.acceleration, dt);

                if remaining - dt > 0.0 || !sight {
                    WaspState::Circling((remaining - dt).max(0.0))
                } else {
                    WaspState::Windup {
                        remaining: WASP_WINDUP_SECS,
                        dir: to_player.normalize_or_zero(),
                    }
                }
            }
            WaspState::Windup { remaining, dir } => {
                steer(&mut vel, Vec2::ZERO, 12.0, dt);
                if remaining - dt > 0.0 {
                    WaspState::Windup {
                        remaining: remaining - dt,
                        dir,
                    }
                } else {
                    vel.0 = dir * dive_speed;
                    WaspState::Diving {
                        remaining: WASP_DIVE_SECS,
                        dir,
                    }
                }
            }
            WaspState::Diving { remaining, dir } => {
                // Wie beim Charger: Bleibt in Stoßrichtung kaum Tempo übrig,
                // ist die Wespe gegen etwas geprallt (oder wurde getroffen).
                if remaining - dt <= 0.0 || vel.0.dot(dir) < dive_speed * 0.5 {
                    WaspState::Recovering(WASP_RECOVER_SECS)
                } else {
                    vel.0 = dir * dive_speed;
                    WaspState::Diving {
                        remaining: remaining - dt,
                        dir,
                    }
                }
            }
            WaspState::Recovering(remaining) => {
                steer(&mut vel, Vec2::ZERO, mob.acceleration, dt);
                if remaining - dt > 0.0 {
                    WaspState::Recovering(remaining - dt)
                } else {
                    WaspState::Circling(WASP_CIRCLE_SECS)
                }
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_only_show_while_winding_up() {
        let mut bomber = Bomber::default();
        assert_eq!(bomber.fuse_progress(), None);
        bomber.state = BomberState::Fuse(BOMBER_FUSE_SECS / 2.0);
        assert!((bomber.fuse_progress().unwrap() - 0.5).abs() < 1e-5);

        let mut turret = Turret::default();
        assert_eq!(turret.windup_progress(), None);
        turret.cooldown = 0.0;
        assert_eq!(turret.windup_progress(), Some(1.0));

        let mut wasp = Wasp::new(Vec2::ZERO);
        assert_eq!(wasp.windup_progress(), None);
        wasp.state = WaspState::Windup {
            remaining: WASP_WINDUP_SECS,
            dir: Vec2::X,
        };
        assert_eq!(wasp.windup_progress(), Some(0.0));
    }

    #[test]
    fn neighbouring_wasps_circle_in_opposite_directions() {
        let a = Wasp::new(Vec2::new(0.5 * TILE_SIZE, 0.0));
        let b = Wasp::new(Vec2::new(1.5 * TILE_SIZE, 0.0));
        assert_eq!(a.spin, -b.spin);
        // Auch links vom Ursprung (negative Spalten) bleibt es bei ±1.
        let c = Wasp::new(Vec2::new(-0.5 * TILE_SIZE, 0.0));
        assert_eq!(c.spin.abs(), 1.0);
    }
}
