//! Scenes: a kind of wallpaper, built from a palette and a seed as flat
//! convex polygons in paint order. Nothing here touches pixels.

mod mesh;

use crate::palette::Palette;
use crate::raster::Polygon;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Strong low-poly relief under a linear gradient
    Terrain,
    /// Faint low-poly relief under colors blended around a pale center
    Lowpoly,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Terrain, Kind::Lowpoly];
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
        Kind::Terrain => mesh::mesh(palette, rng, width, height, mesh::Relief::Strong),
        Kind::Lowpoly => mesh::mesh(palette, rng, width, height, mesh::Relief::Faint),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::dayfox;

    fn scene(kind: Kind, seed: u64) -> Vec<Polygon> {
        let palette = Palette::new(&dayfox(), &mut Rng::new(seed));
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
