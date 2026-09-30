//! Orbit: fragments of huge off-screen geometry. One dominant ring, far
//! larger than the screen and centered well outside it, sets the
//! structure; one or two related rings share or shift its center. Only
//! the sweep of each rim that reaches the screen is drawn, as convex
//! quads of one color; the rasterizer never sees a curve.

use std::f64::consts::TAU;

use super::{best, shares};
use crate::color::Lab;
use crate::palette::Palette;
use crate::raster::{Point, Polygon};
use crate::rng::Rng;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Dominant,
    Secondary,
    Accent,
}

#[derive(Clone, Debug)]
struct Arc {
    role: Role,
    center: Point,
    /// Radii along x and y to the middle of the ring
    rx: f64,
    ry: f64,
    /// Thickness of the ring
    width: f64,
    color: Lab,
}

/// Radii of the dominant ring as a share of the width: well over a
/// screen, so only a small part of the ring shows and its curvature
/// stays gentle
const RADIUS: (f64, f64) = (1.5, 4.0);
/// Width of the dominant ring as a share of the height
const WIDTH: (f64, f64) = (0.08, 0.16);
/// No ring is thinner than this share of the height
const MIN_WIDTH: f64 = 0.03;
/// The center keeps at least this share of the shorter side off screen
const CLEARANCE: f64 = 0.15;
/// Angle per quad: on a circle it is also the bend at each joint, and
/// 2.3° is where a curve stops looking cut from straight pieces
const STEP: f64 = 0.04;
/// A full turn at that step
const MAX_QUADS: usize = 160;
/// Compositions drawn per seed; the best by score is kept
const CANDIDATES: usize = 5;

fn ellipse(center: Point, rx: f64, ry: f64, angle: f64) -> Point {
    [center[0] + rx * angle.cos(), center[1] + ry * angle.sin()]
}

pub fn orbit(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> Vec<Polygon> {
    best(rng, CANDIDATES, |rng| {
        candidate(palette, rng, width, height)
    })
}

/// One composition and how well it reads. Rings that cross more than
/// once, crowd the middle, or show alike lengths read as a symbol, and
/// even one crossing is a little less quiet than none; a scene wants one
/// clear sweep, the rest quieter, and mostly ground
fn candidate(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> (Vec<Polygon>, f64) {
    let arcs = arcs(palette, rng, width, height);
    let m = 0.02 * height;
    let mut polygons = vec![Polygon {
        points: vec![
            [-m, -m],
            [width + m, -m],
            [width + m, height + m],
            [-m, height + m],
        ],
        color: palette.background,
    }];
    let mut ranges = vec![];
    for arc in &arcs {
        let from = polygons.len();
        polygons.extend(ring(arc, width, height));
        ranges.push(from..polygons.len());
    }
    let shares = shares(&polygons, width, height);
    // Mostly ground, but not so much that the rings shrink to slivers at
    // the edges
    let mut score = -(shares[0] - 0.72).abs() * 3.0;
    if shares[0] < 0.5 {
        score -= 2.0;
    }
    let crossings = crossings(&arcs, width, height);
    if crossings > 1 {
        score -= 3.0 * (crossings - 1) as f64;
    } else if crossings == 1 {
        score -= 0.5;
    }
    let seen: Vec<f64> = ranges
        .iter()
        .map(|r| shares[r.clone()].iter().sum())
        .collect();
    for (arc, &seen) in arcs.iter().zip(&seen) {
        match arc.role {
            Role::Dominant if seen < 0.06 => score -= 1.0,
            Role::Accent if seen > 0.15 => score -= 1.0,
            _ => {}
        }
    }
    for i in 0..seen.len() {
        for j in i + 1..seen.len() {
            if (seen[i] - seen[j]).abs() < 0.15 * seen[i].max(seen[j]) {
                score -= 0.5;
            }
        }
    }
    let middle = |q: Point| {
        (0.3 * width..=0.7 * width).contains(&q[0]) && (0.3 * height..=0.7 * height).contains(&q[1])
    };
    let crowded = ranges
        .iter()
        .filter(|r: &&std::ops::Range<usize>| {
            polygons[(*r).clone()]
                .iter()
                .any(|p| p.points.iter().any(|&q| middle(q)))
        })
        .count();
    if crowded >= 3 {
        score -= 1.0;
    }
    (polygons, score)
}

fn arcs(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> Vec<Arc> {
    let lead = dominant(rng, width, height);
    let mut arcs = vec![lead.clone()];
    // Mostly two rings; three about a third of the time, then the third
    // is either a second secondary or an accent
    let third = rng.coin(0.35);
    let accent = third && rng.coin(0.5);
    for _ in 0..if third { 2 } else { 1 } {
        if let Some(arc) = related(rng, &lead, Role::Secondary, width, height) {
            arcs.push(arc);
        }
    }
    if accent && let Some(arc) = related(rng, &lead, Role::Accent, width, height) {
        arcs.pop();
        arcs.push(arc);
    }
    // Wide rings go under narrow ones, so none is hidden
    arcs.sort_by(|a, b| b.width.total_cmp(&a.width));
    for arc in &mut arcs {
        arc.color = match arc.role {
            Role::Dominant => palette.primary,
            Role::Secondary => palette.secondary,
            Role::Accent => palette.accent,
        };
    }
    arcs
}

fn off_screen(center: Point, width: f64, height: f64) -> bool {
    let c = CLEARANCE * width.min(height);
    !((-c..=width + c).contains(&center[0]) && (-c..=height + c).contains(&center[1]))
}

/// The dominant ring: built from a point it passes through on screen, a
/// direction and a radius, then kept only when its center is clearly off
/// screen
fn dominant(rng: &mut Rng, width: f64, height: f64) -> Arc {
    let mut arc = None;
    for _ in 0..64 {
        let through = [
            width * rng.range(0.15, 0.85),
            height * rng.range(0.15, 0.85),
        ];
        let angle = rng.range(0.0, TAU);
        let ratio = rng.range(0.7, 1.3);
        // Weighted toward the smaller radii; the largest are nearly straight
        let rx = width * (RADIUS.0 + (RADIUS.1 - RADIUS.0) * rng.unit().powi(2));
        // In the space where the ellipse is a circle of radius rx, the
        // center sits rx away from the through point
        let center = [
            through[0] + rx * angle.cos(),
            (through[1] / ratio + rx * angle.sin()) * ratio,
        ];
        let candidate = Arc {
            role: Role::Dominant,
            center,
            rx,
            ry: rx * ratio,
            width: height * rng.range(WIDTH.0, WIDTH.1),
            color: Lab::new(0.0, 0.0, 0.0),
        };
        if off_screen(center, width, height) {
            return candidate;
        }
        arc.get_or_insert(candidate);
    }
    arc.expect("one draw at least")
}

/// A ring related to the dominant one: a secondary shares its center
/// almost exactly at a nearby radius, an accent shifts well away at a
/// larger radius. None when no draw reaches the screen
fn related(rng: &mut Rng, lead: &Arc, role: Role, width: f64, height: f64) -> Option<Arc> {
    for _ in 0..16 {
        let (center, rx, w) = match role {
            Role::Accent => {
                let away = rng.range(0.0, TAU);
                let shift = width * rng.range(0.3, 0.8);
                (
                    [
                        lead.center[0] + shift * away.cos(),
                        lead.center[1] + shift * away.sin(),
                    ],
                    lead.rx * rng.range(1.2, 2.0),
                    lead.width * rng.range(0.3, 0.55),
                )
            }
            _ => (
                [
                    lead.center[0] + width * rng.range(-0.08, 0.08),
                    lead.center[1] + height * rng.range(-0.08, 0.08),
                ],
                lead.rx * rng.range(0.72, 1.25),
                lead.width * rng.range(0.45, 0.8),
            ),
        };
        let arc = Arc {
            role,
            center,
            rx,
            ry: rx * lead.ry / lead.rx,
            width: w.max(MIN_WIDTH * height),
            color: lead.color,
        };
        if off_screen(center, width, height) && enters(&arc, width, height) {
            return Some(arc);
        }
    }
    None
}

const PROBES: usize = 720;

/// Whether the middle of the ring passes over the screen itself, not
/// merely within a width of it
fn enters(arc: &Arc, width: f64, height: f64) -> bool {
    (0..PROBES).any(|i| {
        let p = ellipse(arc.center, arc.rx, arc.ry, TAU * i as f64 / PROBES as f64);
        (0.0..=width).contains(&p[0]) && (0.0..=height).contains(&p[1])
    })
}

/// The sweeps of the ring that come within its width of the screen, each
/// as a start angle and length in radians
fn sweeps(arc: &Arc, width: f64, height: f64) -> Vec<(f64, f64)> {
    let half = arc.width / 2.0;
    let reaches = |i: usize| {
        let p = ellipse(arc.center, arc.rx, arc.ry, TAU * i as f64 / PROBES as f64);
        (-half..=width + half).contains(&p[0]) && (-half..=height + half).contains(&p[1])
    };
    // Start the walk at an angle that misses, so a sweep across zero is
    // not cut in two
    let start = (0..PROBES).find(|&i| !reaches(i)).unwrap_or(0);
    let mut sweeps: Vec<(usize, usize)> = vec![];
    let mut open = false;
    for k in 0..PROBES {
        let i = (start + k) % PROBES;
        if reaches(i) {
            match sweeps.last_mut() {
                Some((_, len)) if open => *len += 1,
                _ => sweeps.push((i, 1)),
            }
        }
        open = reaches(i);
    }
    // One probe of slack on each end, so the arc leaves the screen before
    // it ends
    sweeps
        .into_iter()
        .map(|(first, len)| {
            (
                TAU * (first as f64 - 1.0) / PROBES as f64,
                TAU * (len as f64 + 2.0) / PROBES as f64,
            )
        })
        .collect()
}

/// The arc as quads between its inner and outer rim
fn ring(arc: &Arc, width: f64, height: f64) -> Vec<Polygon> {
    let half = arc.width / 2.0;
    let mut quads = vec![];
    for (a0, sweep) in sweeps(arc, width, height) {
        let n = ((sweep / STEP).ceil() as usize).clamp(1, MAX_QUADS);
        for i in 0..n {
            let t0 = a0 + sweep * i as f64 / n as f64;
            let t1 = a0 + sweep * (i + 1) as f64 / n as f64;
            quads.push(Polygon {
                points: vec![
                    ellipse(arc.center, arc.rx - half, arc.ry - half, t0),
                    ellipse(arc.center, arc.rx - half, arc.ry - half, t1),
                    ellipse(arc.center, arc.rx + half, arc.ry + half, t1),
                    ellipse(arc.center, arc.rx + half, arc.ry + half, t0),
                ],
                color: arc.color,
            });
        }
    }
    quads
}

/// The middle line of each visible sweep as a polyline
fn midlines(arc: &Arc, width: f64, height: f64) -> Vec<Vec<Point>> {
    sweeps(arc, width, height)
        .into_iter()
        .map(|(a0, sweep)| {
            let n = ((sweep / STEP).ceil() as usize).clamp(1, MAX_QUADS);
            (0..=n)
                .map(|i| ellipse(arc.center, arc.rx, arc.ry, a0 + sweep * i as f64 / n as f64))
                .collect()
        })
        .collect()
}

fn segments_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    let side = |p: Point, q: Point, r: Point| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    (side(a, b, c) > 0.0) != (side(a, b, d) > 0.0) && (side(c, d, a) > 0.0) != (side(c, d, b) > 0.0)
}

/// How many times the rings' middle lines cross on screen
fn crossings(arcs: &[Arc], width: f64, height: f64) -> usize {
    let lines: Vec<Vec<Vec<Point>>> = arcs.iter().map(|a| midlines(a, width, height)).collect();
    let on_screen = |p: Point| (0.0..=width).contains(&p[0]) && (0.0..=height).contains(&p[1]);
    let mut count = 0;
    for i in 0..lines.len() {
        for j in i + 1..lines.len() {
            for a in &lines[i] {
                for b in &lines[j] {
                    for s in a.windows(2) {
                        for t in b.windows(2) {
                            if segments_cross(s[0], s[1], t[0], t[1])
                                && (on_screen(s[0]) || on_screen(s[1]))
                            {
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::dayfox;
    use crate::palette::Harmony;

    fn palette(seed: u64) -> Palette {
        Palette::new(&dayfox(), Harmony::MutedComplementary, &mut Rng::new(seed))
    }

    #[test]
    fn a_scene_is_two_or_three_related_rings_centered_well_off_screen() {
        let (w, h) = (320.0, 200.0);
        for seed in 0..200 {
            let arcs = arcs(&palette(seed), &mut Rng::new(seed), w, h);
            assert!((2..=3).contains(&arcs.len()), "seed {seed}");
            assert_eq!(arcs.iter().filter(|a| a.role == Role::Dominant).count(), 1);
            let lead = arcs.iter().find(|a| a.role == Role::Dominant).unwrap();
            assert!(lead.rx >= RADIUS.0 * w - 1e-9 && lead.rx <= RADIUS.1 * w + 1e-9);
            for a in &arcs {
                assert!(off_screen(a.center, w, h), "seed {seed}: {:?}", a.center);
                assert!(a.width >= MIN_WIDTH * h - 1e-9 && a.width <= lead.width);
                let quads = ring(a, w, h);
                assert!(!quads.is_empty() && quads.len() <= MAX_QUADS, "seed {seed}");
                assert!(enters(a, w, h), "seed {seed}: ring never enters the screen");
                assert!(
                    quads.iter().all(|q| q.color == a.color),
                    "one color per ring"
                );
            }
        }
    }

    #[test]
    fn the_chosen_scene_lets_rings_cross_at_most_once() {
        let (w, h) = (320.0, 200.0);
        for seed in 0..100 {
            let palette = palette(seed);
            let polygons = orbit(&palette, &mut Rng::new(seed), w, h);
            assert!(polygons.len() <= 1 + 3 * MAX_QUADS);
            // Rebuild the rings the scene kept by their colors
            let mut colors = vec![];
            for p in &polygons[1..] {
                if !colors.contains(&p.color) {
                    colors.push(p.color);
                }
            }
            assert!(colors.len() <= 3, "seed {seed}: {}", colors.len());
        }
    }

    #[test]
    fn crossings_counts_each_meeting_of_two_rings_once() {
        let a = Arc {
            role: Role::Dominant,
            center: [-200.0, 100.0],
            rx: 400.0,
            ry: 400.0,
            width: 20.0,
            color: Lab::new(0.0, 0.0, 0.0),
        };
        let mut b = a.clone();
        b.center = [-200.0, 100.0];
        b.rx = 420.0;
        b.ry = 420.0;
        assert_eq!(crossings(&[a.clone(), b], 320.0, 200.0), 0);
        let mut c = a.clone();
        // Rims 790 apart cross at x = 195, y = 37 and 163: both on screen
        c.center = [590.0, 100.0];
        assert_eq!(crossings(&[a, c], 320.0, 200.0), 2);
    }
}
