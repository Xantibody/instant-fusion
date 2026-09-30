//! Paints flat convex polygons back to front into an RGB image.
//!
//! Each pixel is split into SS×SS samples. The first pass records which
//! polygon owns each sample; the second averages the owners' colors in
//! linear light. Keeping ids instead of colors keeps the buffer small, and
//! most pixels have one owner, which then costs one color conversion.

use crate::color::{Lab, LabExt, encode};

pub type Point = [f64; 2];

#[derive(Clone, Debug, PartialEq)]
pub struct Polygon {
    /// Convex, in either winding; may reach outside the image
    pub points: Vec<Point>,
    pub color: Lab,
}

const SS: usize = 4;

/// 8-bit RGB rows, top to bottom. Pixels no polygon covers are black
pub fn render(polygons: &[Polygon], width: usize, height: usize) -> Vec<u8> {
    // Id 0 is "no polygon"
    assert!(
        polygons.len() < u16::MAX as usize,
        "too many polygons: {}",
        polygons.len()
    );
    let (sw, sh) = (width * SS, height * SS);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());

    let mut ids = vec![0u16; sw * sh];
    let rows_per = sh.div_ceil(threads).max(1);
    std::thread::scope(|sc| {
        for (band_idx, band) in ids.chunks_mut(rows_per * sw).enumerate() {
            sc.spawn(move || {
                for (i, p) in polygons.iter().enumerate() {
                    fill(band, sw, band_idx * rows_per, &p.points, i as u16 + 1);
                }
            });
        }
    });

    let linear: Vec<[f32; 3]> = std::iter::once([0.0; 3])
        .chain(polygons.iter().map(|p| p.color.to_linear()))
        .collect();
    let (ids, linear) = (&ids, &linear);
    let mut img = vec![0u8; width * height * 3];
    let rows_per = height.div_ceil(threads).max(1);
    std::thread::scope(|sc| {
        for (band_idx, band) in img.chunks_mut(rows_per * width * 3).enumerate() {
            sc.spawn(move || {
                for (r, row) in band.chunks_mut(width * 3).enumerate() {
                    let y = band_idx * rows_per + r;
                    for x in 0..width {
                        let sample =
                            |k: usize| ids[(y * SS + k / SS) * sw + x * SS + k % SS] as usize;
                        let first = sample(0);
                        let c = if (1..SS * SS).all(|k| sample(k) == first) {
                            linear[first]
                        } else {
                            let mut acc = [0f32; 3];
                            for k in 0..SS * SS {
                                for (a, c) in acc.iter_mut().zip(linear[sample(k)]) {
                                    *a += c / (SS * SS) as f32;
                                }
                            }
                            acc
                        };
                        for (out, c) in row[x * 3..x * 3 + 3].iter_mut().zip(c) {
                            *out = (encode(c) * 255.0).round() as u8;
                        }
                    }
                }
            });
        }
    });
    img
}

/// Scanline fill of one convex polygon into the band of sample rows that
/// starts at row `y0`. A sample belongs to the polygon when its center does
fn fill(band: &mut [u16], sw: usize, y0: usize, points: &[Point], id: u16) {
    let s = SS as f64;
    let rows = band.len() / sw;
    let (ymin, ymax) = points.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
        (lo.min(p[1] * s), hi.max(p[1] * s))
    });
    let first = (ymin - 0.5).ceil().max(y0 as f64);
    let last = (ymax - 0.5).floor().min((y0 + rows) as f64 - 1.0);
    if last < first {
        return;
    }
    for row in first as usize..=last as usize {
        let yc = row as f64 + 0.5;
        let (mut xl, mut xr) = (f64::MAX, f64::MIN);
        for (i, a) in points.iter().enumerate() {
            let b = points[(i + 1) % points.len()];
            let (ay, by) = (a[1] * s, b[1] * s);
            if (ay <= yc) != (by <= yc) {
                let x = (a[0] + (yc - ay) / (by - ay) * (b[0] - a[0])) * s;
                xl = xl.min(x);
                xr = xr.max(x);
            }
        }
        let i0 = (xl - 0.5).ceil().max(0.0);
        let i1 = (xr - 0.5).floor().min(sw as f64 - 1.0);
        if i1 >= i0 {
            let o = (row - y0) * sw;
            band[o + i0 as usize..=o + i1 as usize].fill(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_to_paint_leaves_a_black_image() {
        assert_eq!(render(&[], 3, 2), vec![0; 3 * 2 * 3]);
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64, rgb: [u8; 3]) -> Polygon {
        Polygon {
            points: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            color: Lab::from_srgb(rgb),
        }
    }

    fn pixel(img: &[u8], width: usize, x: usize, y: usize) -> [u8; 3] {
        let o = (y * width + x) * 3;
        [img[o], img[o + 1], img[o + 2]]
    }

    #[test]
    fn a_polygon_over_the_whole_image_paints_every_pixel() {
        let img = render(&[rect(0.0, 0.0, 4.0, 3.0, [200, 100, 50])], 4, 3);
        assert!(img.chunks(3).all(|p| p == [200, 100, 50]));
    }

    #[test]
    fn a_later_polygon_paints_over_an_earlier_one() {
        let img = render(
            &[
                rect(0.0, 0.0, 4.0, 4.0, [255, 0, 0]),
                rect(1.0, 1.0, 3.0, 3.0, [0, 0, 255]),
            ],
            4,
            4,
        );
        assert_eq!(pixel(&img, 4, 0, 0), [255, 0, 0]);
        assert_eq!(pixel(&img, 4, 2, 2), [0, 0, 255]);
    }

    #[test]
    fn an_edge_through_a_pixel_blends_the_two_sides_in_linear_light() {
        let img = render(&[rect(0.0, 0.0, 1.5, 1.0, [255, 255, 255])], 3, 1);
        assert_eq!(pixel(&img, 3, 0, 0), [255, 255, 255]);
        assert_eq!(pixel(&img, 3, 2, 0), [0, 0, 0]);
        // Half the samples white: 0.5 in linear light, not 128 in sRGB
        assert_eq!(pixel(&img, 3, 1, 0), [188, 188, 188]);
    }

    #[test]
    fn a_triangle_reaching_far_outside_the_image_is_clipped() {
        let tri = Polygon {
            points: vec![[1.0, 1.0], [1e5, -3e4], [-2e4, 9e4]],
            color: Lab::from_srgb([10, 20, 30]),
        };
        let img = render(&[tri], 5, 5);
        assert_eq!(pixel(&img, 5, 4, 4), [10, 20, 30]);
        assert_eq!(pixel(&img, 5, 0, 0), [0, 0, 0]);
    }
}
