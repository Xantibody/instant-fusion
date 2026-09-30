//! Facet: large architectural planes. One or two long cuts run edge to
//! edge across the screen and leave two to four big regions; a few of
//! those are cut once more, never all of them, for six to ten convex
//! faces. Each cut is a fold: the two sides lean apart, and one light
//! direction turns those leans into lightness, so the faces read as one
//! folded solid rather than tiles. Nothing smaller than a face is drawn.

use std::f64::consts::TAU;

use super::{best, shares};
use crate::palette::Palette;
use crate::raster::{Point, Polygon};
use crate::rng::Rng;

/// A convex face and the direction its plane leans toward, whose length
/// says how far; a flat face has no lean
#[derive(Clone, Debug)]
struct Face {
    points: Vec<Point>,
    lean: Point,
    /// Set once no cut through it gives two well-shaped halves
    closed: bool,
}

/// Area over squared perimeter: 0.0625 for a square, 0.048 for an
/// equilateral triangle, 0.035 for a 1:5 rectangle, 0.024 for a right
/// triangle with legs 1:4
const MIN_ROUNDNESS: f64 = 0.03;
/// A cut must leave each half at least this share of its parent
const MIN_HALF: f64 = 0.25;
/// The least lightness step wanted across an edge
const MIN_STEP: f32 = 0.05;
/// Compositions drawn per seed; the best by score is kept
const CANDIDATES: usize = 4;
/// The largest face should take this share of the image
const LEAD: (f64, f64) = (0.25, 0.45);

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
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

/// No face may end up under this share of the image
const MIN_FACE: f64 = 0.025;

fn well_formed(child: &[Point], parent: &[Point], total: f64) -> bool {
    child.len() >= 3
        && area(child) >= MIN_HALF * area(parent)
        && area(child) >= MIN_FACE * total
        && roundness(child) >= MIN_ROUNDNESS
}

/// Whether two faces share an edge of some length: a face's vertex lying
/// on the other's boundary, twice over
fn adjacent(a: &[Point], b: &[Point]) -> bool {
    let on_boundary = |q: Point, poly: &[Point]| {
        let n = poly.len();
        (0..n).any(|i| {
            let (p, r) = (poly[i], poly[(i + 1) % n]);
            let d = sub(r, p);
            let len = d[0].hypot(d[1]);
            let t = dot(sub(q, p), d) / (len * len);
            (-1e-9..=1.0 + 1e-9).contains(&t) && cross(d, sub(q, p)).abs() / len < 1e-6
        })
    };
    let shared = a.iter().filter(|&&q| on_boundary(q, b)).count()
        + b.iter().filter(|&&q| on_boundary(q, a)).count();
    shared >= 2
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

pub fn facet(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> Vec<Polygon> {
    best(rng, CANDIDATES, |rng| {
        candidate(palette, rng, width, height)
    })
}

/// One composition and how well it reads: one face should clearly lead
/// without swallowing the image, none should be tiny, and no corner
/// where three or more faces meet should sit in the middle, which is
/// where a hub reads as the generator
fn candidate(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> (Vec<Polygon>, f64) {
    let (faces, light) = faces(rng, width, height);
    let (lo, hi) = if palette.bright {
        (0.60, 0.95)
    } else {
        (0.15, 0.50)
    };
    let mut lightness: Vec<f32> = faces
        .iter()
        .map(|f| {
            let k = (dot(f.lean, light) as f32 + 1.0) / 2.0;
            lo + (hi - lo) * k
        })
        .collect();
    separate(&faces, &mut lightness, lo, hi);
    let mut polygons = vec![Polygon {
        points: frame(width, height),
        color: palette.background,
    }];
    for (f, &l) in faces.iter().zip(&lightness) {
        // Lit faces drift a little toward the secondary hue; under a
        // monochromatic harmony that changes nothing
        let k = (l - lo) / (hi - lo);
        polygons.push(Polygon {
            points: f.points.clone(),
            color: palette
                .primary
                .mix(palette.secondary, k * 0.3)
                .with_lightness(l),
        });
    }
    let shares = &shares(&polygons, width, height)[1..];
    let lead = shares.iter().cloned().fold(0.0, f64::max);
    let mut score = 0.0;
    if lead < LEAD.0 {
        score -= (LEAD.0 - lead) * 5.0;
    } else if lead > LEAD.1 {
        score -= (lead - LEAD.1) * 5.0;
    }
    if shares.iter().any(|&s| s < 0.02) {
        score -= 1.0;
    }
    let middle = |q: Point| {
        (0.25 * width..=0.75 * width).contains(&q[0])
            && (0.25 * height..=0.75 * height).contains(&q[1])
    };
    let near = |p: Point, q: Point| sub(p, q)[0].hypot(sub(p, q)[1]) < 1e-6;
    // Three faces meet at every T-junction; four or more is a hub
    let hub = faces.iter().flat_map(|f| &f.points).any(|&q| {
        middle(q)
            && faces
                .iter()
                .filter(|f| f.points.iter().any(|&v| near(v, q)))
                .count()
                >= 4
    });
    if hub {
        score -= 1.5;
    }
    (polygons, score)
}

/// A face whose shade sits too close to a neighbor's is moved to the
/// nearest value clear of all its neighbors, smaller faces first, so
/// every edge stays visible
fn separate(faces: &[Face], lightness: &mut [f32], lo: f32, hi: f32) {
    let mut order: Vec<usize> = (0..faces.len()).collect();
    order.sort_by(|&i, &j| area(&faces[i].points).total_cmp(&area(&faces[j].points)));
    for _ in 0..3 {
        for &i in &order {
            let others: Vec<f32> = (0..faces.len())
                .filter(|&j| j != i && adjacent(&faces[i].points, &faces[j].points))
                .map(|j| lightness[j])
                .collect();
            let clearance = |v: f32| {
                others
                    .iter()
                    .map(|o| (v - o).abs())
                    .fold(f32::MAX, f32::min)
            };
            if clearance(lightness[i]) >= MIN_STEP {
                continue;
            }
            let steps = ((hi - lo) / 0.005) as usize;
            let values = (0..=steps).map(|k| lo + (hi - lo) * k as f32 / steps as f32);
            let clear: Vec<f32> = values
                .clone()
                .filter(|&v| clearance(v) >= MIN_STEP)
                .collect();
            lightness[i] = if clear.is_empty() {
                values
                    .max_by(|&a, &b| clearance(a).total_cmp(&clearance(b)))
                    .expect("a value")
            } else {
                clear
                    .into_iter()
                    .min_by(|&a, &b| {
                        (a - lightness[i])
                            .abs()
                            .total_cmp(&(b - lightness[i]).abs())
                    })
                    .expect("a value")
            };
        }
    }
}

/// The faces and the light direction they are shaded by
fn faces(rng: &mut Rng, width: f64, height: f64) -> (Vec<Face>, Point) {
    let light = dir(rng.range(0.0, TAU));
    let mut faces = vec![Face {
        points: frame(width, height),
        lean: [0.0, 0.0],
        closed: false,
    }];
    // One long cut through the middle third, and usually a second: either
    // nearly parallel and well apart, or across it but off center, so the
    // two never meet in the middle
    let first = rng.range(0.0, TAU);
    let through = [width * rng.range(0.3, 0.7), height * rng.range(0.3, 0.7)];
    let total = area(&frame(width, height));
    let mut grains = vec![first];
    fold_all(rng, &mut faces, through, first, total);
    if rng.coin(0.75) {
        let (angle, through) = if rng.coin(0.5) {
            let n = dir(first + TAU / 4.0);
            let reach = (width * n[0].abs() + height * n[1].abs()) / 2.0;
            let side = if rng.coin(0.5) { 1.0 } else { -1.0 };
            let t = side * reach * rng.range(0.35, 0.7);
            (
                first + rng.range(-0.3, 0.3),
                [width / 2.0 + n[0] * t, height / 2.0 + n[1] * t],
            )
        } else {
            let edge = |rng: &mut Rng, size: f64| {
                size * if rng.coin(0.5) {
                    rng.range(0.1, 0.3)
                } else {
                    rng.range(0.7, 0.9)
                }
            };
            let angle = first + rng.range(0.6, 1.5) * if rng.coin(0.5) { 1.0 } else { -1.0 };
            (angle, [edge(rng, width), edge(rng, height)])
        };
        grains.push(angle);
        fold_all(rng, &mut faces, through, angle, total);
    }
    // A few more cuts, each through one face only: the largest face is
    // spared unless it has most of the image, so one plane keeps leading
    let target = 6 + rng.below(4);
    while faces.len() < target {
        let largest = (0..faces.len())
            .max_by(|&i, &j| area(&faces[i].points).total_cmp(&area(&faces[j].points)))
            .expect("faces");
        let open: Vec<usize> = (0..faces.len())
            .filter(|&i| !faces[i].closed)
            .filter(|&i| i != largest || area(&faces[i].points) > LEAD.1 * total)
            .collect();
        let Some(&i) = open.get(rng.below(open.len().max(1))) else {
            break;
        };
        // Along one of the long cuts, mostly; sometimes any way
        let angle = if rng.coin(0.7) {
            grains[rng.below(grains.len())]
                + rng.below(2) as f64 * TAU / 4.0
                + rng.range(-0.25, 0.25)
        } else {
            rng.range(0.0, TAU)
        };
        // A face that will not take a cut along the grain may take one at
        // any angle before it is given up on
        let cut = match fold(rng, &faces[i], None, angle, total) {
            Some(cut) => Some(cut),
            None => {
                let any = rng.range(0.0, TAU);
                fold(rng, &faces[i], None, any, total)
            }
        };
        match cut {
            Some((a, b)) => {
                faces[i] = a;
                faces.push(b);
            }
            None => faces[i].closed = true,
        }
    }
    (faces, light)
}

/// A cut along `angle` through `through` across every face it crosses.
/// A face it would leave a sliver in is left whole instead
fn fold_all(rng: &mut Rng, faces: &mut Vec<Face>, through: Point, angle: f64, total: f64) {
    let mut out = vec![];
    for f in faces.drain(..) {
        match fold(rng, &f, Some(through), angle, total) {
            Some((a, b)) => out.extend([a, b]),
            None => out.push(f),
        }
    }
    *faces = out;
}

/// One face cut along `angle` into two well-shaped halves that lean
/// apart from the cut like a fold. Through `through` when given, else
/// near the middle of the face, retried a dozen times before giving up
fn fold(
    rng: &mut Rng,
    f: &Face,
    through: Option<Point>,
    angle: f64,
    total: f64,
) -> Option<(Face, Face)> {
    let g = centroid(&f.points);
    let size = area(&f.points).sqrt();
    let d = dir(angle);
    let normal = [-d[1], d[0]];
    for _ in 0..12 {
        let p = through.unwrap_or_else(|| {
            [
                g[0] + size * rng.range(-0.2, 0.2),
                g[1] + size * rng.range(-0.2, 0.2),
            ]
        });
        let (l, r) = split(&f.points, p, d);
        if !(well_formed(&l, &f.points, total) && well_formed(&r, &f.points, total)) {
            if through.is_some() {
                return None;
            }
            continue;
        }
        // The halves keep some of the parent's lean and add opposite
        // leans across the cut
        let s = rng.range(0.35, 0.8);
        let lean = |sign: f64| {
            [
                f.lean[0] * 0.5 + normal[0] * s * sign,
                f.lean[1] * 0.5 + normal[1] * s * sign,
            ]
        };
        let face = |points: Vec<Point>, lean: Point| Face {
            points,
            lean,
            closed: false,
        };
        return Some((face(l, lean(1.0)), face(r, lean(-1.0))));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scene_is_a_handful_of_large_faces_with_one_leading() {
        for (w, h) in [(1920.0, 1200.0), (320.0, 200.0), (1080.0, 1920.0)] {
            let total = area(&frame(w, h));
            for seed in 0..200 {
                let (faces, _) = faces(&mut Rng::new(seed), w, h);
                assert!(
                    (5..=10).contains(&faces.len()),
                    "seed {seed}: {}",
                    faces.len()
                );
                let areas: Vec<f64> = faces.iter().map(|f| area(&f.points) / total).collect();
                let lead = areas.iter().cloned().fold(0.0, f64::max);
                assert!(lead >= 0.18, "seed {seed}: largest face {lead}");
                for (f, a) in faces.iter().zip(&areas) {
                    assert!(*a >= MIN_FACE - 1e-9, "seed {seed}: face of {a}");
                    assert!(
                        roundness(&f.points) >= MIN_ROUNDNESS,
                        "seed {seed}: {:?}",
                        f.points
                    );
                }
                let covered: f64 = areas.iter().sum();
                assert!((covered - 1.0).abs() < 1e-6, "seed {seed}: {covered}");
            }
        }
    }

    #[test]
    fn neighboring_faces_never_share_a_lightness() {
        use crate::fixtures::dayfox;
        use crate::palette::Harmony;
        for seed in 0..100 {
            let palette = Palette::new(&dayfox(), Harmony::Monochromatic, &mut Rng::new(seed));
            let polygons = facet(&palette, &mut Rng::new(seed), 320.0, 200.0);
            let faces = &polygons[1..];
            for (i, a) in faces.iter().enumerate() {
                for b in &faces[i + 1..] {
                    if adjacent(&a.points, &b.points) {
                        assert!(
                            (a.color.l - b.color.l).abs() >= MIN_STEP - 1e-6,
                            "seed {seed}: {} vs {}",
                            a.color.l,
                            b.color.l
                        );
                    }
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
        assert!(adjacent(&l, &r));
    }

    #[test]
    fn a_line_that_misses_leaves_one_side_empty() {
        let square = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let (l, r) = split(&square, [5.0, 0.0], [0.0, 1.0]);
        assert!(l.len() < 3 || r.len() < 3);
        assert_eq!(area(&l).max(area(&r)), 4.0);
    }
}
