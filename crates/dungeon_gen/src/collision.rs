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

#[cfg(test)]
mod tests {
    use super::*;

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
}
