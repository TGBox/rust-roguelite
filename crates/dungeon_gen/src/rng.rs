//! Deterministischer Zufall.
//!
//! Eigene Implementierung statt `rand`-Crate:
//! - Die Folge ist garantiert über alle Versionen stabil. Ein Seed erzeugt
//!   für immer dieselbe Etage – wichtig für geteilte Seeds und Replays.
//! - `dungeon_gen` bleibt ohne Dependencies.
//!
//! Verfahren: **xoshiro256\*\*** (schnell, gute Statistik) für die Zahlen,
//! **SplitMix64**, um aus einem einzelnen `u64` den 256-Bit-Zustand zu füllen.
//! Beide stammen von Sebastiano Vigna, siehe <https://prng.di.unimi.it/>.

use std::{fmt, ops::RangeInclusive, str::FromStr};

// --- SplitMix64 ------------------------------------------------------------

/// Einfacher Generator mit 64 Bit Zustand. Hier nur zum Seeden
/// und zum Mischen von Hashwerten.
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

// --- xoshiro256** ----------------------------------------------------------

/// Der Zufallsgenerator, den die Spiellogik benutzt.
///
/// Bewusst **nicht** `Copy`: Ein versehentlich kopierter Generator würde
/// dieselben Zahlen noch einmal liefern – ein typischer, schwer zu findender Bug.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    pub fn from_seed(seed: u64) -> Self {
        let mut sm = SplitMix64::new(seed);
        // Array-Initialisierung mit Closure: ruft `next_u64` viermal nacheinander auf.
        let s = std::array::from_fn(|_| sm.next_u64());
        Self { s }
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Gleichverteilt in `0..n`, ohne Modulo-Verzerrung (Verfahren nach Lemire).
    ///
    /// # Panics
    /// Wenn `n == 0`.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0) ist nicht definiert");
        let mut m = u128::from(self.next_u64()) * u128::from(n);
        let mut low = m as u64;
        if low < n {
            // Schwelle, unterhalb derer ein Ergebnis überrepräsentiert wäre.
            let threshold = n.wrapping_neg() % n;
            while low < threshold {
                m = u128::from(self.next_u64()) * u128::from(n);
                low = m as u64;
            }
        }
        (m >> 64) as u64
    }

    /// Gleichverteilt in einem geschlossenen Intervall, z. B. `rng.range(5..=6)`.
    pub fn range(&mut self, range: RangeInclusive<i32>) -> i32 {
        let (lo, hi) = (*range.start(), *range.end());
        assert!(lo <= hi, "leeres Intervall {lo}..={hi}");
        let span = (i64::from(hi) - i64::from(lo) + 1) as u64;
        (i64::from(lo) + self.below(span) as i64) as i32
    }

    /// Zufälliger Index für eine Liste der Länge `len`.
    pub fn index(&mut self, len: usize) -> usize {
        self.below(len as u64) as usize
    }

    /// Gleichverteilt in `[0, 1)`. Nutzt die oberen 53 Bit (Mantisse von `f64`).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// `true` mit Wahrscheinlichkeit `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Zufälliges Element oder `None` bei leerer Liste.
    ///
    /// Die Lebensdauer `'a` sagt: Die zurückgegebene Referenz lebt so lange
    /// wie die Liste – nicht so lange wie der Generator (`&mut self`).
    pub fn choose<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.index(items.len())])
        }
    }

    /// Index gemäß Gewichten: `[50, 25, 25]` liefert 0 in etwa der Hälfte der Fälle.
    ///
    /// # Panics
    /// Wenn alle Gewichte 0 sind oder die Liste leer ist.
    pub fn weighted_index(&mut self, weights: &[u32]) -> usize {
        let total: u64 = weights.iter().map(|&w| u64::from(w)).sum();
        assert!(
            total > 0,
            "weighted_index braucht mindestens ein Gewicht > 0"
        );
        let mut roll = self.below(total);
        for (i, &w) in weights.iter().enumerate() {
            let w = u64::from(w);
            if roll < w {
                return i;
            }
            roll -= w;
        }
        unreachable!("roll < total ist garantiert")
    }

    /// Fisher-Yates-Mischung.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.index(i + 1);
            items.swap(i, j);
        }
    }
}

// --- RunSeed ---------------------------------------------------------------

/// Der Seed eines Runs. Aus ihm werden unabhängige **Streams** abgeleitet:
///
/// ```
/// # use dungeon_gen::rng::RunSeed;
/// let seed = RunSeed(42);
/// let mut floor_rng = seed.stream("floor", 1);
/// let mut item_rng = seed.stream("items", 0);
/// ```
///
/// Weil jeder Zweck seinen eigenen Generator hat, ändert zusätzlicher Zufall
/// im Kampf nichts an der Etage, die derselbe Seed erzeugt.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RunSeed(pub u64);

impl RunSeed {
    /// Neuer Seed aus der Systemzeit. Nicht kryptografisch – muss es auch nicht sein.
    pub fn from_entropy() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self(SplitMix64::new(nanos).next_u64())
    }

    /// Unabhängiger Generator für einen Zweck (`name`) und optional einen Index
    /// (z. B. die Etagennummer).
    pub fn stream(self, name: &str, index: u64) -> Rng {
        let mut mix = SplitMix64::new(self.0 ^ fnv1a(name.as_bytes()));
        let base = mix.next_u64();
        Rng::from_seed(base ^ SplitMix64::new(index).next_u64())
    }
}

/// FNV-1a: einfacher, stabiler String-Hash. (`std::hash` ist absichtlich
/// nicht über Rust-Versionen hinweg stabil und daher hier ungeeignet.)
const fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xCBF2_9CE4_8422_2325;
    let mut i = 0;
    // `for` ist in `const fn` nicht erlaubt, `while` schon.
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x0100_0000_01B3);
        i += 1;
    }
    hash
}

/// Anzeigeformat: `1A2B-3C4D-5E6F-7A8B`.
impl fmt::Display for RunSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.0;
        write!(
            f,
            "{:04X}-{:04X}-{:04X}-{:04X}",
            (v >> 48) & 0xFFFF,
            (v >> 32) & 0xFFFF,
            (v >> 16) & 0xFFFF,
            v & 0xFFFF
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedParseError;

impl fmt::Display for SeedParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Seed muss aus 16 Hex-Ziffern bestehen (Bindestriche erlaubt)")
    }
}

impl std::error::Error for SeedParseError {}

/// Ermöglicht `"1A2B-3C4D-5E6F-7A8B".parse::<RunSeed>()`.
/// Groß-/Kleinschreibung, Bindestriche und Leerzeichen sind egal.
impl FromStr for RunSeed {
    type Err = SeedParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let digits: String = s.chars().filter(|c| !matches!(c, '-' | ' ')).collect();
        if digits.len() != 16 {
            return Err(SeedParseError);
        }
        u64::from_str_radix(&digits, 16)
            .map(RunSeed)
            .map_err(|_| SeedParseError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_matches_reference_values() {
        // Referenzwerte aus Vignas C-Implementierung, Seed 1234567.
        let mut sm = SplitMix64::new(1_234_567);
        assert_eq!(sm.next_u64(), 6_457_827_717_110_365_317);
        assert_eq!(sm.next_u64(), 3_203_168_211_198_807_973);
    }

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::from_seed(99);
        let mut b = Rng::from_seed(99);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn range_stays_in_bounds_and_hits_every_value() {
        let mut rng = Rng::from_seed(7);
        let mut seen = [false; 7];
        for _ in 0..10_000 {
            let v = rng.range(-3..=3);
            assert!((-3..=3).contains(&v));
            seen[(v + 3) as usize] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn below_is_roughly_uniform() {
        let mut rng = Rng::from_seed(123);
        let mut counts = [0u32; 6];
        let n = 60_000;
        for _ in 0..n {
            counts[rng.below(6) as usize] += 1;
        }
        // Erwartet je 10 000; 5 % Toleranz ist bei dieser Stichprobe sehr großzügig.
        for c in counts {
            assert!(
                (9_500..=10_500).contains(&c),
                "Verteilung schief: {counts:?}"
            );
        }
    }

    #[test]
    fn unit_is_in_half_open_interval() {
        let mut rng = Rng::from_seed(5);
        for _ in 0..10_000 {
            let u = rng.unit();
            assert!((0.0..1.0).contains(&u));
        }
    }

    #[test]
    fn weighted_index_respects_weights() {
        let mut rng = Rng::from_seed(3);
        let mut counts = [0u32; 3];
        for _ in 0..40_000 {
            counts[rng.weighted_index(&[2, 0, 6])] += 1;
        }
        assert_eq!(counts[1], 0, "Gewicht 0 darf nie gewählt werden");
        assert!((9_000..11_000).contains(&counts[0]), "{counts:?}");
        assert!((29_000..31_000).contains(&counts[2]), "{counts:?}");
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut rng = Rng::from_seed(1);
        let mut v: Vec<u32> = (0..50).collect();
        rng.shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(
            v, sorted,
            "50 Elemente sollten praktisch nie unverändert bleiben"
        );
    }

    #[test]
    fn choose_handles_empty_slice() {
        let mut rng = Rng::from_seed(1);
        let empty: [u8; 0] = [];
        assert_eq!(rng.choose(&empty), None);
        assert_eq!(rng.choose(&[42]), Some(&42));
    }

    #[test]
    fn streams_are_independent() {
        let seed = RunSeed(0xDEAD_BEEF);
        let first = |mut r: Rng| r.next_u64();
        let a = first(seed.stream("floor", 1));
        assert_eq!(a, first(seed.stream("floor", 1)), "deterministisch");
        assert_ne!(a, first(seed.stream("floor", 2)), "anderer Index");
        assert_ne!(a, first(seed.stream("items", 1)), "anderer Name");
        assert_ne!(a, first(RunSeed(1).stream("floor", 1)), "anderer Seed");
    }

    #[test]
    fn seed_display_and_parse_roundtrip() {
        let seed = RunSeed(0x1A2B_3C4D_5E6F_7A8B);
        let text = seed.to_string();
        assert_eq!(text, "1A2B-3C4D-5E6F-7A8B");
        assert_eq!(text.parse::<RunSeed>(), Ok(seed));
        assert_eq!("1a2b 3c4d 5e6f 7a8b".parse::<RunSeed>(), Ok(seed));
        assert_eq!("1234".parse::<RunSeed>(), Err(SeedParseError));
        assert_eq!(
            "XYZW-3C4D-5E6F-7A8B".parse::<RunSeed>(),
            Err(SeedParseError)
        );
    }
}
