//! A few large planes. The screen is cut into 6–10 convex faces that are
//! shaded as the sides of a pyramid, the inside of a box corner or a
//! folded sheet; the lightness step across each edge is the whole
//! drawing. Seams, sunk panels and any mark smaller than a face are left
//! out on purpose: at this scale they read as clutter.

use std::f64::consts::TAU;

use crate::palette::Palette;
use crate::raster::{Point, Polygon};
use crate::rng::Rng;

/// A convex face and the pseudo normal it is shaded by: the plane leans
/// toward `tilt` by `slope`, 0 being flat
struct Face {
    points: Vec<Point>,
    tilt: Point,
    slope: f64,
    /// Set once no cut through it gives two well-shaped halves
    closed: bool,
}

/// Area over squared perimeter: 0.0625 for a square, 0.048 for an
/// equilateral triangle, 0.025 for a 1:8 rectangle
const MIN_ROUNDNESS: f64 = 0.022;

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn unit(a: Point) -> Point {
    let l = a[0].hypot(a[1]);
    [a[0] / l, a[1] / l]
}
fn lerp(p: Point, q: Point, k: f64) -> Point {
    [p[0] + (q[0] - p[0]) * k, p[1] + (q[1] - p[1]) * k]
}
fn dir(angle: f64) -> Point {
    [angle.cos(), angle.sin()]
}
fn area(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| cross(points[i], points[(i + 1) % n]))
        .sum::<f64>()
        .abs()
        / 2.0
}
fn perimeter(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let d = sub(points[(i + 1) % n], points[i]);
            d[0].hypot(d[1])
        })
        .sum()
}
fn roundness(points: &[Point]) -> f64 {
    area(points) / perimeter(points).powi(2)
}
/// The mean of the vertices; inside, since every face is convex
fn centroid(points: &[Point]) -> Point {
    let n = points.len() as f64;
    let s = points
        .iter()
        .fold([0.0, 0.0], |s, p| [s[0] + p[0], s[1] + p[1]]);
    [s[0] / n, s[1] / n]
}

/// Both sides of the line through `p` along `d`, left first. A side the
/// line misses comes back with fewer than three points
fn split(points: &[Point], p: Point, d: Point) -> (Vec<Point>, Vec<Point>) {
    let side = |v: Point| cross(d, sub(v, p));
    let (mut left, mut right) = (vec![], vec![]);
    for (i, &a) in points.iter().enumerate() {
        let b = points[(i + 1) % points.len()];
        let (sa, sb) = (side(a), side(b));
        if sa >= 0.0 {
            left.push(a);
        }
        if sa <= 0.0 {
            right.push(a);
        }
        if sa * sb < 0.0 {
            let x = lerp(a, b, sa / (sa - sb));
            left.push(x);
            right.push(x);
        }
    }
    (left, right)
}

/// How much light the face catches, in [0, 1]. Lit from the upper left
fn shade(f: &Face) -> f64 {
    let n = [f.tilt[0] * f.slope, f.tilt[1] * f.slope, 1.0];
    let len = (n[0] * n[0] + n[1] * n[1] + 1.0).sqrt();
    let light = [-0.5, -0.6, 0.62];
    let dot = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / len;
    ((dot + 0.15) / 1.15).clamp(0.0, 1.0)
}

pub fn facet(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> Vec<Polygon> {
    let (lo, hi) = if palette.bright {
        (0.60, 0.95)
    } else {
        (0.15, 0.50)
    };
    let mut polygons = vec![Polygon {
        points: frame(width, height),
        color: palette.background,
    }];
    for f in faces(rng, width, height) {
        let k = shade(&f) as f32;
        // Under an analogous harmony the lit faces drift toward the
        // secondary hue; under a monochromatic one this changes nothing
        let color = palette.primary.mix(palette.secondary, k * 0.8);
        polygons.push(Polygon {
            points: f.points,
            color: color.with_lightness(lo + (hi - lo) * k),
        });
    }
    polygons
}

/// The image with a small margin, so no edge is ever left uncovered
fn frame(width: f64, height: f64) -> Vec<Point> {
    let m = 0.02 * height;
    vec![
        [-m, -m],
        [width + m, -m],
        [width + m, height + m],
        [-m, height + m],
    ]
}

fn faces(rng: &mut Rng, width: f64, height: f64) -> Vec<Face> {
    let (mut faces, grain) = if rng.coin(0.5) {
        (fan(rng, width, height), None)
    } else {
        folded(rng, width, height)
    };
    let target = 6 + rng.below(5);
    while faces.len() < target {
        let Some(i) = (0..faces.len())
            .filter(|&i| !faces[i].closed)
            .max_by(|&i, &j| area(&faces[i].points).total_cmp(&area(&faces[j].points)))
        else {
            break;
        };
        match cut(rng, &faces[i], grain) {
            Some((a, b)) => {
                faces[i] = a;
                faces.push(b);
            }
            None => faces[i].closed = true,
        }
    }
    faces
}

/// Three or four wedges around a hub: a pyramid seen from above when the
/// faces lean away from it, the corner of a box when they lean in
fn fan(rng: &mut Rng, width: f64, height: f64) -> Vec<Face> {
    let frame = frame(width, height);
    let hub = [width * rng.range(0.3, 0.7), height * rng.range(0.3, 0.7)];
    let k = 3 + rng.below(2);
    let step = TAU / k as f64;
    let base = rng.range(0.0, TAU);
    // Jitter under a quarter step keeps every wedge under 180°, so it
    // stays convex
    let angles: Vec<f64> = (0..k)
        .map(|i| base + (i as f64 + rng.range(-0.2, 0.2)) * step)
        .collect();
    let lean = if rng.coin(0.6) { 1.0 } else { -1.0 };
    let mut faces: Vec<Face> = (0..k)
        .map(|i| {
            let (left, _) = split(&frame, hub, dir(angles[i]));
            let (_, points) = split(&left, hub, dir(angles[(i + 1) % k]));
            let away = unit(sub(centroid(&points), hub));
            Face {
                points,
                tilt: [away[0] * lean, away[1] * lean],
                slope: rng.range(0.5, 1.2),
                closed: false,
            }
        })
        .collect();
    separate(rng, &mut faces, true);
    faces
}

/// Two or three long creases, nearly parallel, with the strips between
/// them leaning alternately. Also returns the crease angle, so later
/// cuts follow the same grain
fn folded(rng: &mut Rng, width: f64, height: f64) -> (Vec<Face>, Option<f64>) {
    let along = rng.range(0.0, TAU);
    let v = dir(along + TAU / 4.0);
    let center = [width / 2.0, height / 2.0];
    // Half the frame's extent across the creases
    let reach = (width * v[0].abs() + height * v[1].abs()) / 2.0;
    let creases = 2 + rng.below(2);
    let mut strips = vec![frame(width, height)];
    for i in 0..creases {
        let t = ((i as f64 + rng.range(0.25, 0.75)) / creases as f64 * 2.0 - 1.0) * reach * 0.85;
        let p = [center[0] + v[0] * t, center[1] + v[1] * t];
        let d = dir(along + rng.range(-0.06, 0.06));
        strips = strips
            .into_iter()
            .flat_map(|s| {
                let (l, r) = split(&s, p, d);
                // A crease that would leave a sliver stops short of this
                // strip instead
                if well_formed(&l, &s) && well_formed(&r, &s) {
                    vec![l, r]
                } else {
                    vec![s]
                }
            })
            .collect();
    }
    strips.sort_by(|a, b| {
        let along_v = |s: &[Point]| {
            let g = centroid(s);
            g[0] * v[0] + g[1] * v[1]
        };
        along_v(a).total_cmp(&along_v(b))
    });
    let mut faces: Vec<Face> = strips
        .into_iter()
        .enumerate()
        .map(|(i, points)| {
            let lean = if i % 2 == 0 { 1.0 } else { -1.0 };
            Face {
                points,
                tilt: [v[0] * lean, v[1] * lean],
                slope: rng.range(0.3, 0.9),
                closed: false,
            }
        })
        .collect();
    separate(rng, &mut faces, false);
    (faces, Some(along))
}

/// Two faces symmetric about the light would shade alike and their edge
/// would vanish, so each face is re-leaned until it differs from the one
/// before it. `ring` also checks the last against the first
fn separate(rng: &mut Rng, faces: &mut [Face], ring: bool) {
    let n = faces.len();
    for i in 1..if ring { n + 1 } else { n } {
        let prev = shade(&faces[i - 1]);
        let i = i % n;
        for _ in 0..12 {
            if (shade(&faces[i]) - prev).abs() >= MIN_STEP {
                break;
            }
            faces[i].slope = rng.range(0.3, 1.4);
        }
    }
}

/// The least difference in shade wanted across an edge
const MIN_STEP: f64 = 0.2;

fn well_formed(child: &[Point], parent: &[Point]) -> bool {
    child.len() >= 3 && area(child) >= 0.3 * area(parent) && roundness(child) >= MIN_ROUNDNESS
}

/// One face cut through near its middle into two well-shaped halves that
/// shade differently. Along `grain` or across it when given, else in any
/// direction. None when a dozen tries all leave a sliver
fn cut(rng: &mut Rng, f: &Face, grain: Option<f64>) -> Option<(Face, Face)> {
    let g = centroid(&f.points);
    let size = area(&f.points).sqrt();
    for _ in 0..12 {
        let p = [
            g[0] + size * rng.range(-0.2, 0.2),
            g[1] + size * rng.range(-0.2, 0.2),
        ];
        let angle = match grain {
            Some(a) => a + rng.below(2) as f64 * TAU / 4.0 + rng.range(-0.2, 0.2),
            None => rng.range(0.0, TAU),
        };
        let (l, r) = split(&f.points, p, dir(angle));
        if !(well_formed(&l, &f.points) && well_formed(&r, &f.points)) {
            continue;
        }
        let keep = Face {
            points: l,
            tilt: f.tilt,
            slope: f.slope,
            closed: false,
        };
        let mut other = Face {
            points: r,
            tilt: f.tilt,
            slope: f.slope,
            closed: false,
        };
        for _ in 0..12 {
            let turn = rng.range(0.4, 1.1) * if rng.coin(0.5) { 1.0 } else { -1.0 };
            let (s, c) = turn.sin_cos();
            other.tilt = [f.tilt[0] * c - f.tilt[1] * s, f.tilt[0] * s + f.tilt[1] * c];
            other.slope = (f.slope * rng.range(0.5, 1.6)).clamp(0.2, 1.6);
            if (shade(&other) - shade(&keep)).abs() >= MIN_STEP {
                break;
            }
        }
        return Some((keep, other));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scene_is_a_handful_of_large_faces() {
        for (w, h) in [(1920.0, 1200.0), (320.0, 200.0), (1080.0, 1920.0)] {
            for seed in 0..200 {
                let faces = faces(&mut Rng::new(seed), w, h);
                assert!(
                    (6..=10).contains(&faces.len()),
                    "seed {seed}: {}",
                    faces.len()
                );
                for f in &faces {
                    assert!(
                        roundness(&f.points) >= MIN_ROUNDNESS,
                        "seed {seed}: {:?}",
                        f.points
                    );
                }
            }
        }
    }

    #[test]
    fn under_one_hue_only_lightness_tells_the_faces_apart() {
        use crate::fixtures::dayfox;
        use crate::palette::Harmony;
        let palette = Palette::new(&dayfox(), Harmony::Monochromatic, &mut Rng::new(2));
        let polygons = facet(&palette, &mut Rng::new(2), 320.0, 200.0);
        let mut seen = vec![];
        for p in &polygons[1..] {
            assert!((p.color.hue() - palette.primary.hue()).abs() < 1e-3);
            seen.push(p.color.l);
        }
        seen.sort_by(f32::total_cmp);
        seen.dedup();
        assert!(seen.len() >= 4, "{seen:?}");
    }

    #[test]
    fn splitting_a_square_down_the_middle_gives_two_rectangles() {
        let square = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let (l, r) = split(&square, [1.0, 1.0], [0.0, 1.0]);
        assert_eq!(area(&l), 2.0);
        assert_eq!(area(&r), 2.0);
        assert_eq!(l.len() + r.len(), 8);
    }

    #[test]
    fn a_line_that_misses_leaves_one_side_empty() {
        let square = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let (l, r) = split(&square, [5.0, 0.0], [0.0, 1.0]);
        assert!(l.len() < 3 || r.len() < 3);
        assert_eq!(area(&l).max(area(&r)), 4.0);
    }
}
