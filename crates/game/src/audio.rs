//! Sound und Musik – alles per Code synthetisiert (siehe `audio/synth.rs`).
//!
//! Ablauf:
//! 1. Beim Start erzeugt `SoundBank` alle Effekte und Musikstücke als
//!    WAV-Bytes und legt sie als `AudioSource`-Assets ab.
//! 2. Spiellogik schreibt `Sfx`-Messages („spiel den Münz-Sound“).
//! 3. `play_sfx` spielt sie ab – mit Begrenzung, damit Dauerfeuer nicht zum
//!    Klangbrei wird.
//! 4. `sync_music` sorgt dafür, dass immer die Musik der aktuellen Etage läuft.
//!
//! Taste **M** schaltet den Ton stumm. Lautstärke-Einstellungen folgen in M9.

use std::collections::BTreeMap;

use bevy::{
    audio::{AudioSinkPlayback, Volume},
    prelude::*,
};

use crate::{
    progress::Descend,
    room::RoomCleared,
    run::Run,
    states::{AppState, InGameState},
};

pub mod synth;

pub use synth::Effect;

/// Höchstens so viele Effekte gleichzeitig.
const MAX_ACTIVE_SFX: usize = 16;
/// Mindestabstand zwischen zwei gleichen Effekten (s).
const MIN_REPEAT_SECS: f64 = 0.045;
const MUSIC_VOLUME: f32 = 0.35;

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Sfx>()
            .init_resource::<SoundBank>()
            .init_resource::<AudioSettings>()
            .add_systems(
                Update,
                (
                    toggle_mute,
                    sfx_from_game_events,
                    play_sfx,
                    sync_music.run_if(in_state(AppState::InGame)),
                )
                    .chain(),
            )
            .add_systems(OnEnter(InGameState::Paused), pause_music)
            .add_systems(OnExit(InGameState::Paused), resume_music);
    }
}

/// „Spiel diesen Effekt ab.“
#[derive(Message, Debug, Clone, Copy)]
pub struct Sfx(pub Effect);

/// Globale Audio-Einstellungen. In M9 kommen hier Lautstärkeregler dazu.
#[derive(Resource, Debug, Default)]
pub struct AudioSettings {
    pub muted: bool,
}

/// Alle erzeugten Klänge als Asset-Handles.
#[derive(Resource)]
struct SoundBank {
    effects: BTreeMap<Effect, Handle<AudioSource>>,
    /// Index 0 = Etage 1.
    music: Vec<Handle<AudioSource>>,
}

impl FromWorld for SoundBank {
    fn from_world(world: &mut World) -> Self {
        let start = std::time::Instant::now();
        let mut sources = world.resource_mut::<Assets<AudioSource>>();
        // `AudioSource` ist nur eine Hülle um die Bytes einer Audiodatei.
        // `Arc<[u8]>` statt `Vec<u8>`: mehrere Abspielvorgänge teilen sich
        // dieselben Bytes, ohne sie zu kopieren.
        let mut add = |wav: Vec<u8>| sources.add(AudioSource { bytes: wav.into() });

        let effects = Effect::ALL
            .into_iter()
            .map(|e| (e, add(synth::effect(e).to_wav())))
            .collect();
        let music = (1..=dungeon_gen::meta::MAX_DEPTH)
            .map(|depth| add(synth::music(depth).to_wav()))
            .collect();
        info!("Audio synthetisiert in {:?}", start.elapsed());
        Self { effects, music }
    }
}

/// Grundlautstärke je Effekt: Häufige Sounds leiser, wichtige lauter.
/// (Die Samples selbst sind alle auf dieselbe Spitze normalisiert.)
fn base_volume(effect: Effect) -> f32 {
    match effect {
        Effect::Shoot => 0.35,
        Effect::EnemyShoot => 0.3,
        Effect::Hit => 0.55,
        Effect::PlayerHurt => 0.8,
        Effect::EnemyDeath => 0.6,
        Effect::BossDeath | Effect::Explosion => 1.0,
        Effect::Coin => 0.35,
        Effect::Pickup => 0.6,
        Effect::Item => 0.8,
        Effect::Deny => 0.25,
        Effect::RoomClear | Effect::Unlock => 0.6,
        Effect::Descend => 0.7,
        Effect::Click => 0.4,
        Effect::Summon => 0.5,
        Effect::Secret => 0.7,
        Effect::PowerUp => 0.7,
        Effect::Sacrifice => 0.8,
        Effect::Charged => 0.5,
    }
}

/// Leichte Tonhöhen-Variation für Sounds, die oft hintereinander kommen –
/// sonst klingt Dauerfeuer wie ein Maschinengewehr aus einer Schallplatte.
fn varies_pitch(effect: Effect) -> bool {
    matches!(
        effect,
        Effect::Shoot | Effect::EnemyShoot | Effect::Hit | Effect::EnemyDeath
    )
}

/// Marker für laufende Effekte (zum Zählen).
#[derive(Component)]
struct SfxPlayer;

/// Die laufende Musik und für welche Etage sie ist.
#[derive(Component)]
struct MusicTrack {
    depth: u32,
}

/// Manche Effekte hängen direkt an vorhandenen Messages – dafür muss die
/// Spiellogik gar nichts von Audio wissen.
fn sfx_from_game_events(
    mut cleared: MessageReader<RoomCleared>,
    mut descend: MessageReader<Descend>,
    mut sfx: MessageWriter<Sfx>,
) {
    for _ in cleared.read() {
        sfx.write(Sfx(Effect::RoomClear));
    }
    for _ in descend.read() {
        sfx.write(Sfx(Effect::Descend));
    }
}

fn play_sfx(
    mut commands: Commands,
    mut messages: MessageReader<Sfx>,
    bank: Res<SoundBank>,
    settings: Res<AudioSettings>,
    time: Res<Time<Real>>,
    active: Query<(), With<SfxPlayer>>,
    // Wann jeder Effekt zuletzt gestartet wurde.
    mut last_played: Local<BTreeMap<Effect, f64>>,
    mut counter: Local<u32>,
) {
    let now = time.elapsed_secs_f64();
    let mut running = active.iter().count();
    for Sfx(effect) in messages.read() {
        if settings.muted || running >= MAX_ACTIVE_SFX {
            continue;
        }
        // `entry().or_insert` holt den Eintrag oder legt ihn an – ein Lookup statt zwei.
        let last = last_played.entry(*effect).or_insert(f64::NEG_INFINITY);
        if now - *last < MIN_REPEAT_SECS {
            continue;
        }
        *last = now;
        let Some(handle) = bank.effects.get(effect) else {
            continue;
        };

        *counter = counter.wrapping_add(1);
        let speed = if varies_pitch(*effect) {
            let r = crate::pixel_art::canvas::noise(*counter as i32, 0, 0xA0D1) % 1000;
            0.92 + 0.16 * r as f32 / 1000.0
        } else {
            1.0
        };
        commands.spawn((
            Name::new(format!("Sfx {effect:?}")),
            SfxPlayer,
            AudioPlayer::new(handle.clone()),
            // DESPAWN: Entity verschwindet nach dem Abspielen von selbst.
            PlaybackSettings::DESPAWN
                .with_volume(Volume::Linear(base_volume(*effect)))
                .with_speed(speed),
        ));
        running += 1;
    }
}

/// Läuft die Musik der aktuellen Etage? Sonst wechseln.
/// Deckt Run-Start, Fortsetzen und Etagenwechsel mit einer Regel ab.
fn sync_music(
    mut commands: Commands,
    run: Option<Res<Run>>,
    bank: Res<SoundBank>,
    settings: Res<AudioSettings>,
    tracks: Query<(Entity, &MusicTrack)>,
) {
    let Some(run) = run else {
        return;
    };
    let depth = run.floor.depth;
    let mut playing = false;
    for (entity, track) in &tracks {
        if track.depth == depth {
            playing = true;
        } else {
            commands.entity(entity).despawn();
        }
    }
    if playing {
        return;
    }
    let index = (depth as usize).saturating_sub(1).min(bank.music.len() - 1);
    let mut playback = PlaybackSettings::LOOP.with_volume(Volume::Linear(MUSIC_VOLUME));
    if settings.muted {
        playback = playback.muted();
    }
    commands.spawn((
        Name::new(format!("Music Etage {depth}")),
        MusicTrack { depth },
        AudioPlayer::new(bank.music[index].clone()),
        playback,
        // Hauptmenü und Zusammenfassung bleiben still.
        DespawnOnExit(AppState::InGame),
    ));
}

/// `AudioSink` hängt Bevy an, sobald die Wiedergabe läuft. Darüber lässt sich
/// ein laufender Klang steuern.
fn pause_music(sinks: Query<&AudioSink, With<MusicTrack>>) {
    for sink in &sinks {
        sink.pause();
    }
}

fn resume_music(sinks: Query<&AudioSink, With<MusicTrack>>) {
    for sink in &sinks {
        sink.play();
    }
}

fn toggle_mute(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<AudioSettings>,
    mut sinks: Query<&mut AudioSink>,
    mut toast: Option<ResMut<crate::inventory::Toast>>,
) {
    if !keys.just_pressed(KeyCode::KeyM) {
        return;
    }
    settings.muted = !settings.muted;
    for mut sink in &mut sinks {
        if settings.muted {
            sink.mute();
        } else {
            sink.unmute();
        }
    }
    if let Some(toast) = toast.as_mut() {
        toast.show(
            if settings.muted {
                "Ton aus (M)"
            } else {
                "Ton an (M)"
            }
            .to_string(),
        );
    }
}
