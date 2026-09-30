//! Orbit: two to four arcs of rings far larger than the screen, each
//! centered outside it, so only one sweep of the rim shows. A ring is
//! sampled at a fixed angular step over the angles that reach the screen
//! and cut into convex quads of one color; the rasterizer never sees a
//! curve.

use std::f64::consts::TAU;

use crate::color::Lab;
use crate::palette::Palette;
use crate::raster::{Point, Polygon};
use crate::rng::Rng;

struct Arc {
    center: Point,
    /// Radii along x and y to the middle of the ring
    rx: f64,
    ry: f64,
    /// Thickness of the ring
    width: f64,
    color: Lab,
    /// The accent, if this arc carries it, as a share of the visible sweep:
    /// where it starts and how long it runs
    accent: Option<(f64, f64, Lab)>,
}

/// Radii as a share of the width; the smallest is still most of a screen
const MIN_RADIUS: f64 = 0.7;
const MAX_RADIUS: f64 = 2.0;
/// Ring widths as a share of the height
const MIN_WIDTH: f64 = 0.08;
const MAX_WIDTH: f64 = 0.22;
/// The accent runs over at most this share of one arc's visible sweep
const MAX_ACCENT_SHARE: f64 = 0.35;
/// Angle per quad: on a circle it is also the bend at each joint, and
/// 2.3° is where a curve stops looking cut from straight pieces
const STEP: f64 = 0.04;
/// A full turn at that step
const MAX_QUADS: usize = 160;

fn ellipse(center: Point, rx: f64, ry: f64, angle: f64) -> Point {
    [center[0] + rx * angle.cos(), center[1] + ry * angle.sin()]
}

pub fn orbit(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> Vec<Polygon> {
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
    for arc in &arcs(palette, rng, width, height) {
        polygons.extend(ring(arc, width, height));
    }
    polygons
}

fn arcs(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> Vec<Arc> {
    let n = 2 + rng.below(3);
    let mut arcs = vec![];
    for _ in 0..n {
        if let Some(arc) = place(rng, width, height) {
            arcs.push(arc);
        }
    }
    // Wide arcs go under narrow ones, so none is hidden
    arcs.sort_by(|a, b| b.width.total_cmp(&a.width));
    let colors = [
        palette.primary,
        palette.secondary,
        palette.primary.mix(palette.secondary, 0.5),
    ];
    for (i, arc) in arcs.iter_mut().enumerate() {
        arc.color = colors[i % colors.len()];
    }
    // Most scenes light a short sweep of the topmost arc in the accent
    if rng.coin(0.7) {
        let share = rng.range(0.15, MAX_ACCENT_SHARE);
        let start = rng.range(0.0, 1.0 - share);
        if let Some(top) = arcs.last_mut() {
            top.accent = Some((start, share, palette.accent));
        }
    }
    arcs
}

/// One arc whose center is off screen and whose rim crosses it. None
/// when a few dozen draws all miss the screen or the radius bounds
fn place(rng: &mut Rng, width: f64, height: f64) -> Option<Arc> {
    for _ in 0..64 {
        let center = [width * rng.range(-0.8, 1.8), height * rng.range(-0.8, 1.8)];
        if (0.0..=width).contains(&center[0]) && (0.0..=height).contains(&center[1]) {
            continue;
        }
        let ratio = rng.range(0.6, 1.4);
        // Squeezing y by the ratio turns the ellipse into a circle of
        // radius rx, so the distances to the screen bound the radii
        // that reach it
        let (cx, cy) = (center[0], center[1] / ratio);
        let (w, h) = (width, height / ratio);
        let near = (cx.clamp(0.0, w) - cx).hypot(cy.clamp(0.0, h) - cy);
        let far = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
            .iter()
            .map(|c| (c[0] - cx).hypot(c[1] - cy))
            .fold(0.0, f64::max);
        let rx = near + (far - near) * rng.range(0.15, 0.85);
        if !(MIN_RADIUS * width..=MAX_RADIUS * width).contains(&rx) {
            continue;
        }
        return Some(Arc {
            center,
            rx,
            ry: rx * ratio,
            width: height * rng.range(MIN_WIDTH, MAX_WIDTH),
            color: Lab::new(0.0, 0.0, 0.0),
            accent: None,
        });
    }
    None
}

/// The arc as quads between its inner and outer rim, over the sweep
/// that reaches the screen
fn ring(arc: &Arc, width: f64, height: f64) -> Vec<Polygon> {
    let half = arc.width / 2.0;
    // Coarse pass: which angles land within a ring's width of the screen
    let probes = 720;
    let reaches = |i: usize| {
        let p = ellipse(arc.center, arc.rx, arc.ry, TAU * i as f64 / probes as f64);
        (-half..=width + half).contains(&p[0]) && (-half..=height + half).contains(&p[1])
    };
    // Start the walk at an angle that misses, so a sweep across zero is
    // not cut in two
    let start = (0..probes).find(|&i| !reaches(i)).unwrap_or(0);
    // Each sweep as its first probe and how many probes it runs
    let mut sweeps: Vec<(usize, usize)> = vec![];
    let mut open = false;
    for k in 0..probes {
        let i = (start + k) % probes;
        if reaches(i) {
            match sweeps.last_mut() {
                Some((_, len)) if open => *len += 1,
                _ => sweeps.push((i, 1)),
            }
        }
        open = reaches(i);
    }
    let mut quads = vec![];
    for (first, len) in sweeps {
        // One probe of slack on each end, so the arc leaves the screen
        // before it ends
        let a0 = TAU * (first as f64 - 1.0) / probes as f64;
        let sweep = TAU * (len as f64 + 2.0) / probes as f64;
        let n = ((sweep / STEP).ceil() as usize).clamp(1, MAX_QUADS);
        let accent_range = arc
            .accent
            .map(|(s, share, c)| (a0 + sweep * s, a0 + sweep * (s + share), c));
        for i in 0..n {
            let (t0, t1) = (
                a0 + sweep * i as f64 / n as f64,
                a0 + sweep * (i + 1) as f64 / n as f64,
            );
            let color = match accent_range {
                Some((from, to, c)) if (t0 + t1) / 2.0 >= from && (t0 + t1) / 2.0 < to => c,
                _ => arc.color,
            };
            quads.push(Polygon {
                points: vec![
                    ellipse(arc.center, arc.rx - half, arc.ry - half, t0),
                    ellipse(arc.center, arc.rx - half, arc.ry - half, t1),
                    ellipse(arc.center, arc.rx + half, arc.ry + half, t1),
                    ellipse(arc.center, arc.rx + half, arc.ry + half, t0),
                ],
                color,
            });
        }
    }
    quads
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::dayfox;
    use crate::palette::Harmony;

    fn palette(seed: u64) -> Palette {
        Palette::new(&dayfox(), Harmony::MutedComplementary, &mut Rng::new(seed))
    }

    fn inside(p: Point, w: f64, h: f64) -> bool {
        (0.0..=w).contains(&p[0]) && (0.0..=h).contains(&p[1])
    }

    #[test]
    fn a_scene_is_a_few_huge_arcs_centered_off_screen() {
        let (w, h) = (320.0, 200.0);
        for seed in 0..200 {
            let arcs = arcs(&palette(seed), &mut Rng::new(seed), w, h);
            assert!((2..=4).contains(&arcs.len()), "seed {seed}");
            for a in &arcs {
                assert!(!inside(a.center, w, h), "seed {seed}: {:?}", a.center);
                assert!(a.rx >= MIN_RADIUS * w - 1e-9 && a.rx <= MAX_RADIUS * w + 1e-9);
                let ratio = a.ry / a.rx;
                assert!((0.6..=1.4).contains(&ratio), "seed {seed}: {ratio}");
                assert!(a.width >= MIN_WIDTH * h - 1e-9 && a.width <= MAX_WIDTH * h + 1e-9);
                let quads = ring(a, w, h);
                assert!(!quads.is_empty() && quads.len() <= MAX_QUADS, "seed {seed}");
                assert!(
                    quads
                        .iter()
                        .flat_map(|q| &q.points)
                        .any(|&p| inside(p, w, h)),
                    "seed {seed}: arc never enters the screen"
                );
            }
        }
    }

    #[test]
    fn the_accent_is_a_short_sweep_of_one_arc() {
        let (w, h) = (320.0, 200.0);
        for seed in 0..200 {
            let palette = palette(seed);
            let arcs = arcs(&palette, &mut Rng::new(seed), w, h);
            let mut carriers = 0;
            for a in &arcs {
                let quads = ring(a, w, h);
                let accent = quads.iter().filter(|q| q.color == palette.accent).count();
                if accent > 0 {
                    carriers += 1;
                    assert!(accent as f64 <= MAX_ACCENT_SHARE * quads.len() as f64 + 1.0);
                }
            }
            assert!(carriers <= 1, "seed {seed}");
        }
    }
}
