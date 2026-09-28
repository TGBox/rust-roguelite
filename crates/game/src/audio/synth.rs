//! Ein Mini-Synthesizer: erzeugt alle Soundeffekte und die Musik im Code.
//!
//! Reines Rust ohne Bevy. Ergebnis ist jeweils eine komplette WAV-Datei im
//! Speicher (`Vec<u8>`), die Bevy wie eine geladene Datei abspielt.
//!
//! Bausteine:
//! - **Oszillator**: Sinus, Dreieck, Rechteck, Sägezahn, Rauschen
//! - **Hüllkurve**: kurzer Anstieg (attack), exponentielles Abklingen (decay)
//! - **Tonhöhen-Sweep**: Frequenz gleitet exponentiell von `from` nach `to`
//! - **Tiefpass**: einfacher Ein-Pol-Filter, macht Rauschen „dumpfer“
//!
//! Alles ist deterministisch: gleicher Code → gleiche Samples, bei jedem Start.

use std::f32::consts::TAU;

/// 22 050 Hz, Mono, 16 Bit reicht für Retro-Sounds völlig und hält den
/// Speicher klein (eine Sekunde ≈ 43 KB).
pub const SAMPLE_RATE: u32 = 22_050;

/// Kurvenform eines Oszillators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Triangle,
    Square,
    Saw,
    Noise,
}

/// Einfacher Pseudozufall (xorshift32) für Rauschen – deterministisch.
#[derive(Debug, Clone)]
pub struct NoiseGen(u32);

impl NoiseGen {
    pub fn new(seed: u32) -> Self {
        // 0 ist für xorshift ein Fixpunkt (bliebe für immer 0).
        Self(seed.max(1))
    }

    /// Nächster Wert im Bereich -1.0 … 1.0.
    pub fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Wert einer periodischen Kurve bei `phase` (0.0 … 1.0 = eine Schwingung).
fn oscillate(wave: Wave, phase: f32, noise: &mut NoiseGen) -> f32 {
    match wave {
        Wave::Sine => (phase * TAU).sin(),
        Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
        // Kein ganz symmetrisches Rechteck (Pulsbreite 40 %): klingt runder.
        Wave::Square => {
            if phase < 0.4 {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Saw => 2.0 * phase - 1.0,
        Wave::Noise => noise.next_f32(),
    }
}

/// Beschreibung eines einzelnen Tons. Mit `..Tone::default()` gibt man nur an,
/// was vom Standard abweicht – wie ein Builder, aber ohne Extra-Code.
#[derive(Debug, Clone, Copy)]
pub struct Tone {
    pub wave: Wave,
    /// Startfrequenz (Hz).
    pub from: f32,
    /// Endfrequenz (Hz); gleich `from` für einen festen Ton.
    pub to: f32,
    /// Dauer (s).
    pub duration: f32,
    /// Lautstärke 0.0 … 1.0.
    pub volume: f32,
    /// Anstiegszeit (s). Ein paar Millisekunden verhindern Knacken.
    pub attack: f32,
    /// Wie schnell der Ton abklingt (1/s). 0 = gleichbleibend bis zum Ende.
    pub decay: f32,
    /// Tiefpass 0.0 … 1.0 (1.0 = aus). Kleiner = dumpfer.
    pub lowpass: f32,
    /// Vibrato-Tiefe (Anteil der Frequenz) und -Geschwindigkeit (Hz).
    pub vibrato: (f32, f32),
}

impl Default for Tone {
    fn default() -> Self {
        Self {
            wave: Wave::Square,
            from: 440.0,
            to: 440.0,
            duration: 0.1,
            volume: 0.5,
            attack: 0.003,
            decay: 0.0,
            lowpass: 1.0,
            vibrato: (0.0, 0.0),
        }
    }
}

/// Ein Puffer aus Samples, in den Töne hineingemischt werden.
#[derive(Debug, Clone, Default)]
pub struct Track {
    pub samples: Vec<f32>,
}

impl Track {
    pub fn with_seconds(seconds: f32) -> Self {
        Self {
            samples: vec![0.0; secs_to_samples(seconds)],
        }
    }

    /// Mischt `tone` ab Sekunde `at` hinzu. Der Puffer wächst bei Bedarf.
    pub fn add(&mut self, at: f32, tone: Tone, noise: &mut NoiseGen) {
        let start = secs_to_samples(at);
        let len = secs_to_samples(tone.duration);
        if self.samples.len() < start + len {
            self.samples.resize(start + len, 0.0);
        }
        let dt = 1.0 / SAMPLE_RATE as f32;
        let mut phase = 0.0_f32;
        let mut filtered = 0.0_f32;
        // Ein kurzes Ausblenden am Ende (5 ms) verhindert ein Knacken,
        // falls der Ton abrupt abgeschnitten wird.
        let release = secs_to_samples(0.005).min(len / 2).max(1);
        for i in 0..len {
            let t = i as f32 * dt;
            let progress = i as f32 / len as f32;
            // Exponentieller Sweep: klingt für das Ohr gleichmäßig
            // (Tonhöhe wird logarithmisch wahrgenommen).
            let mut freq = tone.from * (tone.to / tone.from).powf(progress);
            if tone.vibrato.0 > 0.0 {
                freq *= 1.0 + tone.vibrato.0 * (t * tone.vibrato.1 * TAU).sin();
            }
            phase = (phase + freq * dt).fract();

            let raw = oscillate(tone.wave, phase, noise);
            filtered += tone.lowpass * (raw - filtered);

            let attack = if tone.attack > 0.0 {
                (t / tone.attack).min(1.0)
            } else {
                1.0
            };
            let decay = (-tone.decay * t).exp();
            let tail = ((len - i) as f32 / release as f32).min(1.0);
            self.samples[start + i] += filtered * tone.volume * attack * decay * tail;
        }
    }

    /// Lauteste Stelle auf `peak` bringen (verhindert Übersteuern).
    pub fn normalize(&mut self, peak: f32) {
        let max = self.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        if max > 0.0 {
            let gain = peak / max;
            self.samples.iter_mut().for_each(|s| *s *= gain);
        }
    }

    pub fn duration(&self) -> f32 {
        self.samples.len() as f32 / SAMPLE_RATE as f32
    }

    pub fn to_wav(&self) -> Vec<u8> {
        wav_bytes(&self.samples)
    }
}

fn secs_to_samples(seconds: f32) -> usize {
    (seconds.max(0.0) * SAMPLE_RATE as f32).round() as usize
}

/// Frequenz einer Note: MIDI-Nummer 69 = A4 = 440 Hz, +12 = eine Oktave höher.
pub fn midi(note: i32) -> f32 {
    440.0 * 2f32.powf((note - 69) as f32 / 12.0)
}

/// Samples → WAV-Datei (RIFF-Header + 16-Bit-PCM, Mono).
///
/// Das Format ist simpel: ein paar feste Header-Felder, dann die Samples
/// als little-endian `i16`. `extend_from_slice(&x.to_le_bytes())` schreibt
/// eine Zahl Byte für Byte in der richtigen Reihenfolge.
pub fn wav_bytes(samples: &[f32]) -> Vec<u8> {
    const BITS: u16 = 16;
    const CHANNELS: u16 = 1;
    let block_align = CHANNELS * BITS / 8;
    let byte_rate = SAMPLE_RATE * block_align as u32;
    let data_len = (samples.len() * block_align as usize) as u32;

    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    // „fmt “-Block: Format 1 = PCM.
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());
    // „data“-Block: die eigentlichen Samples.
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

// --- Soundeffekte -----------------------------------------------------------------

/// Alle Soundeffekte des Spiels. `ALL` erlaubt es, in einer Schleife alle
/// zu erzeugen (siehe `audio.rs`) und in Tests alle zu prüfen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Effect {
    Shoot,
    EnemyShoot,
    Hit,
    PlayerHurt,
    EnemyDeath,
    BossDeath,
    Explosion,
    Coin,
    Pickup,
    Item,
    Deny,
    RoomClear,
    Unlock,
    Descend,
    Click,
    /// Beschwörer/Brutmutter ruft Nachwuchs.
    Summon,
    /// Geheimraum freigesprengt.
    Secret,
    /// Aktives Item benutzt.
    PowerUp,
    /// Opfer am Altar.
    Sacrifice,
    /// Aktives Item ist voll aufgeladen.
    Charged,
}

impl Effect {
    pub const ALL: [Effect; 20] = [
        Effect::Shoot,
        Effect::EnemyShoot,
        Effect::Hit,
        Effect::PlayerHurt,
        Effect::EnemyDeath,
        Effect::BossDeath,
        Effect::Explosion,
        Effect::Coin,
        Effect::Pickup,
        Effect::Item,
        Effect::Deny,
        Effect::RoomClear,
        Effect::Unlock,
        Effect::Descend,
        Effect::Click,
        Effect::Summon,
        Effect::Secret,
        Effect::PowerUp,
        Effect::Sacrifice,
        Effect::Charged,
    ];
}

/// Erzeugt einen Soundeffekt. Jeder Effekt bekommt einen eigenen Rausch-Seed,
/// damit sich z. B. Treffer und Explosion nicht „gleich rauschen“.
pub fn effect(effect: Effect) -> Track {
    let mut n = NoiseGen::new(effect as u32 * 7919 + 1);
    let mut t = Track::default();
    let n = &mut n;
    match effect {
        Effect::Shoot => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Triangle,
                    from: 900.0,
                    to: 420.0,
                    duration: 0.09,
                    decay: 18.0,
                    volume: 0.8,
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.0,
                Tone {
                    wave: Wave::Noise,
                    duration: 0.03,
                    decay: 60.0,
                    volume: 0.15,
                    lowpass: 0.4,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::EnemyShoot => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Square,
                    from: 340.0,
                    to: 170.0,
                    duration: 0.12,
                    decay: 16.0,
                    volume: 0.5,
                    lowpass: 0.35,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::Hit => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Noise,
                    duration: 0.07,
                    decay: 40.0,
                    volume: 0.6,
                    lowpass: 0.5,
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.0,
                Tone {
                    wave: Wave::Sine,
                    from: 200.0,
                    to: 60.0,
                    duration: 0.1,
                    decay: 20.0,
                    volume: 0.8,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::PlayerHurt => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Square,
                    from: 520.0,
                    to: 120.0,
                    duration: 0.32,
                    decay: 6.0,
                    volume: 0.6,
                    lowpass: 0.3,
                    vibrato: (0.06, 30.0),
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.0,
                Tone {
                    wave: Wave::Noise,
                    duration: 0.12,
                    decay: 25.0,
                    volume: 0.4,
                    lowpass: 0.3,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::EnemyDeath => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Noise,
                    duration: 0.28,
                    decay: 12.0,
                    volume: 0.6,
                    lowpass: 0.25,
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.0,
                Tone {
                    wave: Wave::Square,
                    from: 320.0,
                    to: 55.0,
                    duration: 0.22,
                    decay: 10.0,
                    volume: 0.45,
                    lowpass: 0.3,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::BossDeath => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Noise,
                    duration: 1.4,
                    decay: 2.5,
                    volume: 0.8,
                    lowpass: 0.12,
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.0,
                Tone {
                    wave: Wave::Saw,
                    from: 220.0,
                    to: 30.0,
                    duration: 1.2,
                    decay: 2.0,
                    volume: 0.5,
                    lowpass: 0.2,
                    vibrato: (0.08, 9.0),
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.3,
                Tone {
                    wave: Wave::Sine,
                    from: 90.0,
                    to: 30.0,
                    duration: 0.8,
                    decay: 3.0,
                    volume: 0.9,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::Explosion => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Noise,
                    duration: 0.75,
                    decay: 5.0,
                    volume: 0.9,
                    lowpass: 0.15,
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.0,
                Tone {
                    wave: Wave::Sine,
                    from: 110.0,
                    to: 32.0,
                    duration: 0.5,
                    decay: 5.0,
                    volume: 1.0,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::Coin => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Square,
                    from: midi(83),
                    to: midi(83),
                    duration: 0.06,
                    volume: 0.35,
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.06,
                Tone {
                    wave: Wave::Square,
                    from: midi(88),
                    to: midi(88),
                    duration: 0.22,
                    decay: 10.0,
                    volume: 0.35,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::Pickup => {
            for (i, note) in [72, 76, 79].into_iter().enumerate() {
                t.add(
                    i as f32 * 0.06,
                    Tone {
                        wave: Wave::Triangle,
                        from: midi(note),
                        to: midi(note),
                        duration: 0.12,
                        decay: 14.0,
                        volume: 0.6,
                        ..Tone::default()
                    },
                    n,
                );
            }
        }
        Effect::Item => {
            for (i, note) in [72, 76, 79, 84, 88].into_iter().enumerate() {
                let at = i as f32 * 0.08;
                t.add(
                    at,
                    Tone {
                        wave: Wave::Triangle,
                        from: midi(note),
                        to: midi(note),
                        duration: 0.5,
                        decay: 5.0,
                        volume: 0.5,
                        ..Tone::default()
                    },
                    n,
                );
                t.add(
                    at,
                    Tone {
                        wave: Wave::Sine,
                        from: midi(note + 12),
                        to: midi(note + 12),
                        duration: 0.35,
                        decay: 7.0,
                        volume: 0.2,
                        ..Tone::default()
                    },
                    n,
                );
            }
        }
        Effect::Deny => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Square,
                    from: 130.0,
                    to: 120.0,
                    duration: 0.18,
                    volume: 0.35,
                    lowpass: 0.3,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::RoomClear => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Sine,
                    from: midi(79),
                    to: midi(79),
                    duration: 0.6,
                    decay: 5.0,
                    volume: 0.6,
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.12,
                Tone {
                    wave: Wave::Sine,
                    from: midi(84),
                    to: midi(84),
                    duration: 0.9,
                    decay: 3.5,
                    volume: 0.6,
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.12,
                Tone {
                    wave: Wave::Triangle,
                    from: midi(72),
                    to: midi(72),
                    duration: 0.9,
                    decay: 3.5,
                    volume: 0.3,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::Unlock => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Noise,
                    duration: 0.03,
                    decay: 80.0,
                    volume: 0.5,
                    lowpass: 0.6,
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.04,
                Tone {
                    wave: Wave::Saw,
                    from: 180.0,
                    to: 420.0,
                    duration: 0.2,
                    decay: 8.0,
                    volume: 0.4,
                    lowpass: 0.3,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::Descend => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Sine,
                    from: 700.0,
                    to: 70.0,
                    duration: 1.0,
                    decay: 1.5,
                    volume: 0.7,
                    vibrato: (0.03, 7.0),
                    ..Tone::default()
                },
                n,
            );
            t.add(
                0.0,
                Tone {
                    wave: Wave::Noise,
                    duration: 1.0,
                    decay: 2.5,
                    volume: 0.25,
                    lowpass: 0.08,
                    ..Tone::default()
                },
                n,
            );
        }
        Effect::Summon => {
            let tone = Tone {
                wave: Wave::Saw,
                from: 160.0,
                to: 520.0,
                duration: 0.35,
                decay: 4.0,
                volume: 0.4,
                lowpass: 0.25,
                vibrato: (0.05, 14.0),
                ..Tone::default()
            };
            t.add(0.0, tone, n);
            t.add(
                0.05,
                Tone {
                    from: 240.0,
                    to: 780.0,
                    volume: 0.25,
                    ..tone
                },
                n,
            );
        }
        Effect::Secret => {
            // Kleine Fanfare: aufsteigende Dur-Töne, der letzte lang.
            for (i, note) in [67, 71, 74, 79].into_iter().enumerate() {
                let last = i == 3;
                let tone = Tone {
                    wave: Wave::Square,
                    from: midi(note),
                    to: midi(note),
                    duration: if last { 0.6 } else { 0.1 },
                    decay: if last { 4.0 } else { 0.0 },
                    volume: 0.3,
                    lowpass: 0.5,
                    ..Tone::default()
                };
                t.add(i as f32 * 0.1, tone, n);
            }
        }
        Effect::PowerUp => {
            let tone = Tone {
                wave: Wave::Triangle,
                from: 300.0,
                to: 1200.0,
                duration: 0.3,
                decay: 3.0,
                volume: 0.6,
                ..Tone::default()
            };
            t.add(0.0, tone, n);
            t.add(
                0.0,
                Tone {
                    wave: Wave::Noise,
                    duration: 0.3,
                    decay: 8.0,
                    volume: 0.2,
                    lowpass: 0.2,
                    ..tone
                },
                n,
            );
        }
        Effect::Sacrifice => {
            let tone = Tone {
                wave: Wave::Sine,
                from: midi(45),
                to: midi(45),
                duration: 0.9,
                decay: 3.0,
                volume: 0.7,
                vibrato: (0.02, 5.0),
                ..Tone::default()
            };
            t.add(0.0, tone, n);
            t.add(
                0.0,
                Tone {
                    from: midi(52),
                    to: midi(52),
                    volume: 0.4,
                    ..tone
                },
                n,
            );
            t.add(
                0.0,
                Tone {
                    wave: Wave::Noise,
                    duration: 0.15,
                    decay: 20.0,
                    volume: 0.4,
                    lowpass: 0.3,
                    ..tone
                },
                n,
            );
        }
        Effect::Charged => {
            for (i, note) in [84, 91].into_iter().enumerate() {
                let tone = Tone {
                    wave: Wave::Sine,
                    from: midi(note),
                    to: midi(note),
                    duration: 0.25,
                    decay: 10.0,
                    volume: 0.5,
                    ..Tone::default()
                };
                t.add(i as f32 * 0.07, tone, n);
            }
        }
        Effect::Click => {
            t.add(
                0.0,
                Tone {
                    wave: Wave::Sine,
                    from: 1300.0,
                    to: 900.0,
                    duration: 0.035,
                    decay: 50.0,
                    volume: 0.5,
                    ..Tone::default()
                },
                n,
            );
        }
    }
    t.normalize(0.9);
    t
}

// --- Musik ------------------------------------------------------------------------------

/// Parameter eines Musikstücks pro Etage.
struct Song {
    bpm: f32,
    /// Grundton (MIDI-Nummer) der Tonart.
    root: i32,
    /// Akkordfolge als Stufen relativ zum Grundton (in Halbtönen).
    progression: [i32; 4],
    seed: u32,
}

fn song_for_depth(depth: u32) -> Song {
    match depth {
        // a-Moll: i – VI – III – VII (A, F, C, G) – klassisch „Abenteuer“.
        1 => Song {
            bpm: 100.0,
            root: 57,
            progression: [0, -4, 3, -2],
            seed: 1,
        },
        // d-Moll, langsamer und dunkler: i – iv – VI – V.
        2 => Song {
            bpm: 92.0,
            root: 50,
            progression: [0, 5, -4, -5],
            seed: 2,
        },
        // e-Moll, schneller und drängender: i – VII – VI – VII.
        _ => Song {
            bpm: 116.0,
            root: 52,
            progression: [0, -2, -4, -2],
            seed: 3,
        },
    }
}

/// Erzeugt die Musikschleife für eine Etage: 8 Takte, nahtlos wiederholbar.
///
/// Vier Stimmen: Bass (Achtel), Arpeggio (Sechzehntel), Kick auf jedem
/// Schlag, Hi-Hat auf den Achtel-Offbeats.
pub fn music(depth: u32) -> Track {
    let song = song_for_depth(depth);
    let beat = 60.0 / song.bpm;
    let bars = 8;
    // Puffer exakt so lang wie die Schleife: Ausklingende Töne am Ende werden
    // abgeschnitten, damit die Wiederholung genau im Takt bleibt.
    let length = bars as f32 * 4.0 * beat;
    let mut t = Track::with_seconds(length);
    let mut n = NoiseGen::new(song.seed);
    let n = &mut n;

    for bar in 0..bars {
        let chord_root = song.root + song.progression[bar % 4];
        // Moll-Dreiklang auf der ersten Stufe, sonst Dur – vereinfacht, klingt aber passend.
        let third = if song.progression[bar % 4] == 0 { 3 } else { 4 };
        let chord = [0, third, 7, 12];
        let bar_start = bar as f32 * 4.0 * beat;

        for eighth in 0..8 {
            let at = bar_start + eighth as f32 * beat / 2.0;
            // Bass: Grundton, jede vierte Achtel eine Oktave höher.
            let note = chord_root - 12 + if eighth % 4 == 3 { 12 } else { 0 };
            t.add(
                at,
                Tone {
                    wave: Wave::Triangle,
                    from: midi(note),
                    to: midi(note),
                    duration: beat / 2.0 * 0.9,
                    decay: 3.0,
                    volume: 0.55,
                    ..Tone::default()
                },
                n,
            );
            // Hi-Hat auf den Offbeats.
            if eighth % 2 == 1 {
                t.add(
                    at,
                    Tone {
                        wave: Wave::Noise,
                        duration: 0.04,
                        decay: 70.0,
                        volume: 0.12,
                        lowpass: 0.9,
                        ..Tone::default()
                    },
                    n,
                );
            }
        }
        for beat_i in 0..4 {
            let at = bar_start + beat_i as f32 * beat;
            t.add(
                at,
                Tone {
                    wave: Wave::Sine,
                    from: 120.0,
                    to: 40.0,
                    duration: 0.14,
                    decay: 18.0,
                    volume: 0.6,
                    ..Tone::default()
                },
                n,
            );
        }
        // Arpeggio erst ab Takt 3 – so baut sich das Stück auf.
        if bar >= 2 {
            for step in 0..16 {
                let at = bar_start + step as f32 * beat / 4.0;
                let pattern = [0, 1, 2, 3, 2, 1, 2, 3];
                let note = chord_root + 12 + chord[pattern[step % 8]];
                t.add(
                    at,
                    Tone {
                        wave: Wave::Square,
                        from: midi(note),
                        to: midi(note),
                        duration: beat / 4.0 * 0.8,
                        decay: 9.0,
                        volume: 0.13,
                        lowpass: 0.35,
                        ..Tone::default()
                    },
                    n,
                );
            }
        }
    }
    t.samples.truncate(secs_to_samples(length));
    t.normalize(0.7);
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid() {
        let wav = wav_bytes(&[0.0, 0.5, -0.5, 1.0]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + 4 * 2);
        // Größenfeld = Dateigröße - 8.
        let riff_len = u32::from_le_bytes(wav[4..8].try_into().unwrap());
        assert_eq!(riff_len as usize, wav.len() - 8);
        // 1.0 → i16::MAX.
        assert_eq!(
            i16::from_le_bytes(wav[50..52].try_into().unwrap()),
            i16::MAX
        );
    }

    #[test]
    fn midi_a4_is_440() {
        assert!((midi(69) - 440.0).abs() < 1e-3);
        assert!((midi(81) - 880.0).abs() < 1e-2);
    }

    #[test]
    fn all_effects_are_short_audible_and_not_clipping() {
        for e in Effect::ALL {
            let track = effect(e);
            let peak = track.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(track.duration() > 0.02 && track.duration() < 2.0, "{e:?}");
            assert!(peak > 0.5 && peak <= 0.9 + 1e-4, "{e:?}: Spitze {peak}");
            assert!(track.samples.iter().all(|s| s.is_finite()), "{e:?}");
        }
    }

    #[test]
    fn music_loops_have_exact_length() {
        for depth in 1..=3 {
            let song = song_for_depth(depth);
            let expected = 8.0 * 4.0 * 60.0 / song.bpm;
            let track = music(depth);
            assert!((track.duration() - expected).abs() < 0.001, "Etage {depth}");
            // Der Loop beginnt und endet leise genug, um nicht zu knacken.
            assert!(track.samples.last().unwrap().abs() < 0.3);
        }
    }

    #[test]
    fn synthesis_is_deterministic() {
        assert_eq!(
            effect(Effect::Explosion).samples,
            effect(Effect::Explosion).samples
        );
    }
}
