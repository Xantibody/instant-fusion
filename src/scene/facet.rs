//! Facet: a fragment of one folded solid. A few ridges run out from an
//! apex that lies off screen, so the planes between them converge on a
//! peak or valley the viewer never sees; one or two creases cross them.
//! Every fold is where two planes meet and lean apart or toward each
//! other, and one light direction turns the leans into lightness, so the
//! faces read as connected surfaces rather than tiles. Five to eight
//! large convex faces cover the screen; nothing smaller is drawn.

use std::f64::consts::{PI, TAU};

use super::{best, shares};
use crate::color::LabExt;
use crate::palette::Palette;
use crate::raster::{Point, Polygon};
use crate::rng::Rng;
use palette::Mix;

/// A convex face and the direction its plane leans toward, whose length
/// says how far; a flat face has no lean
#[derive(Clone, Debug)]
struct Face {
    points: Vec<Point>,
    lean: Point,
    /// Set once no cut through it gives two well-shaped halves
    closed: bool,
}

/// Where two planes meet: the line through `p` along `d`. Across a ridge
/// the sides lean away from it, across a valley toward it, by `strength`
#[derive(Clone, Copy, Debug)]
struct Fold {
    p: Point,
    d: Point,
    /// 1 for a ridge, -1 for a valley
    sign: f64,
    strength: f64,
}

/// What the faces were cut from; the tests read what the scene only
/// draws
#[cfg_attr(not(test), allow(dead_code))]
struct Structure {
    /// Where the ridges meet, off screen
    apex: Point,
    light: Point,
    /// The ridges from the apex that shaped the first faces
    ridges: Vec<Fold>,
}

/// Area over squared perimeter: 0.0625 for a square, 0.048 for an
/// equilateral triangle, 0.035 for a 1:5 rectangle, 0.024 for a right
/// triangle with legs 1:4
const MIN_ROUNDNESS: f64 = 0.03;
/// A cut must leave each half at least this share of its parent
const MIN_HALF: f64 = 0.25;
/// No face may end up under this share of the image
const MIN_FACE: f64 = 0.025;
/// The least lightness step wanted across an edge
const MIN_STEP: f32 = 0.05;
/// Compositions drawn per seed; the best by score is kept
const CANDIDATES: usize = 4;
/// The largest face should take this share of the image
const LEAD: (f64, f64) = (0.25, 0.45);
/// How many faces a scene ends with
const FACES: (usize, usize) = (5, 8);
/// How far beyond an edge the apex sits, as a share of the shorter side
const REACH: (f64, f64) = (0.05, 0.9);
/// Two ridges keep at least this share of the view between them
const MIN_RIDGE_GAP: f64 = 0.18;
/// How far the planes lean across a fold
const STRENGTH: (f64, f64) = (0.3, 0.6);
/// How far every plane also tilts away from the apex, or toward it
const SLOPE: (f64, f64) = (0.15, 0.35);

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
fn borders(points: &[Point], fold: &Fold) -> bool {
    let n = points.len();
    let off = |q: Point| cross(fold.d, sub(q, fold.p)).abs();
    (0..n).any(|i| off(points[i]) < 1e-6 && off(points[(i + 1) % n]) < 1e-6)
}

/// Where a face leans, summed over the folds it borders: away from each
/// ridge and toward each valley
fn lean(points: &[Point], folds: &[Fold]) -> Point {
    let g = centroid(points);
    folds
        .iter()
        .filter(|fold| borders(points, fold))
        .fold([0.0, 0.0], |lean, fold| {
            let side = cross(fold.d, sub(g, fold.p)).signum();
            let k = side * fold.sign * fold.strength;
            [lean[0] - fold.d[1] * k, lean[1] + fold.d[0] * k]
        })
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
/// without swallowing the image, none should be tiny, the light alone
/// should tell neighbors apart, and no corner where four or more faces
/// meet should sit in the middle, which is where a hub reads as the
/// generator
fn candidate(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> (Vec<Polygon>, f64) {
    let (faces, structure) = faces(rng, width, height);
    let (lo, hi) = if palette.bright {
        (0.60, 0.95)
    } else {
        (0.15, 0.50)
    };
    let mut lightness: Vec<f32> = faces
        .iter()
        .map(|f| {
            let k = ((dot(f.lean, structure.light) as f32 + 1.0) / 2.0).clamp(0.0, 1.0);
            lo + (hi - lo) * k
        })
        .collect();
    let mut flat = 0;
    for (i, a) in faces.iter().enumerate() {
        for (j, b) in faces.iter().enumerate().skip(i + 1) {
            if adjacent(&a.points, &b.points) && (lightness[i] - lightness[j]).abs() < MIN_STEP {
                flat += 1;
            }
        }
    }
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
    let mut score = -0.3 * flat as f64;
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

/// A point beyond one edge of the screen, anywhere along it and a little
/// past its ends
fn apex(rng: &mut Rng, width: f64, height: f64) -> Point {
    let reach = width.min(height) * rng.range(REACH.0, REACH.1);
    let along = rng.range(-0.3, 1.3);
    match rng.below(4) {
        0 => [width * along, -reach],
        1 => [width + reach, height * along],
        2 => [width * along, height + reach],
        _ => [-reach, height * along],
    }
}

/// The direction from the apex to the middle of the screen, and the
/// angles either side of it within which the screen is seen; less than a
/// half turn, since the apex is outside
fn view(apex: Point, width: f64, height: f64) -> (f64, f64, f64) {
    let base = (height / 2.0 - apex[1]).atan2(width / 2.0 - apex[0]);
    frame(width, height)
        .into_iter()
        .fold((base, 0.0, 0.0), |(base, lo, hi), c| {
            let a = (c[1] - apex[1]).atan2(c[0] - apex[0]) - base;
            let a = (a + PI).rem_euclid(TAU) - PI;
            (base, lo.min(a), hi.max(a))
        })
}

/// The faces and the structure they were cut from
fn faces(rng: &mut Rng, width: f64, height: f64) -> (Vec<Face>, Structure) {
    let apex = apex(rng, width, height);
    let (base, lo, hi) = view(apex, width, height);
    // Lit from across the ridges, give or take, so the leans they cause
    // show; along them every side would look alike
    let light =
        dir(base + TAU / 4.0 * if rng.coin(0.5) { 1.0 } else { -1.0 } + rng.range(-0.6, 0.6));
    let total = area(&frame(width, height));
    let mut faces = vec![Face {
        points: frame(width, height),
        lean: [0.0, 0.0],
        closed: false,
    }];
    let mut folds: Vec<Fold> = vec![];
    let strength = |rng: &mut Rng| rng.range(STRENGTH.0, STRENGTH.1);
    // Mostly two ridges from the apex, spread across the view; a ridge
    // that would cut a sliver off is redrawn
    let wanted = match rng.below(20) {
        0..=5 => 1,
        6..=14 => 2,
        _ => 3,
    };
    let mut ridges: Vec<(f64, usize)> = vec![];
    for _ in 0..wanted {
        for _ in 0..8 {
            let t = rng.range(0.15, 0.85);
            if ridges.iter().any(|&(u, _)| (t - u).abs() < MIN_RIDGE_GAP) {
                continue;
            }
            let fold = Fold {
                p: apex,
                d: dir(base + lo + (hi - lo) * t),
                sign: 1.0,
                strength: strength(rng),
            };
            if fold_all(&mut faces, &fold, total, |_| true) {
                ridges.push((t, folds.len()));
                folds.push(fold);
                break;
            }
        }
    }
    // Neighboring ridges mostly fold the other way, so the planes zigzag
    // like a peak beside a valley; sometimes the same way, which steps
    // them like a hipped roof
    ridges.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut sign = if rng.coin(0.5) { 1.0 } else { -1.0 };
    for &(_, i) in &ridges {
        folds[i].sign = sign;
        if rng.coin(0.65) {
            sign = -sign;
        }
    }
    // One crease across the ridges, sometimes none or two, through a
    // point off the middle; where it would leave a sliver it ends at the
    // ridge before. The largest face is spared unless it has too much of
    // the image, so one plane keeps leading
    let edge = |rng: &mut Rng, size: f64| {
        size * if rng.coin(0.5) {
            rng.range(0.1, 0.3)
        } else {
            rng.range(0.7, 0.9)
        }
    };
    let creases = match rng.below(4) {
        0 => 0,
        1..=2 => 1,
        _ => 2,
    };
    for _ in 0..creases {
        let fold = Fold {
            p: [edge(rng, width), edge(rng, height)],
            d: dir(base + TAU / 4.0 + rng.range(-0.5, 0.5)),
            sign: if rng.coin(0.5) { 1.0 } else { -1.0 },
            strength: strength(rng),
        };
        let lead = faces.iter().map(|f| area(&f.points)).fold(0.0, f64::max);
        let spared = |f: &Face| area(&f.points) == lead && lead <= LEAD.1 * total;
        if fold_all(&mut faces, &fold, total, |f| !spared(f)) {
            folds.push(fold);
        }
    }
    // Too few faces, or one with too much of the image: the largest open
    // face takes one more ridge from the apex while there are under three,
    // since more read as a fan of stripes; otherwise, or failing that, a
    // crease across it near its middle; or it is closed
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
        let sign = if rng.coin(0.5) { 1.0 } else { -1.0 };
        let strength = strength(rng);
        let radial = Fold {
            p: apex,
            d: dir((g[1] - apex[1]).atan2(g[0] - apex[0]) + rng.range(-0.08, 0.08)),
            sign,
            strength,
        };
        let radials = folds.iter().filter(|f| f.p == apex).count();
        let mut fold = radial;
        let mut halves = None;
        if radials < 3 && rng.coin(0.6) {
            halves = cut(&faces[i], &fold, total);
        }
        if halves.is_none() {
            let d = dir(base + TAU / 4.0 + rng.range(-0.5, 0.5));
            for _ in 0..12 {
                let p = [
                    g[0] + size * rng.range(-0.2, 0.2),
                    g[1] + size * rng.range(-0.2, 0.2),
                ];
                fold = Fold {
                    p,
                    d,
                    sign,
                    strength,
                };
                halves = cut(&faces[i], &fold, total);
                if halves.is_some() {
                    break;
                }
            }
        }
        match halves {
            Some((a, b)) => {
                faces[i] = a;
                faces.push(b);
                folds.push(fold);
            }
            None => faces[i].closed = true,
        }
    }
    // The whole solid rises to a peak at the apex, or sinks to a valley,
    // so every plane also tilts away from it or toward it
    let slope = rng.range(SLOPE.0, SLOPE.1) * if rng.coin(0.6) { 1.0 } else { -1.0 };
    for f in &mut faces {
        let v = sub(centroid(&f.points), apex);
        let len = v[0].hypot(v[1]);
        let l = lean(&f.points, &folds);
        f.lean = [l[0] + v[0] / len * slope, l[1] + v[1] / len * slope];
    }
    let structure = Structure {
        apex,
        light,
        ridges: ridges.iter().map(|&(_, i)| folds[i]).collect(),
    };
    (faces, structure)
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

/// One face cut along the fold into two well-shaped halves, left first;
/// none when a half would be a sliver. Leans are settled once every fold
/// is known
fn cut(f: &Face, fold: &Fold, total: f64) -> Option<(Face, Face)> {
    let (l, r) = split(&f.points, fold.p, fold.d);
    if !(well_formed(&l, &f.points, total) && well_formed(&r, &f.points, total)) {
        return None;
    }
    let face = |points: Vec<Point>| Face {
        points,
        lean: [0.0, 0.0],
        closed: false,
    };
    Some((face(l), face(r)))
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
    fn the_ridges_run_from_one_apex_beyond_the_screen() {
        let (w, h) = (320.0, 200.0);
        for seed in 0..200 {
            let (faces, s) = faces(&mut Rng::new(seed), w, h);
            let inside = (0.0..=w).contains(&s.apex[0]) && (0.0..=h).contains(&s.apex[1]);
            assert!(!inside, "seed {seed}: apex {:?}", s.apex);
            assert!((1..=3).contains(&s.ridges.len()), "seed {seed}");
            for r in &s.ridges {
                assert_eq!(r.p, s.apex);
                let sides = faces.iter().filter(|f| borders(&f.points, r)).count();
                assert!(
                    sides >= 2,
                    "seed {seed}: a ridge with {sides} faces along it"
                );
            }
        }
    }

    #[test]
    fn the_sides_of_a_ridge_lean_apart_and_of_a_valley_toward_it() {
        let square = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let up = [0.0, 1.0];
        for sign in [1.0, -1.0] {
            let fold = Fold {
                p: [1.0, 1.0],
                d: up,
                sign,
                strength: 0.5,
            };
            let (l, r) = split(&square, fold.p, up);
            // The left half is the x < 1 side; leaning away from the fold
            // is leaning toward negative x
            assert_eq!(lean(&l, &[fold])[0], -0.5 * sign);
            assert_eq!(lean(&r, &[fold])[0], 0.5 * sign);
            assert_eq!(lean(&square, &[fold]), [0.0, 0.0]);
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
