//! OKLab colors. Mixing and shading happen here rather than in sRGB, where
//! midpoints turn muddy and "lighter" does not look evenly lighter.
//!
//! The matrices are Björn Ottosson's reference values, kept digit for digit
//! so they can be checked against the source.
#![allow(clippy::excessive_precision)]

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

fn decode(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light to sRGB-encoded, both in [0, 1]
pub fn encode(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

impl Lab {
    pub const fn new(l: f32, a: f32, b: f32) -> Lab {
        Lab { l, a, b }
    }

    pub fn from_srgb(rgb: [u8; 3]) -> Lab {
        let [r, g, b] = rgb.map(|c| decode(c as f32 / 255.0));
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        Lab::new(
            0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
        )
    }

    /// Linear-light RGB, not clamped: a color outside the sRGB gamut comes
    /// out with components below 0 or above 1
    pub fn to_linear(self) -> [f32; 3] {
        let l = (self.l + 0.3963377774 * self.a + 0.2158037573 * self.b).powi(3);
        let m = (self.l - 0.1055613458 * self.a - 0.0638541728 * self.b).powi(3);
        let s = (self.l - 0.0894841775 * self.a - 1.2914855480 * self.b).powi(3);
        [
            4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
        ]
    }

    pub fn to_srgb(self) -> [u8; 3] {
        self.to_linear().map(|c| (encode(c) * 255.0).round() as u8)
    }

    pub fn mix(self, other: Lab, t: f32) -> Lab {
        Lab::new(
            self.l + (other.l - self.l) * t,
            self.a + (other.a - self.a) * t,
            self.b + (other.b - self.b) * t,
        )
    }

    /// The same hue at lightness `l`, with chroma at most `cap`
    pub fn level(self, l: f32, cap: f32) -> Lab {
        let chroma = self.a.hypot(self.b);
        let k = if chroma > cap { cap / chroma } else { 1.0 };
        Lab::new(l, self.a * k, self.b * k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_survives_a_round_trip_through_oklab() {
        for rgb in [
            [0, 0, 0],
            [255, 255, 255],
            [246, 242, 238],
            [165, 34, 47],
            [40, 72, 169],
        ] {
            assert_eq!(Lab::from_srgb(rgb).to_srgb(), rgb);
        }
    }

    #[test]
    fn mix_moves_linearly_from_one_color_to_the_other() {
        let (x, y) = (Lab::new(0.2, 0.1, -0.1), Lab::new(0.8, -0.1, 0.1));
        assert_eq!(x.mix(y, 0.0), x);
        assert_eq!(x.mix(y, 1.0), y);
        assert_eq!(x.mix(y, 0.5), Lab::new(0.5, 0.0, 0.0));
    }

    #[test]
    fn leveling_sets_lightness_and_caps_chroma_but_keeps_hue() {
        let red = Lab::from_srgb([165, 34, 47]);
        let leveled = red.level(0.78, 0.05);
        assert_eq!(leveled.l, 0.78);
        assert!((leveled.a.hypot(leveled.b) - 0.05).abs() < 1e-6);
        assert!((leveled.b.atan2(leveled.a) - red.b.atan2(red.a)).abs() < 1e-5);
    }

    #[test]
    fn leveling_leaves_a_duller_color_as_dull_as_it_was() {
        let grey = Lab::new(0.3, 0.01, 0.0);
        assert_eq!(grey.level(0.5, 0.05), Lab::new(0.5, 0.01, 0.0));
    }
}
