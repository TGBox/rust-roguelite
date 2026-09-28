//! Spielerwerte und ihre Veränderung durch Items.
//!
//! Regel: **Erst alle Additionen, dann alle Multiplikationen**, danach
//! Untergrenzen. Dadurch ist die Reihenfolge der Items egal – ein Item, das
//! man zuerst aufhebt, wirkt genauso wie eines, das man zuletzt aufhebt.

/// Welcher Wert verändert wird.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stat {
    /// Schaden pro Schuss.
    Damage,
    /// Schüsse pro Sekunde.
    FireRate,
    /// Schussgeschwindigkeit (Kacheln/s).
    ShotSpeed,
    /// Reichweite (Kacheln).
    Range,
    /// Laufgeschwindigkeit (Kacheln/s).
    MoveSpeed,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Add(f32),
    Mul(f32),
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modifier {
    pub stat: Stat,
    pub op: Op,
}

impl Modifier {
    pub const fn add(stat: Stat, value: f32) -> Self {
        Self {
            stat,
            op: Op::Add(value),
        }
    }

    pub const fn mul(stat: Stat, value: f32) -> Self {
        Self {
            stat,
            op: Op::Mul(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub damage: f32,
    pub fire_rate: f32,
    pub shot_speed: f32,
    pub range: f32,
    pub move_speed: f32,
}

/// Untergrenzen, damit kein Item-Mix das Spiel unspielbar macht.
const MIN: Stats = Stats {
    damage: 0.5,
    fire_rate: 0.8,
    shot_speed: 3.0,
    range: 2.0,
    move_speed: 2.0,
};

impl Stats {
    /// Startwerte eines neuen Runs.
    pub const BASE: Stats = Stats {
        damage: 2.0,
        fire_rate: 2.85,
        shot_speed: 9.0,
        range: 6.5,
        move_speed: 5.0,
    };

    pub fn get(&self, stat: Stat) -> f32 {
        match stat {
            Stat::Damage => self.damage,
            Stat::FireRate => self.fire_rate,
            Stat::ShotSpeed => self.shot_speed,
            Stat::Range => self.range,
            Stat::MoveSpeed => self.move_speed,
        }
    }

    /// `&mut f32` auf ein Feld: So kann eine Funktion jeden Wert verändern,
    /// ohne fünfmal denselben `match` zu schreiben.
    fn get_mut(&mut self, stat: Stat) -> &mut f32 {
        match stat {
            Stat::Damage => &mut self.damage,
            Stat::FireRate => &mut self.fire_rate,
            Stat::ShotSpeed => &mut self.shot_speed,
            Stat::Range => &mut self.range,
            Stat::MoveSpeed => &mut self.move_speed,
        }
    }

    /// Wendet Modifier auf `base` an – unabhängig von ihrer Reihenfolge.
    pub fn with_modifiers(base: Stats, modifiers: impl IntoIterator<Item = Modifier>) -> Stats {
        let modifiers: Vec<Modifier> = modifiers.into_iter().collect();
        let mut out = base;
        for m in &modifiers {
            if let Op::Add(v) = m.op {
                *out.get_mut(m.stat) += v;
            }
        }
        for m in &modifiers {
            if let Op::Mul(v) = m.op {
                *out.get_mut(m.stat) *= v;
            }
        }
        for stat in ALL_STATS {
            let min = MIN.get(stat);
            let v = out.get_mut(stat);
            *v = v.max(min);
        }
        out
    }

    /// Pause zwischen zwei Schüssen (s).
    pub fn fire_delay(&self) -> f32 {
        1.0 / self.fire_rate
    }
}

pub const ALL_STATS: [Stat; 5] = [
    Stat::Damage,
    Stat::FireRate,
    Stat::ShotSpeed,
    Stat::Range,
    Stat::MoveSpeed,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn no_modifiers_keeps_base() {
        assert_eq!(Stats::with_modifiers(Stats::BASE, []), Stats::BASE);
    }

    #[test]
    fn adds_before_multiplies() {
        let s = Stats::with_modifiers(
            Stats::BASE,
            [
                Modifier::mul(Stat::Damage, 2.0),
                Modifier::add(Stat::Damage, 1.0),
            ],
        );
        // (2 + 1) * 2 = 6, nicht 2 * 2 + 1 = 5.
        assert!(approx(s.damage, 6.0), "{s:?}");
    }

    #[test]
    fn order_does_not_matter() {
        let mods = vec![
            Modifier::add(Stat::Damage, 1.0),
            Modifier::mul(Stat::Damage, 1.5),
            Modifier::mul(Stat::FireRate, 0.7),
            Modifier::add(Stat::FireRate, 0.5),
            Modifier::add(Stat::Range, -1.0),
            Modifier::mul(Stat::MoveSpeed, 1.2),
        ];
        let expected = Stats::with_modifiers(Stats::BASE, mods.clone());
        let mut rng = Rng::from_seed(1);
        for _ in 0..50 {
            let mut shuffled = mods.clone();
            rng.shuffle(&mut shuffled);
            let got = Stats::with_modifiers(Stats::BASE, shuffled);
            for stat in ALL_STATS {
                assert!(approx(got.get(stat), expected.get(stat)), "{stat:?}");
            }
        }
    }

    #[test]
    fn minimums_are_enforced() {
        let s = Stats::with_modifiers(
            Stats::BASE,
            [
                Modifier::mul(Stat::FireRate, 0.01),
                Modifier::add(Stat::Damage, -100.0),
            ],
        );
        assert!(approx(s.fire_rate, MIN.fire_rate));
        assert!(approx(s.damage, MIN.damage));
    }

    #[test]
    fn fire_delay_is_inverse_of_rate() {
        assert!(approx(
            Stats::BASE.fire_delay() * Stats::BASE.fire_rate,
            1.0
        ));
    }
}
