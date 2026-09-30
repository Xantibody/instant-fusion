//! Scenes: a kind of wallpaper, built from a palette and a seed as flat
//! convex polygons in paint order. Nothing here touches pixels.

mod facet;
mod flow;
mod orbit;

use crate::palette::{Harmony, Palette};
use crate::raster::Polygon;
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
    use crate::fixtures::dayfox;

    fn scene(kind: Kind, seed: u64) -> Vec<Polygon> {
        let mut prng = Rng::new(seed);
        let palette = Palette::new(&dayfox(), kind.harmony(&mut prng), &mut prng);
        compose(kind, &palette, &mut Rng::new(seed), 320.0, 200.0)
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

    fn contains(p: &Polygon, q: [f64; 2]) -> bool {
        let n = p.points.len();
        let sides: Vec<f64> = (0..n)
            .map(|i| cross(p.points[i], p.points[(i + 1) % n], q))
            .collect();
        sides.iter().all(|&s| s >= 0.0) || sides.iter().all(|&s| s <= 0.0)
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
