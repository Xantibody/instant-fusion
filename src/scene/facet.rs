//! Facet: a fragment of one folded solid. One ridge runs across the
//! screen with a plane on either side; one or two creases cross it, and
//! a plane still too large takes a crease of its own. Every fold turns
//! the plane normals on its two sides away from each other, a ridge, or
//! toward each other, a valley, and one light gives each face its
//! lightness from its normal alone, so neighbors differ because they
//! face the light differently and for no other reason. Five to eight
//! convex faces cover the screen; nothing smaller is drawn.

use std::f64::consts::TAU;

use super::{best, shares};
use crate::color::LabExt;
use crate::palette::Palette;
use crate::raster::{Point, Polygon};
use crate::rng::Rng;

/// A convex face and the normal of its plane, unit length, z toward the
/// viewer
#[derive(Clone, Debug)]
struct Face {
    points: Vec<Point>,
    normal: [f64; 3],
    /// Set once no cut through it gives two well-shaped halves
    closed: bool,
}

/// Where two planes meet: the line through `p` along `d`, with the
/// planes turned `angle` apart about it, away from each other across a
/// ridge and toward each other across a valley
#[derive(Clone, Copy, Debug)]
struct Fold {
    p: Point,
    d: Point,
    /// 1 for a ridge, -1 for a valley
    sign: f64,
    angle: f64,
}

/// What the faces were cut from; the tests read what the scene only
/// draws
#[cfg_attr(not(test), allow(dead_code))]
struct Structure {
    light: [f64; 3],
    /// Every fold in the order it was cut; the first is the ridge
    folds: Vec<Fold>,
}

/// Area over squared perimeter: 0.0625 for a square, 0.048 for an
/// equilateral triangle, 0.035 for a 1:5 rectangle, 0.024 for a right
/// triangle with legs 1:4
const MIN_ROUNDNESS: f64 = 0.03;
/// A cut must leave each half at least this share of its parent
const MIN_HALF: f64 = 0.25;
/// No face may end up under this share of the image
const MIN_FACE: f64 = 0.025;
/// Compositions drawn per seed; the best by score is kept
const CANDIDATES: usize = 4;
/// The largest face should take this share of the image
const LEAD: (f64, f64) = (0.25, 0.45);
/// How many faces a scene ends with
const FACES: (usize, usize) = (5, 8);
/// How far the planes turn apart across a fold, in radians: 20° to 45°
const FOLD: (f64, f64) = (0.35, 0.8);
/// How far the first plane may lean before any fold, in radians
const TILT: f64 = 0.25;
/// The light's height above the screen plane, in radians: 20° to 40°,
/// low enough that a turn of the normal changes the shade clearly
const ELEVATION: (f64, f64) = (0.35, 0.7);
/// A crease crosses the ridge at no shallower an angle than this
const MIN_CROSS: f64 = 0.5;
/// Two neighbors whose shades differ by less than this read as one
const FLAT: f32 = 0.03;

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

/// `v` turned by `angle` about the axis lying in the screen along `d`
fn turn(v: [f64; 3], d: Point, angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    let a = [d[0], d[1], 0.0];
    let along = a[0] * v[0] + a[1] * v[1];
    let axv = [a[1] * v[2], -a[0] * v[2], a[0] * v[1] - a[1] * v[0]];
    [
        v[0] * c + axv[0] * s + a[0] * along * (1.0 - c),
        v[1] * c + axv[1] * s + a[1] * along * (1.0 - c),
        v[2] * c + axv[2] * s,
    ]
}

/// How much of the light a plane with this normal catches, 0 to 1
fn shade(normal: [f64; 3], light: [f64; 3]) -> f64 {
    (normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2]).clamp(0.0, 1.0)
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

/// Whether an edge of the face runs along the fold
#[cfg(test)]
fn borders(points: &[Point], fold: &Fold) -> bool {
    let n = points.len();
    let off = |q: Point| cross(fold.d, sub(q, fold.p)).abs();
    (0..n).any(|i| off(points[i]) < 1e-6 && off(points[(i + 1) % n]) < 1e-6)
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
/// without swallowing the image, none should be tiny, the light should
/// tell every pair of neighbors apart, and no corner where four or more
/// faces meet should sit in the middle, which is where a hub reads as
/// the generator
fn candidate(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> (Vec<Polygon>, f64) {
    let (faces, structure) = faces(rng, width, height);
    let (lo, hi) = if palette.bright {
        (0.60, 0.95)
    } else {
        (0.15, 0.50)
    };
    let lightness: Vec<f32> = faces
        .iter()
        .map(|f| lo + (hi - lo) * shade(f.normal, structure.light) as f32)
        .collect();
    let mut flat = 0;
    for (i, a) in faces.iter().enumerate() {
        for (j, b) in faces.iter().enumerate().skip(i + 1) {
            if adjacent(&a.points, &b.points) && (lightness[i] - lightness[j]).abs() < FLAT {
                flat += 1;
            }
        }
    }
    let mut polygons = vec![Polygon {
        points: frame(width, height),
        color: palette.background,
    }];
    for (f, &l) in faces.iter().zip(&lightness) {
        polygons.push(Polygon {
            points: f.points.clone(),
            color: palette.primary.with_lightness(l),
        });
    }
    let shares = &shares(&polygons, width, height)[1..];
    let lead = shares.iter().cloned().fold(0.0, f64::max);
    let mut score = -0.5 * flat as f64;
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

/// The faces and the structure they were cut from
fn faces(rng: &mut Rng, width: f64, height: f64) -> (Vec<Face>, Structure) {
    let total = area(&frame(width, height));
    let coin = |rng: &mut Rng| if rng.coin(0.5) { 1.0 } else { -1.0 };
    // The ridge: across the middle third at any angle. The first plane
    // leans a little some way, so the ridge is not seen exactly edge on
    let ridge = Fold {
        p: [width * rng.range(0.3, 0.7), height * rng.range(0.3, 0.7)],
        d: dir(rng.range(0.0, TAU)),
        sign: coin(rng),
        angle: rng.range(FOLD.0, FOLD.1),
    };
    let mut faces = vec![Face {
        points: frame(width, height),
        normal: turn(
            [0.0, 0.0, 1.0],
            dir(rng.range(0.0, TAU)),
            rng.range(0.0, TILT),
        ),
        closed: false,
    }];
    let mut folds = vec![];
    if fold_all(&mut faces, &ridge, total, |_| true) {
        folds.push(ridge);
    }
    // Lit from low over the screen, from any side; higher, every plane
    // would catch about the same light
    let azimuth = rng.range(0.0, TAU);
    let elevation = rng.range(ELEVATION.0, ELEVATION.1);
    let light = [
        azimuth.cos() * elevation.cos(),
        azimuth.sin() * elevation.cos(),
        elevation.sin(),
    ];
    // One crease across the ridge, sometimes two or none, through a point
    // off the middle; where it would leave a sliver it ends at the ridge.
    // The largest face is spared unless it has too much of the image, so
    // one plane keeps leading
    let edge = |rng: &mut Rng, size: f64| {
        size * if rng.coin(0.5) {
            rng.range(0.1, 0.3)
        } else {
            rng.range(0.7, 0.9)
        }
    };
    let creases = match rng.below(10) {
        0..=1 => 0,
        2..=6 => 1,
        _ => 2,
    };
    let across = |rng: &mut Rng| {
        let base = ridge.d[1].atan2(ridge.d[0]);
        dir(base + coin(rng) * rng.range(MIN_CROSS, TAU / 2.0 - MIN_CROSS))
    };
    for _ in 0..creases {
        let fold = Fold {
            p: [edge(rng, width), edge(rng, height)],
            d: across(rng),
            sign: coin(rng),
            angle: rng.range(FOLD.0, FOLD.1),
        };
        let lead = faces.iter().map(|f| area(&f.points)).fold(0.0, f64::max);
        let spared = |f: &Face| area(&f.points) == lead && lead <= LEAD.1 * total;
        if fold_all(&mut faces, &fold, total, |f| !spared(f)) {
            folds.push(fold);
        }
    }
    // Too few faces, or one with too much of the image: the largest open
    // face takes a crease of its own near its middle, across the ridge
    // or any way, or is closed
    loop {
        let open = (0..faces.len()).filter(|&i| !faces[i].closed);
        let Some(i) =
            open.max_by(|&i, &j| area(&faces[i].points).total_cmp(&area(&faces[j].points)))
        else {
            break;
        };
        let lead = area(&faces[i].points) / total;
        if faces.len() >= FACES.0 && (lead <= LEAD.1 || faces.len() >= FACES.1) {
            break;
        }
        let g = centroid(&faces[i].points);
        let size = area(&faces[i].points).sqrt();
        let d = if rng.coin(0.6) {
            across(rng)
        } else {
            dir(rng.range(0.0, TAU))
        };
        let (sign, angle) = (coin(rng), rng.range(FOLD.0, FOLD.1));
        let mut cut_with = None;
        for _ in 0..12 {
            let fold = Fold {
                p: [
                    g[0] + size * rng.range(-0.2, 0.2),
                    g[1] + size * rng.range(-0.2, 0.2),
                ],
                d,
                sign,
                angle,
            };
            if let Some(halves) = cut(&faces[i], &fold, total) {
                cut_with = Some((fold, halves));
                break;
            }
        }
        match cut_with {
            Some((fold, (a, b))) => {
                faces[i] = a;
                faces.push(b);
                folds.push(fold);
            }
            None => faces[i].closed = true,
        }
    }
    (faces, Structure { light, folds })
}

/// The fold cut across every face it crosses that `may` be cut, until
/// the most faces exist; a face it would leave a sliver in is left
/// whole. Whether any was cut
fn fold_all(faces: &mut Vec<Face>, fold: &Fold, total: f64, may: impl Fn(&Face) -> bool) -> bool {
    let n = faces.len();
    let mut out = Vec::with_capacity(n + 1);
    let mut any = false;
    for (i, f) in faces.drain(..).enumerate() {
        let room = out.len() + (n - i) < FACES.1 && may(&f);
        match room.then(|| cut(&f, fold, total)).flatten() {
            Some((a, b)) => {
                out.extend([a, b]);
                any = true;
            }
            None => out.push(f),
        }
    }
    *faces = out;
    any
}

/// One face cut along the fold into two well-shaped halves, left first,
/// each turned half the fold's angle about it: away from the fold for a
/// ridge, toward it for a valley. None when a half would be a sliver
fn cut(f: &Face, fold: &Fold, total: f64) -> Option<(Face, Face)> {
    let (l, r) = split(&f.points, fold.p, fold.d);
    if !(well_formed(&l, &f.points, total) && well_formed(&r, &f.points, total)) {
        return None;
    }
    // A positive turn about the fold's direction tips the normal toward
    // the right side; the left face of a ridge tips the other way
    let face = |points: Vec<Point>, side: f64| Face {
        points,
        normal: turn(f.normal, fold.d, -side * fold.sign * fold.angle / 2.0),
        closed: false,
    };
    Some((face(l, 1.0), face(r, -1.0)))
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
                    (FACES.0..=FACES.1).contains(&faces.len()),
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
    fn the_ridge_crosses_the_screen_and_no_folds_share_a_point() {
        let (w, h) = (320.0, 200.0);
        for seed in 0..200 {
            let (faces, s) = faces(&mut Rng::new(seed), w, h);
            let ridge = s.folds[0];
            let (l, r) = split(&frame(w, h), ridge.p, ridge.d);
            assert!(l.len() >= 3 && r.len() >= 3, "seed {seed}");
            let sides = faces.iter().filter(|f| borders(&f.points, &ridge)).count();
            assert!(
                sides >= 2,
                "seed {seed}: the ridge has {sides} faces along it"
            );
            // Three folds through one point would be a fan
            for (i, a) in s.folds.iter().enumerate() {
                for (j, b) in s.folds.iter().enumerate().skip(i + 1) {
                    for c in &s.folds[j + 1..] {
                        let meet = |x: &Fold, y: &Fold| -> Option<Point> {
                            let den = cross(x.d, y.d);
                            (den.abs() > 1e-9).then(|| {
                                let t = cross(sub(y.p, x.p), y.d) / den;
                                [x.p[0] + x.d[0] * t, x.p[1] + x.d[1] * t]
                            })
                        };
                        if let (Some(p), Some(q)) = (meet(a, b), meet(a, c)) {
                            assert!(sub(p, q)[0].hypot(sub(p, q)[1]) > 1e-6, "seed {seed}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_sides_of_a_ridge_tip_apart_and_of_a_valley_toward_it() {
        let flat = Face {
            points: vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]],
            normal: [0.0, 0.0, 1.0],
            closed: false,
        };
        for sign in [1.0, -1.0] {
            let fold = Fold {
                p: [1.0, 1.0],
                d: [0.0, 1.0],
                sign,
                angle: 0.6,
            };
            let (l, r) = cut(&flat, &fold, 4.0).expect("two halves");
            // The left half is the x < 1 side; tipping away from the fold
            // is tipping toward negative x
            assert!((l.normal[0] - -sign * 0.3f64.sin()).abs() < 1e-9);
            assert!((r.normal[0] - sign * 0.3f64.sin()).abs() < 1e-9);
            assert!((l.normal[2] - 0.3f64.cos()).abs() < 1e-9);
            let dot3 =
                l.normal[0] * r.normal[0] + l.normal[1] * r.normal[1] + l.normal[2] * r.normal[2];
            assert!(
                (dot3.acos() - 0.6).abs() < 1e-9,
                "the halves turn the fold's angle apart"
            );
        }
    }

    #[test]
    fn a_shade_is_the_light_a_plane_catches() {
        let light = [0.6, 0.0, 0.8];
        assert!((shade(light, light) - 1.0).abs() < 1e-9);
        assert_eq!(shade([-0.8, 0.0, 0.6], light), 0.0);
        assert!(shade([0.0, 0.0, 1.0], light) < shade(light, light));
        assert!(shade([-0.6, 0.0, 0.8], light) < shade([0.0, 0.0, 1.0], light));
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
            assert!((p.color.chroma() - palette.primary.chroma()).abs() < 1e-3);
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
