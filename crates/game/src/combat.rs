//! Kampf: Lebenspunkte, Schaden, Unverwundbarkeit, Rückstoß, Treffer-Feedback.
//!
//! Ablauf pro Tick (alles in `GameSet::Combat`, in dieser Reihenfolge):
//! 1. `projectile_hits` / `contact_damage` **erkennen** Treffer und schreiben
//!    `Damage`-Messages. Sie ändern selbst nichts.
//! 2. `apply_damage` **wendet** alle Messages an: Leben abziehen, Rückstoß,
//!    Unverwundbarkeit, Aufblitzen.
//! 3. `handle_deaths` entfernt tote Gegner bzw. lässt den Spieler sterben.
//!
//! Die Trennung von Erkennen und Anwenden hält beide Seiten einfach: Neue
//! Schadensquellen (Explosionen, Stacheln, Items) schreiben nur Messages.

use bevy::prelude::*;
use dungeon_gen::collision::aabb_overlap;

use crate::{
    assets::ActorArt,
    audio::{Effect, Sfx},
    enemy::{Asleep, Boss, Enemy},
    juice::{Fx, FxColor},
    physics::{Body, Position, Velocity},
    player::Player,
    projectile::Projectile,
    schedule::GameSet,
    states::InGameState,
};

/// Unverwundbarkeit des Spielers nach einem Treffer (s).
const PLAYER_IFRAMES: f32 = 1.0;
/// Dauer des weißen Aufblitzens nach einem Treffer (s).
const FLASH_SECS: f32 = 0.08;
/// Rückstoß durch Schüsse bzw. Körperkontakt (Pixel/s).
const SHOT_KNOCKBACK: f32 = 90.0;
const CONTACT_KNOCKBACK: f32 = 260.0;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Damage>()
            .add_message::<EnemyKilled>()
            .add_systems(
                FixedUpdate,
                (
                    (projectile_hits, contact_damage),
                    apply_damage,
                    handle_deaths,
                    tick_invulnerability,
                )
                    .chain()
                    .in_set(GameSet::Combat),
            )
            .add_systems(Update, (tick_hit_flash, blink_while_invulnerable));
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn full(max: f32) -> Self {
        Self { current: max, max }
    }
}

/// Wer auf wen schießen darf.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Faction {
    Player,
    Enemy,
}

/// Schaden bei Berührung mit dem Spieler.
#[derive(Component, Debug)]
pub struct ContactDamage(pub f32);

/// Verbleibende Unverwundbarkeit (s). Nur der Spieler hat diese Komponente.
#[derive(Component, Debug, Default)]
pub struct Invulnerable(pub f32);

/// Normales Bild und weiße Silhouette für das Aufblitzen bei Treffern.
#[derive(Component, Debug, Clone)]
pub struct FlashArt {
    pub normal: Handle<Image>,
    pub flash: Handle<Image>,
}

/// `From` statt einer `new`-Funktion: Aufrufer schreiben `FlashArt::from(art)`
/// oder – wo der Typ feststeht – einfach `art.into()`.
impl From<&ActorArt> for FlashArt {
    fn from(art: &ActorArt) -> Self {
        Self {
            normal: art.image.clone(),
            flash: art.flash.clone(),
        }
    }
}

#[derive(Component, Debug)]
struct HitFlash(f32);

/// Ein Gegner ist gestorben (für Item-Effekte wie „Münze pro Kill“).
#[derive(Message, Debug, Clone, Copy)]
pub struct EnemyKilled;

#[derive(Message, Debug, Clone, Copy)]
pub struct Damage {
    pub target: Entity,
    pub amount: f32,
    /// Stoß in Pixel/s, wird auf die Geschwindigkeit addiert.
    pub knockback: Vec2,
}

fn projectile_hits(
    mut commands: Commands,
    mut projectiles: Query<(Entity, &Position, &Body, &Velocity, &mut Projectile)>,
    targets: Query<(Entity, &Position, &Body, &Faction, &Health), Without<Projectile>>,
    mut damage: MessageWriter<Damage>,
) {
    for (shot, shot_pos, shot_body, shot_vel, mut projectile) in &mut projectiles {
        for (target, pos, body, faction, health) in &targets {
            if *faction == projectile.faction
                || health.current <= 0.0
                || projectile.already_hit.contains(&target)
            {
                continue;
            }
            let hit = aabb_overlap(
                shot_pos.0.to_array(),
                shot_body.half_size.to_array(),
                pos.0.to_array(),
                body.half_size.to_array(),
            );
            if hit {
                damage.write(Damage {
                    target,
                    amount: projectile.damage,
                    knockback: shot_vel.0.normalize_or_zero() * SHOT_KNOCKBACK,
                });
                if projectile.piercing {
                    // Durchschlagend: weiterfliegen, dieses Ziel aber nie wieder treffen.
                    projectile.already_hit.push(target);
                } else {
                    commands.entity(shot).try_despawn();
                    // Ein normaler Schuss trifft nur ein Ziel.
                    break;
                }
            }
        }
    }
}

fn contact_damage(
    player: Single<(Entity, &Position, &Body, &Invulnerable), With<Player>>,
    enemies: Query<(&Position, &Body, &ContactDamage, &Health), (With<Enemy>, Without<Asleep>)>,
    mut damage: MessageWriter<Damage>,
) {
    let (player, player_pos, player_body, invulnerable) = player.into_inner();
    if invulnerable.0 > 0.0 {
        return;
    }
    for (pos, body, contact, health) in &enemies {
        if health.current <= 0.0 {
            continue;
        }
        let touching = aabb_overlap(
            player_pos.0.to_array(),
            player_body.half_size.to_array(),
            pos.0.to_array(),
            body.half_size.to_array(),
        );
        if touching {
            damage.write(Damage {
                target: player,
                amount: contact.0,
                knockback: (player_pos.0 - pos.0).normalize_or_zero() * CONTACT_KNOCKBACK,
            });
            // Pro Tick höchstens ein Kontakttreffer – danach ist der Spieler unverwundbar.
            break;
        }
    }
}

fn apply_damage(
    mut commands: Commands,
    mut messages: MessageReader<Damage>,
    mut fx: MessageWriter<Fx>,
    mut sfx: MessageWriter<Sfx>,
    mut targets: Query<(
        &mut Health,
        &mut Velocity,
        Option<&mut Invulnerable>,
        &mut Sprite,
        &FlashArt,
        &Position,
        Option<&FxColor>,
    )>,
) {
    for hit in messages.read() {
        // Das Ziel kann im selben Tick schon entfernt worden sein.
        let Ok((mut health, mut velocity, invulnerable, mut sprite, art, pos, color)) =
            targets.get_mut(hit.target)
        else {
            continue;
        };
        let is_player = invulnerable.is_some();
        if let Some(mut inv) = invulnerable {
            if inv.0 > 0.0 {
                continue;
            }
            // Direkt setzen (nicht per Commands), damit ein zweiter Treffer
            // im selben Tick schon abprallt.
            inv.0 = PLAYER_IFRAMES;
        }
        health.current = (health.current - hit.amount).max(0.0);
        velocity.0 += hit.knockback;
        sprite.image = art.flash.clone();
        commands.entity(hit.target).try_insert(HitFlash(FLASH_SECS));

        // Optik: Spritzer in der Farbe des Ziels; ein Spielertreffer soll
        // deutlich spürbar sein.
        let color = color.map_or(Color::WHITE, |c| c.0);
        if is_player {
            sfx.write(Sfx(Effect::PlayerHurt));
            fx.write(Fx::Shake(0.45));
            fx.write(Fx::Hitstop(0.08));
            fx.write(Fx::Burst {
                at: pos.0,
                color: Color::srgb(0.9, 0.15, 0.2),
                count: 10,
                speed: 120.0,
            });
        } else {
            sfx.write(Sfx(Effect::Hit));
            fx.write(Fx::Burst {
                at: pos.0,
                color,
                count: 4,
                speed: 70.0,
            });
        }
    }
}

/// `Changed<Health>`: nur Entities, deren Leben sich in diesem Tick geändert hat.
fn handle_deaths(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &Health,
            &Position,
            Option<&FxColor>,
            Has<Player>,
            Has<Boss>,
        ),
        Changed<Health>,
    >,
    mut next: ResMut<NextState<InGameState>>,
    mut killed: MessageWriter<EnemyKilled>,
    mut fx: MessageWriter<Fx>,
    mut sfx: MessageWriter<Sfx>,
) {
    for (entity, health, pos, color, is_player, is_boss) in &query {
        if health.current > 0.0 {
            continue;
        }
        if is_player {
            fx.write(Fx::Shake(0.8));
            next.set(InGameState::Dying);
            continue;
        }
        commands.entity(entity).try_despawn();
        killed.write(EnemyKilled);

        // Je größer der Gegner, desto mehr Wumms.
        let (count, shake, stop) = if is_boss {
            (48, 0.9, 0.3)
        } else {
            (14, 0.25, 0.035)
        };
        fx.write(Fx::Burst {
            at: pos.0,
            color: color.map_or(Color::WHITE, |c| c.0),
            count,
            speed: if is_boss { 260.0 } else { 150.0 },
        });
        fx.write(Fx::Shake(shake));
        fx.write(Fx::Hitstop(stop));
        sfx.write(Sfx(if is_boss {
            Effect::BossDeath
        } else {
            Effect::EnemyDeath
        }));
    }
}

fn tick_invulnerability(time: Res<Time>, mut query: Query<&mut Invulnerable>) {
    for mut inv in &mut query {
        if inv.0 > 0.0 {
            inv.0 = (inv.0 - time.delta_secs()).max(0.0);
        }
    }
}

fn tick_hit_flash(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut HitFlash, &FlashArt, &mut Sprite)>,
) {
    for (entity, mut flash, art, mut sprite) in &mut query {
        flash.0 -= time.delta_secs();
        if flash.0 <= 0.0 {
            sprite.image = art.normal.clone();
            commands.entity(entity).remove::<HitFlash>();
        }
    }
}

/// Blinken als Zeichen der Unverwundbarkeit.
/// `set_if_neq` schreibt nur bei echter Änderung – so bleibt die
/// Change Detection für `Visibility` ruhig.
fn blink_while_invulnerable(mut query: Query<(&Invulnerable, &mut Visibility)>) {
    for (inv, mut visibility) in &mut query {
        let wanted = if inv.0 > 0.0 && (inv.0 * 12.0) as i32 % 2 == 0 {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        visibility.set_if_neq(wanted);
    }
}
