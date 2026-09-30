//! Flow: two to five wide, smooth bands sweeping across the screen. All
//! follow one wave, each at its own height and width, so they read as
//! one current rather than a tangle. A band is the wave sampled
//! along x and cut into vertical-sided quads of one color; the rasterizer
//! never sees a curve.

use std::f64::consts::TAU;

use crate::color::Lab;
use crate::palette::Palette;
use crate::raster::{Point, Polygon};
use crate::rng::Rng;

/// The wave every band of one scene follows
struct Wave {
    wavelength: f64,
    phase: f64,
    /// Rise of the centerline per unit of x
    drift: f64,
}

struct Band {
    /// Height of the centerline at the middle of the screen
    base: f64,
    amplitude: f64,
    /// Thickness measured across the band
    width: f64,
    color: Lab,
}

/// Band widths as a share of the height
const MIN_WIDTH: f64 = 0.08;
const MAX_WIDTH: f64 = 0.25;
/// An accent band is kept narrow so the accent stays a small share of
/// the image
const ACCENT_WIDTH: f64 = 0.10;
/// Together the bands never take more than this share of the height, so
/// a good part of the ground stays quiet
const MAX_TOTAL_WIDTH: f64 = 0.6;
/// Quads per band at the tightest curve
const MAX_SEGMENTS: usize = 160;
/// The least ground left between two bands, as a share of the height
const MIN_GAP: f64 = 0.06;
/// How far a band may stray from the shared amplitude. Kept well under
/// the gap, so neighbors never meet
const AMPLITUDE_PLAY: f64 = 0.08;

pub fn flow(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> Vec<Polygon> {
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
    let (wave, bands) = bands(palette, rng, width, height);
    for band in &bands {
        polygons.extend(ribbon(&wave, band, width, height));
    }
    polygons
}

fn bands(palette: &Palette, rng: &mut Rng, width: f64, height: f64) -> (Wave, Vec<Band>) {
    let wave = Wave {
        wavelength: width * rng.range(1.0, 2.2),
        phase: rng.range(0.0, TAU),
        drift: rng.range(-0.15, 0.15),
    };
    let n = 2 + rng.below(4);
    let amplitude = height * rng.range(0.08, 0.2);
    let mut widths: Vec<f64> = (0..n).map(|_| rng.range(MIN_WIDTH, MAX_WIDTH)).collect();
    // Only the part above the minimum is squeezed, so no band drops
    // under it
    let (total, floor) = (widths.iter().sum::<f64>(), n as f64 * MIN_WIDTH);
    if total > MAX_TOTAL_WIDTH {
        let k = (MAX_TOTAL_WIDTH - floor) / (total - floor);
        for w in &mut widths {
            *w = MIN_WIDTH + (*w - MIN_WIDTH) * k;
        }
    }
    // Stacked top to bottom with a clear gap between neighbors, then the
    // stack is set at a random height. AIDEV-NOTE: bands do not cross.
    // Crossing bands pinch the ground between them into crescent slivers
    let gaps: Vec<f64> = (1..n).map(|_| height * rng.range(MIN_GAP, 0.25)).collect();
    let stack = widths.iter().sum::<f64>() * height + gaps.iter().sum::<f64>();
    let mut top = (height - stack) * rng.unit();
    let mut bands = vec![];
    for (i, &w) in widths.iter().enumerate() {
        bands.push(Band {
            base: top + w * height / 2.0,
            amplitude: amplitude * rng.range(1.0 - AMPLITUDE_PLAY, 1.0 + AMPLITUDE_PLAY),
            width: w * height,
            color: palette.background,
        });
        top += w * height + gaps.get(i).copied().unwrap_or(0.0);
    }
    // Wide bands go under narrow ones, so none is hidden
    bands.sort_by(|a, b| b.width.total_cmp(&a.width));
    let colors = [
        palette.primary,
        palette.secondary,
        palette.primary.mix(palette.secondary, 0.5),
    ];
    for (i, band) in bands.iter_mut().enumerate() {
        band.color = colors[i % colors.len()];
    }
    // Every other scene ends with one narrow accent band on top
    if rng.coin(0.5) {
        let last = bands.last_mut().expect("at least two bands");
        last.width = last.width.min(ACCENT_WIDTH * height);
        last.color = palette.accent;
    }
    (wave, bands)
}

/// One band as quads between its upper and lower edge, left to right
fn ribbon(wave: &Wave, band: &Band, width: f64, height: f64) -> Vec<Polygon> {
    let m = 0.02 * height;
    let k = TAU / wave.wavelength;
    let center = |x: f64| {
        band.base + wave.drift * (x - width / 2.0) + band.amplitude * (k * x + wave.phase).sin()
    };
    let slope = |x: f64| wave.drift + band.amplitude * k * (k * x + wave.phase).cos();
    // Short enough that the polyline bends no more than about 2.3° at any
    // joint, which is where a curve stops looking cut from straight pieces
    let span = width + 2.0 * m;
    let curvature = band.amplitude * k * k;
    let n = ((curvature * span / 0.04).ceil() as usize).clamp(16, MAX_SEGMENTS);
    let edge = |x: f64| -> (Point, Point) {
        let y = center(x);
        // A vertical cut through a sloping band is longer than its width;
        // the cap keeps a steep stretch from ballooning
        let half = band.width * (1.0 + slope(x).powi(2)).sqrt().min(1.8) / 2.0;
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
    fn a_scene_is_two_to_five_bands_that_leave_room() {
        let h = 200.0;
        for seed in 0..200 {
            let palette = palette(seed);
            let (_, bands) = bands(&palette, &mut Rng::new(seed), 320.0, h);
            assert!((2..=5).contains(&bands.len()), "seed {seed}");
            let total: f64 = bands.iter().map(|b| b.width).sum();
            assert!(total <= MAX_TOTAL_WIDTH * h + 1e-9, "seed {seed}: {total}");
            for b in &bands {
                assert!(b.width >= MIN_WIDTH * h - 1e-9 && b.width <= MAX_WIDTH * h + 1e-9);
                if b.color == palette.accent {
                    assert!(b.width <= ACCENT_WIDTH * h + 1e-9, "seed {seed}");
                }
            }
            for (i, a) in bands.iter().enumerate() {
                for b in &bands[i + 1..] {
                    let apart = (a.base - b.base).abs() - (a.width + b.width) / 2.0;
                    let play = 2.0 * AMPLITUDE_PLAY * (a.amplitude.max(b.amplitude));
                    assert!(
                        apart - play >= 0.0,
                        "seed {seed}: bands {apart} apart, {play} play"
                    );
                }
            }
        }
    }

    #[test]
    fn the_quads_stay_few_and_never_thin_out() {
        let h = 200.0;
        for seed in 0..100 {
            let polygons = flow(&palette(seed), &mut Rng::new(seed), 320.0, h);
            assert!(polygons.len() <= 1 + 5 * MAX_SEGMENTS, "seed {seed}");
            for p in &polygons[1..] {
                let [u0, u1, l1, l0] = p.points[..] else {
                    panic!("seed {seed}: {:?}", p.points)
                };
                assert!(l0[1] - u0[1] >= MIN_WIDTH * h - 1e-9, "seed {seed}");
                assert!(l1[1] - u1[1] >= MIN_WIDTH * h - 1e-9, "seed {seed}");
            }
        }
    }
}
