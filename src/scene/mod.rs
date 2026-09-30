//! Scenes: a kind of wallpaper, built from a palette and a seed as flat
//! convex polygons in paint order. Nothing here touches pixels.

mod facet;
mod flow;
mod orbit;

use crate::palette::{Harmony, Palette};
use crate::raster::{Point, Polygon};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    /// A few wide, smooth bands sweeping across the screen
    Flow,
    /// A few sweeps of rings far larger than the screen, centered off it
    Orbit,
    /// A few large planes told apart by lightness alone
    Facet,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Flow, Kind::Orbit, Kind::Facet];

    /// The color harmony a seed picks for this kind. Facet lives on
    /// lightness and stays mostly in one hue; orbit is the one place a
    /// dull complementary accent is wanted
    pub fn harmony(self, rng: &mut Rng) -> Harmony {
        match self {
            Kind::Flow => {
                if rng.coin(0.5) {
                    Harmony::Monochromatic
                } else {
                    Harmony::Analogous
                }
            }
            Kind::Orbit => {
                if rng.coin(0.7) {
                    Harmony::Analogous
                } else {
                    Harmony::MutedComplementary
                }
            }
            Kind::Facet => {
                if rng.coin(0.75) {
                    Harmony::Monochromatic
                } else {
                    Harmony::Analogous
                }
            }
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Flow => "flow",
            Kind::Orbit => "orbit",
            Kind::Facet => "facet",
        }
    }
}

/// The best of `n` candidates by the score `make` returns with each.
/// Every candidate draws from `rng`, so a seed still names one scene;
/// this only spares the seeds that would have drawn a poor composition
pub(crate) fn best<T>(rng: &mut Rng, n: usize, mut make: impl FnMut(&mut Rng) -> (T, f64)) -> T {
    let (mut best, mut top) = make(rng);
    for _ in 1..n {
        let (candidate, score) = make(rng);
        if score > top {
            (best, top) = (candidate, score);
        }
    }
    best
}

/// Whether the convex polygon covers the point
pub(crate) fn contains(p: &Polygon, q: Point) -> bool {
    let n = p.points.len();
    let cross = |o: Point, a: Point, b: Point| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let sides = (0..n).map(|i| cross(p.points[i], p.points[(i + 1) % n], q));
    let (mut pos, mut neg) = (false, false);
    for s in sides {
        pos |= s > 0.0;
        neg |= s < 0.0;
    }
    !(pos && neg)
}

pub(crate) const GRID: (usize, usize) = (48, 30);

/// Which polygon is on top at each point of a coarse grid over the
/// image, row by row, or None where nothing is painted. Geometry alone
/// is enough to judge a composition, so no pixels are involved
pub(crate) fn owners(polygons: &[Polygon], width: f64, height: f64) -> Vec<Option<usize>> {
    let (cols, rows) = GRID;
    // Most polygons are small quads, so a box test spares nearly every
    // point the full check
    let boxes: Vec<[f64; 4]> = polygons
        .iter()
        .map(|p| {
            p.points
                .iter()
                .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, q| {
                    [
                        b[0].min(q[0]),
                        b[1].min(q[1]),
                        b[2].max(q[0]),
                        b[3].max(q[1]),
                    ]
                })
        })
        .collect();
    let mut out = Vec::with_capacity(cols * rows);
    for r in 0..rows {
        for c in 0..cols {
            let q = [
                width * (c as f64 + 0.5) / cols as f64,
                height * (r as f64 + 0.5) / rows as f64,
            ];
            out.push(boxes.iter().enumerate().rev().find_map(|(i, b)| {
                let inside = q[0] >= b[0] && q[0] <= b[2] && q[1] >= b[1] && q[1] <= b[3];
                (inside && contains(&polygons[i], q)).then_some(i)
            }));
        }
    }
    out
}

/// The share of the grid each polygon is on top of
pub(crate) fn shares(polygons: &[Polygon], width: f64, height: f64) -> Vec<f64> {
    let owners = owners(polygons, width, height);
    let mut shares = vec![0.0; polygons.len()];
    for i in owners.into_iter().flatten() {
        shares[i] += 1.0 / (GRID.0 * GRID.1) as f64;
    }
    shares
}

/// Polygons covering the `width` × `height` image, in paint order
pub fn compose(
    kind: Kind,
    palette: &Palette,
    rng: &mut Rng,
    width: f64,
    height: f64,
) -> Vec<Polygon> {
    match kind {
        Kind::Flow => flow::flow(palette, rng, width, height),
        Kind::Orbit => orbit::orbit(palette, rng, width, height),
        Kind::Facet => facet::facet(palette, rng, width, height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Lab;
    use crate::fixtures::dayfox;

    fn scene(kind: Kind, seed: u64) -> Vec<Polygon> {
        let mut prng = Rng::new(seed);
        let palette = Palette::new(&dayfox(), kind.harmony(&mut prng), &mut prng);
        compose(kind, &palette, &mut Rng::new(seed), 320.0, 200.0)
    }

    #[test]
    fn best_keeps_the_highest_scoring_candidate() {
        let mut rng = Rng::new(1);
        let mut calls = 0;
        let pick = best(&mut rng, 4, |_| {
            calls += 1;
            (calls, [3.0, 9.0, 1.0, 9.0][calls - 1])
        });
        assert_eq!(pick, 2);
        assert_eq!(calls, 4);
    }

    #[test]
    fn owners_reports_the_topmost_polygon_and_the_gaps() {
        let rect = |x0: f64, y0: f64, x1: f64, y1: f64| Polygon {
            points: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            color: Lab::new(0.5, 0.0, 0.0),
        };
        // Left half under, a small square over the middle, right half bare
        let polygons = [rect(0.0, 0.0, 50.0, 100.0), rect(40.0, 40.0, 60.0, 60.0)];
        let owners = owners(&polygons, 100.0, 100.0);
        let at = |x: f64, y: f64| {
            let (cols, rows) = GRID;
            let (c, r) = (
                (x / 100.0 * cols as f64) as usize,
                (y / 100.0 * rows as f64) as usize,
            );
            owners[r * cols + c]
        };
        assert_eq!(at(10.0, 10.0), Some(0));
        assert_eq!(at(50.0, 50.0), Some(1));
        assert_eq!(at(90.0, 90.0), None);
        let shares = shares(&polygons, 100.0, 100.0);
        assert!(
            (shares[0] - 0.48).abs() < 0.04 && (shares[1] - 0.04).abs() < 0.02,
            "{shares:?}"
        );
    }

    #[test]
    fn the_same_seed_gives_the_same_scene() {
        for kind in Kind::ALL {
            assert_eq!(scene(kind, 5), scene(kind, 5));
            assert_ne!(scene(kind, 5), scene(kind, 6));
        }
    }

    fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    }

    #[test]
    fn every_polygon_is_convex_and_has_an_area() {
        for kind in Kind::ALL {
            for seed in 0..20 {
                for p in scene(kind, seed) {
                    let n = p.points.len();
                    assert!(n >= 3);
                    let turns: Vec<f64> = (0..n)
                        .map(|i| cross(p.points[i], p.points[(i + 1) % n], p.points[(i + 2) % n]))
                        .collect();
                    let convex = turns.iter().all(|&t| t >= 0.0) || turns.iter().all(|&t| t <= 0.0);
                    assert!(convex, "{kind:?} seed {seed}: {:?}", p.points);
                    assert!(turns.iter().any(|&t| t != 0.0));
                }
            }
        }
    }

    #[test]
    fn the_polygons_cover_the_whole_image() {
        for kind in Kind::ALL {
            for seed in 0..20 {
                let polygons = scene(kind, seed);
                for y in 0..=20 {
                    for x in 0..=32 {
                        let q = [x as f64 * 10.0, y as f64 * 10.0];
                        assert!(
                            polygons.iter().any(|p| contains(p, q)),
                            "{kind:?} seed {seed}: {q:?} uncovered"
                        );
                    }
                }
            }
        }
    }
}
