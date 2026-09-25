//! Kollision von Rechtecken (AABB) gegen ein Kachelraster.
//!
//! Alles rechnet im **Kachelraum**: Kachel `(i, j)` belegt das Intervall
//! `[i, i+1) × [j, j+1)`. Die Spiel-Crate rechnet Weltkoordinaten vorher um.
//!
//! Verfahren: achsengetrennte Bewegung. Erst X bewegen und auflösen, dann Y.
//! Dadurch „rutscht“ man an Wänden entlang, statt hängen zu bleiben. Große
//! Bewegungen werden in Teilschritte zerlegt, damit schnelle Objekte nicht
//! durch dünne Wände tunneln.
//!
//! Positionen sind `[f32; 2]` statt eines Vektortyps, damit diese Crate
//! ohne Mathe-Dependency auskommt. `glam::Vec2` konvertiert per
//! `to_array()` / `Vec2::from_array()` verlustfrei.

use crate::GridPos;

/// Toleranz, damit ein Körper, der exakt an einer Kante anliegt,
/// nicht als überlappend gilt.
pub const EPSILON: f32 = 1e-4;

/// Maximale Teilschritt-Länge in Kacheln (kleiner als jede Hitbox-Hälfte).
const MAX_STEP: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveResult {
    pub center: [f32; 2],
    pub hit_x: bool,
    pub hit_y: bool,
}

impl MoveResult {
    pub fn hit_any(&self) -> bool {
        self.hit_x || self.hit_y
    }
}

/// Bewegt ein Rechteck (`center`, `half` = halbe Kantenlängen) um `delta`
/// und schiebt es aus allen Kacheln heraus, für die `is_solid` `true` liefert.
///
/// `is_solid` ist ein generischer Parameter statt eines konkreten Raumtyps:
/// So entscheidet der Aufrufer, was blockiert (Grube für Läufer ja, für
/// Schüsse nein), und Tests können einfache Closures übergeben.
pub fn move_and_slide(
    center: [f32; 2],
    half: [f32; 2],
    delta: [f32; 2],
    is_solid: impl Fn(GridPos) -> bool,
) -> MoveResult {
    let longest = delta[0].abs().max(delta[1].abs());
    let steps = ((longest / MAX_STEP).ceil() as u32).max(1);
    let step = [delta[0] / steps as f32, delta[1] / steps as f32];

    let mut result = MoveResult {
        center,
        hit_x: false,
        hit_y: false,
    };
    for _ in 0..steps {
        if !result.hit_x {
            let (c, hit) = sweep_axis(result.center, half, step[0], 0, &is_solid);
            result.center = c;
            result.hit_x = hit;
        }
        if !result.hit_y {
            let (c, hit) = sweep_axis(result.center, half, step[1], 1, &is_solid);
            result.center = c;
            result.hit_y = hit;
        }
    }
    result
}

/// Bewegt entlang einer Achse (0 = x, 1 = y) und löst Überlappungen auf.
fn sweep_axis(
    mut center: [f32; 2],
    half: [f32; 2],
    delta: f32,
    axis: usize,
    is_solid: &impl Fn(GridPos) -> bool,
) -> ([f32; 2], bool) {
    if delta == 0.0 {
        return (center, false);
    }
    center[axis] += delta;

    let other = 1 - axis;
    let (o_min, o_max) = tile_span(center[other], half[other]);
    let (a_min, a_max) = tile_span(center[axis], half[axis]);

    let pos = |a: i32, o: i32| {
        if axis == 0 {
            GridPos::new(a, o)
        } else {
            GridPos::new(o, a)
        }
    };
    // Ist irgendeine Kachel dieser Spalte/Zeile (quer zur Bewegung) solide?
    let line_blocked = |a: i32| (o_min..=o_max).any(|o| is_solid(pos(a, o)));

    if delta > 0.0 {
        // Aufsteigend suchen: die erste blockierte Linie ist die nächstgelegene.
        if let Some(a) = (a_min..=a_max).find(|&a| line_blocked(a)) {
            center[axis] = a as f32 - half[axis];
            return (center, true);
        }
    } else if let Some(a) = (a_min..=a_max).rev().find(|&a| line_blocked(a)) {
        center[axis] = (a + 1) as f32 + half[axis];
        return (center, true);
    }
    (center, false)
}

/// Welche Kachelindizes überdeckt das Intervall `[center - half, center + half]`?
fn tile_span(center: f32, half: f32) -> (i32, i32) {
    (
        (center - half + EPSILON).floor() as i32,
        (center + half - EPSILON).floor() as i32,
    )
}

/// Einstellungen für die Ecken-Korrektur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nudge {
    /// Größter seitlicher Versatz, nach dem gesucht wird (Kacheln).
    pub max_offset: f32,
    /// Größter tatsächlicher Versatz pro Aufruf (Kacheln) – sorgt für ein
    /// sanftes Hineingleiten statt eines Sprungs.
    pub max_step: f32,
}

/// Wie [`move_and_slide`], aber mit **Ecken-Korrektur** („corner correction“):
///
/// Wird die Bewegung auf einer Achse blockiert, sucht die Funktion quer dazu
/// den kleinsten Versatz (bis `nudge.max_offset`), bei dem der Weg frei wäre –
/// typischerweise eine Türlücke oder die Kante eines Felsens. Dorthin wird der
/// Körper um höchstens `nudge.max_step` geschoben. Über mehrere Ticks gleitet
/// er so in die Lücke, statt an der Ecke hängen zu bleiben.
///
/// Gegen die Eingabe wird nie geschoben: Bewegt sich der Körper auf der
/// Querachse selbst, muss der Versatz in dieselbe Richtung zeigen.
pub fn move_and_slide_nudged(
    center: [f32; 2],
    half: [f32; 2],
    delta: [f32; 2],
    nudge: Nudge,
    is_solid: impl Fn(GridPos) -> bool,
) -> MoveResult {
    let mut result = move_and_slide(center, half, delta, &is_solid);

    for axis in 0..2 {
        let blocked = if axis == 0 {
            result.hit_x
        } else {
            result.hit_y
        };
        if !blocked || delta[axis] == 0.0 {
            continue;
        }
        let other = 1 - axis;
        // Kleiner Test-Schritt in die blockierte Richtung.
        let probe = delta[axis].signum() * PROBE;
        let Some(offset) = find_nudge(
            result.center,
            half,
            probe,
            axis,
            nudge.max_offset,
            &is_solid,
        ) else {
            continue;
        };
        if delta[other] != 0.0 && delta[other].signum() != offset.signum() {
            continue;
        }
        let step = offset.signum() * offset.abs().min(nudge.max_step);
        let (c, _) = sweep_axis(result.center, half, step, other, &is_solid);
        result.center = c;
    }
    result
}

/// Länge des Test-Schritts in die Wand hinein (Kacheln).
const PROBE: f32 = 0.05;
/// Suchraster für den Versatz (Kacheln).
const NUDGE_SEARCH_STEP: f32 = 0.02;

/// Kleinster Versatz quer zu `axis`, bei dem der Körper frei steht **und**
/// sich um `probe` weiter in Bewegungsrichtung schieben ließe.
fn find_nudge(
    center: [f32; 2],
    half: [f32; 2],
    probe: f32,
    axis: usize,
    max_offset: f32,
    is_solid: &impl Fn(GridPos) -> bool,
) -> Option<f32> {
    let other = 1 - axis;
    let steps = (max_offset / NUDGE_SEARCH_STEP).round() as u32;
    for i in 1..=steps {
        let k = i as f32 * NUDGE_SEARCH_STEP;
        for sign in [1.0, -1.0] {
            let mut shifted = center;
            shifted[other] += sign * k;
            if overlaps_solid(shifted, half, is_solid) {
                continue;
            }
            let mut probed = shifted;
            probed[axis] += probe;
            if !overlaps_solid(probed, half, is_solid) {
                return Some(sign * k);
            }
        }
    }
    None
}

/// Überlappt das Rechteck irgendeine solide Kachel?
fn overlaps_solid(center: [f32; 2], half: [f32; 2], is_solid: &impl Fn(GridPos) -> bool) -> bool {
    let (x_min, x_max) = tile_span(center[0], half[0]);
    let (y_min, y_max) = tile_span(center[1], half[1]);
    (x_min..=x_max).any(|x| (y_min..=y_max).any(|y| is_solid(GridPos::new(x, y))))
}

/// Überlappen sich zwei Rechtecke? Exakt anliegende Kanten zählen nicht.
/// Einheiten sind egal, solange beide Rechtecke dieselben benutzen.
pub fn aabb_overlap(
    a_center: [f32; 2],
    a_half: [f32; 2],
    b_center: [f32; 2],
    b_half: [f32; 2],
) -> bool {
    (0..2).all(|i| (a_center[i] - b_center[i]).abs() < a_half[i] + b_half[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_overlap_cases() {
        let h = [0.5, 0.5];
        assert!(aabb_overlap([0.0, 0.0], h, [0.9, 0.0], h), "überlappend");
        assert!(
            !aabb_overlap([0.0, 0.0], h, [1.0, 0.0], h),
            "exakt anliegend"
        );
        assert!(
            !aabb_overlap([0.0, 0.0], h, [0.5, 3.0], h),
            "nur x überlappt"
        );
        assert!(
            aabb_overlap([0.0, 0.0], [5.0, 5.0], [1.0, 1.0], [0.1, 0.1]),
            "enthalten"
        );
    }

    const HALF: [f32; 2] = [0.4, 0.4];

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn free_movement_is_unchanged() {
        let r = move_and_slide([2.5, 2.5], HALF, [1.0, -0.5], |_| false);
        assert!(!r.hit_any());
        assert!(approx(r.center[0], 3.5) && approx(r.center[1], 2.0));
    }

    #[test]
    fn stops_at_wall_on_the_right() {
        let wall = |p: GridPos| p.x == 5;
        let r = move_and_slide([3.5, 2.5], HALF, [2.0, 0.0], wall);
        assert!(r.hit_x);
        assert!(approx(r.center[0], 5.0 - 0.4));
    }

    #[test]
    fn stops_at_wall_on_the_left() {
        let wall = |p: GridPos| p.x == 1;
        let r = move_and_slide([4.5, 2.5], HALF, [-3.0, 0.0], wall);
        assert!(r.hit_x);
        assert!(approx(r.center[0], 2.0 + 0.4));
    }

    #[test]
    fn slides_along_wall() {
        let wall = |p: GridPos| p.x == 5;
        let r = move_and_slide([4.5, 2.5], HALF, [1.0, 1.0], wall);
        assert!(r.hit_x && !r.hit_y);
        assert!(approx(r.center[0], 4.6));
        assert!(approx(r.center[1], 3.5), "y muss sich trotz Wand bewegen");
    }

    #[test]
    fn fast_objects_do_not_tunnel() {
        let thin_wall = |p: GridPos| p.x == 5;
        let r = move_and_slide([3.5, 2.5], [0.15, 0.15], [20.0, 0.0], thin_wall);
        assert!(r.hit_x);
        assert!(approx(r.center[0], 5.0 - 0.15));
    }

    #[test]
    fn corner_stops_both_axes() {
        let corner = |p: GridPos| p.x == 5 || p.y == 5;
        let r = move_and_slide([4.5, 4.5], HALF, [1.0, 1.0], corner);
        assert!(r.hit_x && r.hit_y);
        assert!(approx(r.center[0], 4.6) && approx(r.center[1], 4.6));
    }

    #[test]
    fn touching_a_wall_does_not_block_perpendicular_movement() {
        // Körper liegt exakt links an der Wand an und läuft dann nach oben.
        let wall = |p: GridPos| p.x == 2;
        let r = move_and_slide([3.4, 2.5], HALF, [0.0, 2.0], wall);
        assert!(!r.hit_any());
        assert!(approx(r.center[1], 4.5));
    }

    #[test]
    fn fits_through_one_tile_corridor() {
        let corridor = |p: GridPos| p.x == 2 || p.x == 4;
        let r = move_and_slide([3.5, 1.5], HALF, [0.0, 5.0], corridor);
        assert!(!r.hit_any());
    }

    // --- Ecken-Korrektur ---------------------------------------------------

    const NUDGE: Nudge = Nudge {
        max_offset: 0.4,
        max_step: 0.0625,
    };
    const PLAYER: [f32; 2] = [0.3, 0.3];

    /// Wand in Reihe y = 8 mit einer Türlücke bei x = 7.
    fn wall_with_door(p: GridPos) -> bool {
        p.y >= 8 && !(p.y == 8 && p.x == 7)
    }

    /// Läuft 30 Ticks nach oben und liefert die Endposition.
    fn walk_up(start: [f32; 2], nudged: bool) -> [f32; 2] {
        let mut c = start;
        for _ in 0..30 {
            c = if nudged {
                move_and_slide_nudged(c, PLAYER, [0.0, 0.1], NUDGE, wall_with_door).center
            } else {
                move_and_slide(c, PLAYER, [0.0, 0.1], wall_with_door).center
            };
        }
        c
    }

    #[test]
    fn without_nudge_the_player_gets_stuck() {
        // Hitbox [6.85, 7.45] ragt 0,15 Kacheln über die linke Türkante.
        let c = walk_up([7.15, 7.5], false);
        assert!(c[1] < 8.0, "Testaufbau falsch: {c:?}");
    }

    #[test]
    fn slides_into_door_gap_from_the_left() {
        let c = walk_up([7.15, 7.5], true);
        assert!(c[1] > 8.0, "sollte in der Tür stehen, ist bei {c:?}");
        assert!((7.3..=7.7).contains(&c[0]), "x außerhalb der Lücke: {c:?}");
    }

    #[test]
    fn slides_into_door_gap_from_the_right() {
        let c = walk_up([7.85, 7.5], true);
        assert!(c[1] > 8.0, "{c:?}");
    }

    #[test]
    fn slides_in_from_max_offset() {
        // 0,4 Kacheln neben der Stelle, an der die Hitbox gerade passt.
        let c = walk_up([7.3 - 0.4, 7.5], true);
        assert!(c[1] > 8.0, "{c:?}");
    }

    #[test]
    fn no_nudge_along_a_solid_wall() {
        let wall = |p: GridPos| p.y >= 8;
        let r = move_and_slide_nudged([3.5, 7.7], PLAYER, [0.0, 0.1], NUDGE, wall);
        assert!(r.hit_y);
        assert!(
            approx(r.center[0], 3.5),
            "darf nicht seitlich wandern: {:?}",
            r.center
        );
    }

    #[test]
    fn no_nudge_when_too_far_from_gap() {
        // 1,0 Kacheln neben der Lücke – außerhalb von max_offset.
        let r = move_and_slide_nudged([6.2, 7.7], PLAYER, [0.0, 0.1], NUDGE, wall_with_door);
        assert!(approx(r.center[0], 6.2), "{:?}", r.center);
    }

    #[test]
    fn never_nudges_against_player_input() {
        // Lücke liegt rechts, Spieler läuft schräg nach links oben.
        let r = move_and_slide_nudged([7.2, 7.7], PLAYER, [-0.05, 0.1], NUDGE, wall_with_door);
        assert!(
            r.center[0] < 7.2,
            "wurde gegen die Eingabe geschoben: {:?}",
            r.center
        );
    }

    #[test]
    fn nudge_step_is_limited() {
        let before = [7.2, 7.7];
        let r = move_and_slide_nudged(before, PLAYER, [0.0, 0.1], NUDGE, wall_with_door);
        assert!((r.center[0] - before[0]).abs() <= NUDGE.max_step + 1e-5);
    }
}
