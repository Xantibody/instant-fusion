//! A paneled facade: large irregular triangles in one hue, dark seams
//! between them, and a few panels sunk into the wall.

use crate::palette::Palette;
use crate::raster::{Point, Polygon};
use crate::rng::Rng;

type Tri = [Point; 3];

fn lerp(p: Point, q: Point, k: f64) -> Point {
    [p[0] + (q[0] - p[0]) * k, p[1] + (q[1] - p[1]) * k]
}
fn dist(p: Point, q: Point) -> f64 {
    (p[0] - q[0]).hypot(p[1] - q[1])
}
fn area(t: &Tri) -> f64 {
    ((t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0])).abs()
        / 2.0
}

pub fn facet(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> Vec<Polygon> {
    // Lightness of the panels, the seams and the floor of a sunk panel
    let (lo, hi, seam, pit) = if palette.bright {
        (0.68, 0.98, 0.34, 0.30)
    } else {
        (0.20, 0.58, 0.09, 0.07)
    };
    let paint = |l: f64| palette.primary.with_lightness(l as f32);

    let tris = panels(rng, width, height);
    let mut polygons = vec![];
    // Seen from the upper left, the same side as the mesh scenes' light
    let sun = [-0.64, -0.77];
    for t in &tris {
        polygons.push(Polygon {
            points: t.to_vec(),
            color: paint(lo + (hi - lo) * rng.unit().powf(0.8)),
        });
        let girth: f64 = (0..3).map(|i| dist(t[i], t[(i + 1) % 3])).sum();
        // Only a fairly round panel is sunk; in a sliver the walls would
        // swallow the floor
        if area(t) / (girth * girth) < 0.025 || !rng.coin(0.06) {
            continue;
        }
        // The floor is a smaller copy of the panel, shifted as if seen at an
        // angle. Each wall is lit by how it faces the light
        let size = area(t).sqrt();
        let g = [
            (t[0][0] + t[1][0] + t[2][0]) / 3.0 + size * rng.range(-0.12, 0.12),
            (t[0][1] + t[1][1] + t[2][1]) / 3.0 + size * rng.range(-0.12, 0.12),
        ];
        let k = rng.range(0.45, 0.62);
        let floor = t.map(|v| lerp(v, g, k));
        for i in 0..3 {
            let j = (i + 1) % 3;
            let (ex, ey) = (t[j][0] - t[i][0], t[j][1] - t[i][1]);
            let mut n = [-ey / ex.hypot(ey), ex / ex.hypot(ey)];
            if n[0] * (g[0] - t[i][0]) + n[1] * (g[1] - t[i][1]) < 0.0 {
                n = [-n[0], -n[1]];
            }
            let facing = (n[0] * sun[0] + n[1] * sun[1]).max(0.0);
            polygons.push(Polygon {
                points: vec![t[i], t[j], floor[j], floor[i]],
                color: paint(pit + (lo - pit) * (0.25 + 0.6 * facing)),
            });
        }
        polygons.push(Polygon {
            points: floor.to_vec(),
            color: paint(pit),
        });
    }
    let thick = height * 0.0018;
    for t in &tris {
        for i in 0..3 {
            let (p, q) = (t[i], t[(i + 1) % 3]);
            let d = dist(p, q);
            let n = [
                -(q[1] - p[1]) / d * thick / 2.0,
                (q[0] - p[0]) / d * thick / 2.0,
            ];
            polygons.push(Polygon {
                points: vec![
                    [p[0] + n[0], p[1] + n[1]],
                    [q[0] + n[0], q[1] + n[1]],
                    [q[0] - n[0], q[1] - n[1]],
                    [p[0] - n[0], p[1] - n[1]],
                ],
                color: paint(seam),
            });
        }
    }
    polygons
}

/// Triangles covering the image: a fan around one point, then repeated
/// splits from a vertex to the opposite edge until each is small enough
fn panels(rng: &mut Rng, width: f64, height: f64) -> Vec<Tri> {
    let m = 0.04 * height;
    let corners = [
        [-m, -m],
        [width + m, -m],
        [width + m, height + m],
        [-m, height + m],
    ];
    let hub = [width * rng.range(0.3, 0.7), height * rng.range(0.3, 0.7)];
    let mut todo: Vec<Tri> = (0..4)
        .map(|i| [hub, corners[i], corners[(i + 1) % 4]])
        .collect();
    let target = width * height / rng.range(45.0, 80.0);
    let mut done = vec![];
    while let Some(t) = todo.pop() {
        if area(&t) < target * rng.range(0.6, 2.2) {
            done.push(t);
            continue;
        }
        // AIDEV-NOTE: a split is deliberately not carried into the
        // neighbor. The T-junctions that leaves are what make this read as
        // panels on a wall rather than a mesh
        let longest = (0..3)
            .max_by(|&i, &j| dist(t[i], t[(i + 1) % 3]).total_cmp(&dist(t[j], t[(j + 1) % 3])))
            .unwrap();
        // Usually the longest edge; the odd other one makes the slivers
        let e = if rng.coin(0.12) {
            rng.below(3)
        } else {
            longest
        };
        let (p, q, o) = (t[e], t[(e + 1) % 3], t[(e + 2) % 3]);
        let cut = lerp(p, q, rng.range(0.3, 0.7));
        todo.extend([[o, p, cut], [o, cut, q]]);
    }
    done
}
