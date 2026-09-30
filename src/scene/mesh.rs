//! Low-poly relief: a jittered grid of points with random heights,
//! triangulated and flat-shaded. Color comes from a smooth field sampled
//! once per triangle, so every triangle stays one flat color.

use std::f64::consts::TAU;

use crate::color::Lab;
use crate::palette::Palette;
use crate::raster::{Point, Polygon};
use crate::rng::Rng;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

#[derive(Clone, Copy, PartialEq)]
pub enum Relief {
    /// Deep facets under a linear gradient across the screen
    Strong,
    /// Shallow facets under colors blended around a pale center
    Faint,
}

pub fn mesh(
    palette: &Palette,
    rng: &mut Rng,
    width: f64,
    height: f64,
    relief: Relief,
) -> Vec<Polygon> {
    let strong = relief == Relief::Strong;
    let rows = 5 + rng.below(4);
    let cell = height / rows as f64;
    let depth = if strong {
        rng.range(0.35, 0.6)
    } else {
        rng.range(0.12, 0.22)
    };
    // One ring of cells outside the image, so jitter never uncovers an edge
    let (gw, gh) = ((width / cell).ceil() as usize + 3, rows + 3);
    let verts: Vec<V3> = (0..gw * gh)
        .map(|k| {
            let (i, j) = ((k % gw) as f64 - 1.0, (k / gw) as f64 - 1.0);
            [
                (i + rng.range(-0.33, 0.33)) * cell,
                (j + rng.range(-0.33, 0.33)) * cell,
                rng.range(-1.0, 1.0) * depth * cell,
            ]
        })
        .collect();

    let [h0, h1, h2] = palette.hues;
    // Mostly vertical, like the light of a sky
    let axis = TAU / 4.0 + rng.range(-0.45, 0.45);
    let spots = [
        (
            [width * rng.range(0.0, 0.25), height * rng.range(0.0, 1.0)],
            h0,
        ),
        (
            [width * rng.range(0.75, 1.0), height * rng.range(0.0, 0.5)],
            h1,
        ),
        (
            [width * rng.range(0.3, 1.0), height * rng.range(0.8, 1.0)],
            h2,
        ),
        (
            [width * rng.range(0.3, 0.6), height * rng.range(0.35, 0.6)],
            palette.glow,
        ),
    ];
    let field = |p: Point| -> Lab {
        if strong {
            let s = ((p[0] - width / 2.0) * axis.cos() + (p[1] - height / 2.0) * axis.sin())
                / height
                + 0.5;
            let s = s.clamp(0.0, 1.0) as f32 * 2.0;
            return if s < 1.0 {
                h0.mix(h1, s)
            } else {
                h1.mix(h2, s - 1.0)
            };
        }
        // Inverse-distance blend; the constant keeps a spot from becoming a dot
        let (mut sum, mut total) = (Lab::new(0.0, 0.0, 0.0), 0.0);
        for (q, c) in spots {
            let k = (1.0
                / ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (0.15 * height).powi(2)))
                as f32;
            total += k;
            sum = sum.mix(c, k / total);
        }
        sum
    };

    // Image space: y down, z toward the viewer, light from the upper left
    let light = unit([-0.5, -0.6, 0.62]);
    let gain = if strong { 0.38 } else { 0.35 };
    let mut polygons = vec![];
    for j in 0..gh - 1 {
        for i in 0..gw - 1 {
            let q = [
                verts[j * gw + i],
                verts[j * gw + i + 1],
                verts[(j + 1) * gw + i + 1],
                verts[(j + 1) * gw + i],
            ];
            // The shorter diagonal, unless jitter bent the quad so that
            // diagonal runs outside it
            let turn = |t: &[V3; 3]| cross(sub(t[1], t[0]), sub(t[2], t[0]))[2];
            let inside = |s: &[[V3; 3]; 2]| turn(&s[0]) * turn(&s[1]) > 0.0;
            let len = |p: V3, q: V3| (p[0] - q[0]).hypot(p[1] - q[1]);
            let a = [[q[0], q[1], q[2]], [q[0], q[2], q[3]]];
            let b = [[q[0], q[1], q[3]], [q[1], q[2], q[3]]];
            let split = if inside(&a) && (!inside(&b) || len(q[0], q[2]) < len(q[1], q[3])) {
                a
            } else {
                b
            };
            for t in split {
                let mut n = unit(cross(sub(t[1], t[0]), sub(t[2], t[0])));
                if n[2] < 0.0 {
                    n = [-n[0], -n[1], -n[2]];
                }
                let c = field([
                    (t[0][0] + t[1][0] + t[2][0]) / 3.0,
                    (t[0][1] + t[1][1] + t[2][1]) / 3.0,
                ]);
                // A face tilted toward the light is lighter than a flat one
                let l = c.l + (gain * (dot(n, light) - light[2])) as f32;
                polygons.push(Polygon {
                    points: t.iter().map(|v| [v[0], v[1]]).collect(),
                    color: Lab::new(l.clamp(0.03, 0.99), c.a, c.b),
                });
            }
        }
    }
    polygons
}
