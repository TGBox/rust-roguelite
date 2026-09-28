//! „Game Feel“: Bildschirmwackeln, Hitstop, Partikel und kleine Animationen.
//!
//! Spiellogik schreibt nur `Fx`-Messages („hier soll es krachen“), dieses
//! Modul setzt sie um – dasselbe Muster wie bei `Damage`: Erkennen und
//! Darstellen sind getrennt. Nichts hier beeinflusst die Spiellogik; man
//! könnte das ganze Plugin abschalten, und das Spiel liefe identisch.

use std::f32::consts::TAU;

use bevy::{prelude::*, transform::TransformSystems};

use crate::{
    TILE_SIZE,
    assets::GameAssets,
    camera::MainCamera,
    combat::{Frozen, Poisoned},
    enemy::{Asleep, BroodAi, Champion, Charger, Enemy},
    physics::{Position, Velocity},
    pixel_art::canvas::noise,
    player::{Player, PlayerInput},
    room::RoomScoped,
    states::{AppState, InGameState},
};

/// Größter Kameraversatz bei vollem Trauma (Pixel).
const MAX_SHAKE_PX: f32 = 7.0;
/// Wie schnell das Trauma abklingt (pro Sekunde).
const TRAUMA_DECAY: f32 = 2.2;
/// Spielgeschwindigkeit während eines Hitstops. Nicht 0: `set_relative_speed`
/// lehnt 0 ab, und ein Hauch Bewegung wirkt ohnehin lebendiger.
const HITSTOP_SPEED: f32 = 0.05;

pub struct JuicePlugin;

impl Plugin for JuicePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Fx>()
            .init_resource::<Shake>()
            .init_resource::<Hitstop>()
            .init_resource::<ParticleSeed>()
            // Versatz vom letzten Bild zuerst entfernen: Alle anderen Systeme
            // sehen so die „echte“ Kameraposition (z. B. der Raumwechsel).
            .add_systems(PreUpdate, remove_shake_offset)
            .add_systems(
                Update,
                (
                    handle_fx,
                    tick_hitstop,
                    update_particles,
                    wobble_actors,
                    face_direction,
                    tint_enemies,
                )
                    .chain(),
            )
            // Ganz am Ende, kurz bevor Bevy die Transforms für das Rendering
            // zusammenrechnet.
            .add_systems(
                PostUpdate,
                apply_shake_offset.before(TransformSystems::Propagate),
            )
            .add_systems(OnExit(InGameState::Playing), end_hitstop)
            .add_systems(OnExit(AppState::InGame), |mut shake: ResMut<Shake>| {
                *shake = Shake::default();
            });
    }
}

// --- Schnittstelle für die Spiellogik -------------------------------------------

/// Ein Effekt-Wunsch. Enum statt drei Message-Typen: ein Writer reicht.
#[derive(Message, Debug, Clone, Copy)]
pub enum Fx {
    /// Partikel-Explosion an einer Stelle.
    Burst {
        at: Vec2,
        color: Color,
        count: u32,
        /// Anfangsgeschwindigkeit (Pixel/s).
        speed: f32,
    },
    /// Trauma für das Bildschirmwackeln (0.0 … 1.0, addiert sich).
    Shake(f32),
    /// Spiel für diese Zeit (echte Sekunden) fast anhalten.
    Hitstop(f32),
}

/// Farbe für Partikel, wenn diese Entity getroffen wird oder stirbt.
#[derive(Component, Debug, Clone, Copy)]
pub struct FxColor(pub Color);

/// Quetschen und Strecken beim Laufen. `base` ist die normale Sprite-Größe.
#[derive(Component, Debug, Clone, Copy)]
pub struct Wobble {
    pub base: Vec2,
    /// Versatz, damit nicht alle Gegner im Gleichtakt hüpfen.
    pub phase: f32,
}

impl Wobble {
    pub fn new(base: Vec2, spawn_pos: Vec2) -> Self {
        Self {
            base,
            phase: (spawn_pos.x * 0.37 + spawn_pos.y * 0.21) % TAU,
        }
    }
}

// --- Bildschirmwackeln ----------------------------------------------------------

/// „Trauma“-Modell: Treffer erhöhen das Trauma, der Versatz wächst mit
/// Trauma², so wirken kleine Treffer dezent und große wuchtig.
#[derive(Resource, Debug, Default)]
struct Shake {
    trauma: f32,
    /// Der im letzten Bild auf die Kamera addierte Versatz.
    applied: Vec2,
}

fn remove_shake_offset(
    mut shake: ResMut<Shake>,
    camera: Option<Single<&mut Transform, With<MainCamera>>>,
) {
    if let Some(mut camera) = camera {
        camera.translation -= shake.applied.extend(0.0);
    }
    shake.applied = Vec2::ZERO;
}

fn apply_shake_offset(
    time: Res<Time<Real>>,
    state: Option<Res<State<InGameState>>>,
    mut shake: ResMut<Shake>,
    camera: Option<Single<&mut Transform, With<MainCamera>>>,
) {
    // Echte Zeit: Das Wackeln klingt auch während eines Hitstops ab.
    let dt = time.delta_secs();
    shake.trauma = (shake.trauma - TRAUMA_DECAY * dt).max(0.0);

    // Nur beim Spielen wackeln – nicht im Pausemenü und nicht beim Raumwechsel,
    // wo die Kamera selbst animiert wird.
    let playing = state.is_some_and(|s| *s.get() == InGameState::Playing);
    let (Some(mut camera), true) = (camera, playing) else {
        return;
    };
    if shake.trauma <= 0.0 {
        return;
    }
    // Zwei Sinuswellen mit „krummen“ Frequenzen ergeben ein unruhiges,
    // aber glattes Zittern – ohne Zufallsgenerator.
    let t = time.elapsed_secs() * 40.0;
    let dir = Vec2::new(
        (t * 1.13).sin() + (t * 2.71).sin() * 0.5,
        (t * 1.37 + 1.0).sin() + (t * 3.07).sin() * 0.5,
    ) / 1.5;
    let offset = dir * MAX_SHAKE_PX * shake.trauma * shake.trauma;
    camera.translation += offset.extend(0.0);
    shake.applied = offset;
}

// --- Hitstop ---------------------------------------------------------------------

/// Verbleibende echte Sekunden. Solange > 0, läuft die virtuelle Zeit fast nicht.
#[derive(Resource, Debug, Default)]
struct Hitstop(f32);

fn tick_hitstop(
    time: Res<Time<Real>>,
    mut hitstop: ResMut<Hitstop>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    if hitstop.0 <= 0.0 {
        return;
    }
    hitstop.0 -= time.delta_secs();
    if hitstop.0 <= 0.0 {
        virtual_time.set_relative_speed(1.0);
    }
}

/// Pause oder Raumwechsel während eines Hitstops: normale Geschwindigkeit
/// wiederherstellen, sonst liefe das Spiel danach in Zeitlupe weiter.
fn end_hitstop(mut hitstop: ResMut<Hitstop>, mut virtual_time: ResMut<Time<Virtual>>) {
    hitstop.0 = 0.0;
    virtual_time.set_relative_speed(1.0);
}

// --- Partikel ---------------------------------------------------------------------

#[derive(Component, Debug)]
struct Particle {
    velocity: Vec2,
    age: f32,
    lifetime: f32,
    size: f32,
    color: Color,
}

/// Zähler für das Partikel-„Zufallsrauschen“. Partikel sind reine Optik,
/// deshalb brauchen sie keinen Seed-Stream wie die Spiellogik.
#[derive(Resource, Debug, Default)]
struct ParticleSeed(u32);

impl ParticleSeed {
    /// Nächste Zahl im Bereich 0.0 … 1.0.
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(1);
        noise(self.0 as i32, 0, 0xFACE) as f32 / u32::MAX as f32
    }
}

fn handle_fx(
    mut commands: Commands,
    mut messages: MessageReader<Fx>,
    mut shake: ResMut<Shake>,
    mut hitstop: ResMut<Hitstop>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut seed: ResMut<ParticleSeed>,
    state: Option<Res<State<InGameState>>>,
    assets: Res<GameAssets>,
) {
    let playing = state.is_some_and(|s| *s.get() == InGameState::Playing);
    for fx in messages.read() {
        match *fx {
            Fx::Shake(amount) => shake.trauma = (shake.trauma + amount).min(1.0),
            Fx::Hitstop(secs) => {
                // Stirbt der Spieler, wechselt der Zustand im selben Tick –
                // dann keinen Hitstop mehr starten (`end_hitstop` lief schon).
                if playing {
                    hitstop.0 = hitstop.0.max(secs);
                    virtual_time.set_relative_speed(HITSTOP_SPEED);
                }
            }
            Fx::Burst {
                at,
                color,
                count,
                speed,
            } => {
                for _ in 0..count {
                    let angle = seed.next() * TAU;
                    let velocity = Vec2::from_angle(angle) * speed * (0.35 + 0.65 * seed.next());
                    let size = 2.0 + 3.0 * seed.next();
                    commands.spawn((
                        Name::new("Particle"),
                        Particle {
                            velocity,
                            age: 0.0,
                            lifetime: 0.25 + 0.35 * seed.next(),
                            size,
                            color,
                        },
                        RoomScoped,
                        DespawnOnExit(AppState::InGame),
                        Sprite {
                            image: assets.sprites.pixel.clone(),
                            color,
                            custom_size: Some(Vec2::splat(size)),
                            ..default()
                        },
                        // Über den Figuren, unter der UI.
                        Transform::from_translation(at.extend(30.0)),
                    ));
                }
            }
        }
    }
}

/// Virtuelle Zeit: Partikel frieren im Hitstop und in der Pause mit ein.
fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Particle, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    // Luftwiderstand: framerate-unabhängig über `exp`.
    let drag = (-5.0 * dt).exp();
    for (entity, mut p, mut transform, mut sprite) in &mut query {
        p.age += dt;
        let life = 1.0 - p.age / p.lifetime;
        if life <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += (p.velocity * dt).extend(0.0);
        p.velocity *= drag;
        sprite.custom_size = Some(Vec2::splat(p.size * (0.4 + 0.6 * life)));
        sprite.color = p.color.with_alpha(life.min(1.0));
    }
}

// --- Figuren-Animation ---------------------------------------------------------------

/// Beim Laufen hüpfen, im Stand leicht „atmen“.
fn wobble_actors(time: Res<Time>, mut query: Query<(&Wobble, &Velocity, &mut Sprite)>) {
    let t = time.elapsed_secs();
    for (wobble, velocity, mut sprite) in &mut query {
        let moving = (velocity.0.length() / (3.0 * TILE_SIZE)).min(1.0);
        let (amount, freq) = if moving > 0.1 {
            (0.10 * moving, 16.0)
        } else {
            (0.03, 3.0)
        };
        let s = (t * freq + wobble.phase).sin() * amount;
        // Breiter UND flacher (oder umgekehrt): Das Volumen bleibt ungefähr gleich.
        sprite.custom_size = Some(wobble.base * Vec2::new(1.0 + s, 1.0 - s));
    }
}

/// Alle Figuren schauen standardmäßig nach rechts; nach links wird gespiegelt.
fn face_direction(
    input: Res<PlayerInput>,
    mut player: Query<(&Velocity, &mut Sprite), (With<Player>, Without<Enemy>)>,
    player_pos: Query<&Position, With<Player>>,
    mut enemies: Query<(&Position, &mut Sprite), With<Enemy>>,
) {
    for (velocity, mut sprite) in &mut player {
        // Schussrichtung hat Vorrang: Man schaut dahin, wohin man schießt.
        let x = input
            .shoot_dir
            .map(|d| d.x)
            .filter(|x| *x != 0.0)
            .unwrap_or(velocity.0.x);
        if x.abs() > 1.0e-3 {
            sprite.flip_x = x < 0.0;
        }
    }
    let Ok(target) = player_pos.single() else {
        return;
    };
    for (pos, mut sprite) in &mut enemies {
        let dx = target.0.x - pos.0.x;
        // Kleine Totzone gegen Flackern, wenn der Spieler direkt darüber steht.
        if dx.abs() > 4.0 {
            sprite.flip_x = dx < 0.0;
        }
    }
}

/// Einfärben nach Zustand: schlafend halb durchsichtig, Charger beim Ausholen
/// rot pulsierend und geduckt, benommen grau.
///
/// Reihenfolge = Priorität: Frost > Gift > Champion-Gold; Ausholen überschreibt alles.
fn tint_enemies(
    time: Res<Time>,
    mut query: Query<
        (
            &mut Sprite,
            Has<Asleep>,
            Has<Frozen>,
            Has<Poisoned>,
            Has<Champion>,
            Option<&Charger>,
            Option<&BroodAi>,
        ),
        With<Enemy>,
    >,
) {
    let t = time.elapsed_secs();
    for (mut sprite, asleep, frozen, poisoned, champion, charger, brood) in &mut query {
        let mut color = if frozen {
            Color::srgb(0.55, 0.85, 1.0)
        } else if poisoned {
            // Leichtes Pulsieren, damit man den Schaden über Zeit „sieht“.
            let pulse = 0.5 + 0.5 * (t * 8.0).sin();
            Color::srgb(0.6 + 0.2 * pulse, 1.0, 0.5)
        } else if champion {
            Color::srgb(1.0, 0.85, 0.45)
        } else {
            Color::WHITE
        };
        // Beim Einschlafen nach dem Betreten halb durchsichtig – eingefroren nicht.
        if asleep && !frozen {
            color = color.with_alpha(0.55);
        }
        if brood.is_some_and(|b| b.is_winding_up()) {
            let pulse = 0.5 + 0.5 * (t * 30.0).sin();
            color = Color::srgb(1.0, 1.0 - 0.6 * pulse, 1.0 - 0.6 * pulse);
        }
        if let Some(charger) = charger {
            if let Some(progress) = charger.windup_progress() {
                // Immer schneller pulsieren, je näher der Sprint rückt.
                let pulse = 0.5 + 0.5 * (t * (20.0 + 30.0 * progress)).sin();
                color = Color::srgb(1.0, 1.0 - 0.6 * pulse, 1.0 - 0.6 * pulse);
                // Ducken: breiter und flacher (überschreibt das Wackeln).
                if let Some(size) = sprite.custom_size.as_mut() {
                    *size *= Vec2::new(1.0 + 0.2 * progress, 1.0 - 0.2 * progress);
                }
            } else if charger.is_stunned() {
                color = Color::srgb(0.65, 0.65, 0.75);
            }
        }
        sprite.color = color;
    }
}
