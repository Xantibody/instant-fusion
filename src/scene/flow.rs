//! Flow: layered movement. One master curve, low in frequency, sweeps
//! across the screen; two to four bands ride it at their own offsets,
//! widths and slight waves of their own, so they belong to one current
//! without being copies of each other. A band is sampled along x and cut
//! into vertical-sided quads of one color; the rasterizer never sees a
//! curve.

use std::f64::consts::TAU;

use super::{best, shares};
use crate::color::Lab;
use crate::palette::Palette;
use crate::raster::{Point, Polygon};
use crate::rng::Rng;
use lyon_geom::euclid::default::Transform2D;
use lyon_geom::{Angle, point, vector};

/// The curve every band follows: two low-frequency waves and a tilt, so
/// it bends once or twice across the screen and may leave it
struct Master {
    a1: f64,
    k1: f64,
    p1: f64,
    a2: f64,
    k2: f64,
    p2: f64,
    drift: f64,
}

impl Master {
    fn at(&self, x: f64, width: f64) -> f64 {
        self.a1 * (self.k1 * x + self.p1).sin()
            + self.a2 * (self.k2 * x + self.p2).sin()
            + self.drift * (x - width / 2.0)
    }

    fn slope(&self, x: f64) -> f64 {
        self.a1 * self.k1 * (self.k1 * x + self.p1).cos()
            + self.a2 * self.k2 * (self.k2 * x + self.p2).cos()
            + self.drift
    }

    /// An upper bound on the bend anywhere along it
    fn curvature(&self) -> f64 {
        self.a1 * self.k1 * self.k1 + self.a2 * self.k2 * self.k2
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// The one wide band the eye lands on first
    Dominant,
    /// Narrower, quieter in color
    Secondary,
    /// Narrowest, in the accent color
    Accent,
}

struct Band {
    role: Role,
    /// Multiplies the master's swing
    scale: f64,
    /// A slight wave of its own, so no two bands are parallel
    va: f64,
    vk: f64,
    vp: f64,
    offset: f64,
    /// Thickness measured across the band
    width: f64,
    color: Lab,
}

impl Band {
    fn shape(&self, m: &Master, x: f64, width: f64) -> f64 {
        m.at(x, width) * self.scale + self.va * (self.vk * x + self.vp).sin()
    }
    fn center(&self, m: &Master, x: f64, width: f64) -> f64 {
        self.shape(m, x, width) + self.offset
    }
    fn slope(&self, m: &Master, x: f64) -> f64 {
        m.slope(x) * self.scale + self.va * self.vk * (self.vk * x + self.vp).cos()
    }
    fn curvature(&self, m: &Master) -> f64 {
        m.curvature() * self.scale + self.va * self.vk * self.vk
    }
}

/// No band is thinner than this share of the height
const MIN_WIDTH: f64 = 0.06;
/// The least ground left between two bands, as a share of the height
const MIN_GAP: f64 = 0.05;
/// When a band lies over its neighbor, by this share of the narrower one
const OVERLAP: (f64, f64) = (0.35, 0.6);
/// Quads per band at the tightest curve
const MAX_SEGMENTS: usize = 200;
/// Compositions drawn per seed; the best by score is kept
const CANDIDATES: usize = 4;
/// Where the separation between two bands is checked
const PROBES: usize = 96;
/// How far the whole current may tilt either way, in radians: 25°
const SLANT: f64 = 0.44;
/// Two bands whose separation varies by less than this share of the
/// height run parallel
const PARALLEL: f64 = 0.1;

fn frame(width: f64, height: f64) -> Vec<Point> {
    let m = 0.02 * height;
    vec![
        [-m, -m],
        [width + m, -m],
        [width + m, height + m],
        [-m, height + m],
    ]
}

pub fn flow(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> Vec<Polygon> {
    best(rng, CANDIDATES, |rng| {
        candidate(palette, rng, width, height)
    })
}

/// One composition and how well it reads: the ground should stay the
/// largest area without the bands drifting mostly off screen, the
/// dominant band must actually show, some band should leave through an
/// edge, since bands all shown end to end read as stripes, and no two
/// bands should run parallel
fn candidate(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> (Vec<Polygon>, f64) {
    // The current runs at a slant. The bands are laid out over the box
    // that covers the screen turned back by that slant, then turned with
    // it, so a horizontal layout never shows as one
    let slant = rng.range(-SLANT, SLANT);
    let (s, c) = slant.sin_cos();
    let (bw, bh) = (
        width * c.abs() + height * s.abs(),
        height * c.abs() + width * s.abs(),
    );
    let (master, bands) = bands(palette, rng, bw, bh);
    let turn = Transform2D::rotation(Angle::radians(slant))
        .pre_translate(vector(-bw / 2.0, -bh / 2.0))
        .then_translate(vector(width / 2.0, height / 2.0));
    let mut polygons = vec![Polygon {
        points: frame(width, height),
        color: palette.background,
    }];
    let mut ranges = vec![];
    for band in &bands {
        let from = polygons.len();
        polygons.extend(ribbon(&master, band, bw, bh).into_iter().map(|mut q| {
            for p in &mut q.points {
                let t = turn.transform_point(point(p[0], p[1]));
                *p = [t.x, t.y];
            }
            q
        }));
        ranges.push(from..polygons.len());
    }
    let shares = shares(&polygons, width, height);
    let ground = shares[0];
    let dominant = bands
        .iter()
        .position(|b| b.role == Role::Dominant)
        .expect("one dominant band");
    let lead: f64 = shares[ranges[dominant].clone()].iter().sum();
    let mut score = -((ground - 0.5).abs() * 2.0);
    if lead < 0.12 {
        score -= 1.0;
    }
    if ranges
        .iter()
        .any(|r| leaves(&polygons[r.clone()], width, height))
    {
        score += 0.3;
    } else if bands.len() >= 3 {
        score -= 0.5;
    }
    let xs = (0..PROBES).map(|i| bw * i as f64 / (PROBES - 1) as f64);
    for (i, a) in bands.iter().enumerate() {
        for b in &bands[i + 1..] {
            let (lo, hi) = xs.clone().fold((f64::MAX, f64::MIN), |(lo, hi), x| {
                let d = a.shape(&master, x, bw) - b.shape(&master, x, bw);
                (lo.min(d), hi.max(d))
            });
            if hi - lo < PARALLEL * bh {
                score -= 0.3;
            }
        }
    }
    (polygons, score)
}

/// Whether a band, as its quads on screen, is cut by an edge of the
/// screen: entirely off it somewhere and on it somewhere else
fn leaves(quads: &[Polygon], width: f64, height: f64) -> bool {
    let gone = quads.iter().any(|q| {
        q.points.iter().all(|p| p[0] < 0.0)
            || q.points.iter().all(|p| p[0] > width)
            || q.points.iter().all(|p| p[1] < 0.0)
            || q.points.iter().all(|p| p[1] > height)
    });
    let shown = quads.iter().any(|q| {
        q.points
            .iter()
            .any(|p| (0.0..=width).contains(&p[0]) && (0.0..=height).contains(&p[1]))
    });
    gone && shown
}

fn bands(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> (Master, Vec<Band>) {
    let a1 = height * rng.range(0.10, 0.22);
    let master = Master {
        a1,
        k1: TAU / (width * rng.range(1.2, 2.5)),
        p1: rng.range(0.0, TAU),
        a2: a1 * rng.range(0.2, 0.45),
        k2: TAU / (width * rng.range(0.8, 1.1)),
        p2: rng.range(0.0, TAU),
        drift: rng.range(-0.3, 0.3),
    };
    // Two to four bands, mostly three
    let n = match rng.below(20) {
        0..=6 => 2,
        7..=14 => 3,
        _ => 4,
    };
    let lead = height * rng.range(0.14, 0.24);
    let mut roles: Vec<(Role, f64)> = vec![(Role::Dominant, lead)];
    for _ in 1..n {
        roles.push((Role::Secondary, lead * rng.range(0.55, 0.8)));
    }
    if n >= 3 && rng.coin(0.6) {
        roles[n - 1] = (
            Role::Accent,
            (lead * rng.range(0.35, 0.55)).max(MIN_WIDTH * height),
        );
    }
    // The dominant band is not always on top: shuffle the stack
    for i in (1..n).rev() {
        roles.swap(i, rng.below(i + 1));
    }
    let mut bands: Vec<Band> = roles
        .into_iter()
        .map(|(role, w)| Band {
            role,
            scale: rng.range(0.8, 1.2),
            va: height * rng.range(0.0, 0.05),
            vk: TAU / (width * rng.range(0.7, 1.4)),
            vp: rng.range(0.0, TAU),
            offset: 0.0,
            width: w,
            color: match role {
                Role::Dominant => palette.primary,
                Role::Secondary => palette.secondary,
                Role::Accent => palette.accent,
            },
        })
        .collect();
    // Each band is set below the one before it by the least separation
    // measured along the screen, so two bands either keep a clear gap or
    // clearly overlap. AIDEV-NOTE: bands never nearly touch; a near miss
    // pinches the ground between them into a crescent sliver.
    // An overlap is never followed by another, so a band cannot reach
    // through its neighbor to the one beyond
    let xs: Vec<f64> = (0..PROBES)
        .map(|i| -0.02 * height + (width + 0.04 * height) * i as f64 / (PROBES - 1) as f64)
        .collect();
    let mut overlapped = false;
    for i in 1..n {
        let (lo, hi) = xs.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &x| {
            let d = bands[i].shape(&master, x, width) - bands[i - 1].shape(&master, x, width);
            (lo.min(d), hi.max(d))
        });
        let touch = bands[i - 1].offset + (bands[i - 1].width + bands[i].width) / 2.0;
        let narrow = bands[i - 1].width.min(bands[i].width);
        // Only bands shaped alike may overlap: two that differ by more
        // than a third of the narrower would swap sides and cross
        overlapped = !overlapped && hi - lo <= 0.35 * narrow && rng.coin(0.3);
        // A little more than the least, since the extremes between two
        // probes go unmeasured
        bands[i].offset = if overlapped {
            touch - narrow * rng.range(OVERLAP.0 + 0.05, OVERLAP.1) - hi
        } else {
            touch + height * rng.range(MIN_GAP + 0.01, 0.3) - lo
        };
    }
    // The whole stack sits at a random height and may run off the top or
    // bottom by a varying amount, which is how a band leaves the screen
    let (top, bottom) = bands.iter().fold((f64::MAX, f64::MIN), |(t, b), band| {
        xs.iter().fold((t, b), |(t, b), &x| {
            let c = band.center(&master, x, width);
            (t.min(c - band.width / 2.0), b.max(c + band.width / 2.0))
        })
    });
    let slack = height * rng.range(0.1, 0.45);
    let (lo, hi) = (-slack - top, height + slack - (bottom - top) - top);
    let shift = lo.min(hi) + (lo.max(hi) - lo.min(hi)) * rng.unit();
    for band in &mut bands {
        band.offset += shift;
    }
    // Wide bands go under narrow ones, so an overlap shows the narrow one
    bands.sort_by(|a, b| b.width.total_cmp(&a.width));
    (master, bands)
}

/// One band as quads between its upper and lower edge, left to right
fn ribbon(master: &Master, band: &Band, width: f64, height: f64) -> Vec<Polygon> {
    let m = 0.02 * height;
    // Short enough that the polyline bends no more than about 2.3° at any
    // joint, which is where a curve stops looking cut from straight pieces
    let span = width + 2.0 * m;
    let n = ((band.curvature(master) * span / 0.04).ceil() as usize).clamp(16, MAX_SEGMENTS);
    let edge = |x: f64| -> (Point, Point) {
        let y = band.center(master, x, width);
        // A vertical cut through a sloping band is longer than its width;
        // the cap keeps a steep stretch from ballooning
        let half = band.width * (1.0 + band.slope(master, x).powi(2)).sqrt().min(1.8) / 2.0;
        ([x, y - half], [x, y + half])
    };
    (0..n)
        .map(|i| {
            let (u0, l0) = edge(-m + span * i as f64 / n as f64);
            let (u1, l1) = edge(-m + span * (i + 1) as f64 / n as f64);
            Polygon {
                points: vec![u0, u1, l1, l0],
                color: band.color,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::dayfox;
    use crate::palette::Harmony;

    fn palette(seed: u64) -> Palette {
        Palette::new(&dayfox(), Harmony::Analogous, &mut Rng::new(seed))
    }

    #[test]
    fn a_scene_is_a_few_bands_of_clearly_different_widths() {
        let (w, h) = (320.0, 200.0);
        for seed in 0..200 {
            let palette = palette(seed);
            let (_, bands) = bands(&palette, &mut Rng::new(seed), w, h);
            assert!((2..=4).contains(&bands.len()), "seed {seed}");
            assert_eq!(bands.iter().filter(|b| b.role == Role::Dominant).count(), 1);
            let widest = bands[0].width;
            let narrowest = bands.last().unwrap().width;
            assert!(widest / narrowest >= 1.25, "seed {seed}");
            for b in &bands {
                assert!(b.width >= MIN_WIDTH * h - 1e-9, "seed {seed}");
                assert!(b.width <= widest, "seed {seed}");
                if b.role == Role::Accent {
                    assert_eq!(b.width, narrowest);
                }
            }
        }
    }

    #[test]
    fn bands_either_keep_a_clear_gap_or_clearly_overlap() {
        let (w, h) = (320.0, 200.0);
        for seed in 0..200 {
            let (master, bands) = bands(&palette(seed), &mut Rng::new(seed), w, h);
            for (i, a) in bands.iter().enumerate() {
                for b in &bands[i + 1..] {
                    let narrow = a.width.min(b.width);
                    let (mut gap, mut over) = (true, true);
                    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                    for k in 0..=400 {
                        let x = -0.02 * h + (w + 0.04 * h) * k as f64 / 400.0;
                        let d = (a.center(&master, x, w) - b.center(&master, x, w)).abs()
                            - (a.width + b.width) / 2.0;
                        gap &= d >= MIN_GAP * h - 1e-6;
                        over &= d <= -OVERLAP.0 * narrow + 1e-6;
                        lo = lo.min(d);
                        hi = hi.max(d);
                    }
                    assert!(
                        gap || over,
                        "seed {seed}: widths {} and {}, separation {lo} to {hi}",
                        a.width,
                        b.width
                    );
                }
            }
        }
    }

    /// The bands of a scene as runs of quads: within a band each quad
    /// starts on the edge the one before it ends on
    fn runs(polygons: &[Polygon]) -> Vec<&[Polygon]> {
        let starts: Vec<usize> = (1..polygons.len())
            .filter(|&i| i == 1 || polygons[i].points[0] != polygons[i - 1].points[1])
            .collect();
        starts
            .iter()
            .enumerate()
            .map(|(k, &s)| &polygons[s..starts.get(k + 1).copied().unwrap_or(polygons.len())])
            .collect()
    }

    #[test]
    fn most_scenes_let_a_band_leave_through_the_top_or_bottom() {
        let (w, h) = (320.0, 200.0);
        let mut left = 0;
        for seed in 0..100 {
            let polygons = flow(&palette(seed), &mut Rng::new(seed), w, h);
            let leaves = runs(&polygons).into_iter().any(|run| leaves(run, w, h));
            left += leaves as usize;
        }
        assert!(left >= 50, "{left} of 100 scenes let a band leave");
    }

    #[test]
    fn the_quads_stay_bounded_and_never_thin_out() {
        let (w, h) = (320.0, 200.0);
        for seed in 0..100 {
            let polygons = flow(&palette(seed), &mut Rng::new(seed), w, h);
            assert!(polygons.len() <= 1 + 4 * MAX_SEGMENTS, "seed {seed}");
            for p in &polygons[1..] {
                let [u0, u1, l1, l0] = p.points[..] else {
                    panic!("seed {seed}: {:?}", p.points)
                };
                let thick = |a: Point, b: Point| (a[0] - b[0]).hypot(a[1] - b[1]);
                assert!(thick(u0, l0) >= MIN_WIDTH * h - 1e-9, "seed {seed}");
                assert!(thick(u1, l1) >= MIN_WIDTH * h - 1e-9, "seed {seed}");
            }
        }
    }
}
