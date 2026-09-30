//! Colors live in OKLab, palette's `Oklab`, where mixing and shading stay
//! even; sRGB is only the input and output format. The conversions are
//! palette's. This module adds the polar helpers the scene code speaks
//! in, so a caller never handles `a` and `b` directly.

use palette::encoding::FromLinear;
use palette::{FromColor, IntoColor, LinSrgb, Oklab, Oklch, ShiftHue, Srgb};

pub type Lab = Oklab;

/// Linear light to sRGB-encoded, both in [0, 1]
pub fn encode(c: f32) -> f32 {
    palette::encoding::Srgb::from_linear(c.clamp(0.0, 1.0))
}

pub trait LabExt: Sized {
    fn from_srgb(rgb: [u8; 3]) -> Self;
    /// Linear-light RGB, not clamped: a color outside the sRGB gamut comes
    /// out with components below 0 or above 1
    fn to_linear(self) -> [f32; 3];
    fn to_srgb(self) -> [u8; 3];
    /// Distance from grey
    fn chroma(self) -> f32;
    /// Hue angle in radians; 0 for a grey
    fn hue(self) -> f32;
    /// The same lightness and chroma, hue turned by `angle` radians
    fn rotate_hue(self, angle: f32) -> Self;
    /// The same hue and lightness at exactly `chroma`. A grey has no hue
    /// to keep, so it stays grey
    fn with_chroma(self, chroma: f32) -> Self;
    fn with_lightness(self, l: f32) -> Self;
}

impl LabExt for Lab {
    fn from_srgb(rgb: [u8; 3]) -> Lab {
        let [r, g, b] = rgb;
        Lab::from_color(Srgb::new(r, g, b).into_format::<f32>().into_linear())
    }

    fn to_linear(self) -> [f32; 3] {
        let c: LinSrgb = self.into_color();
        [c.red, c.green, c.blue]
    }

    fn to_srgb(self) -> [u8; 3] {
        self.to_linear().map(|c| (encode(c) * 255.0).round() as u8)
    }

    fn chroma(self) -> f32 {
        Oklch::from_color(self).chroma
    }

    fn hue(self) -> f32 {
        Oklch::from_color(self).hue.into_radians()
    }

    fn rotate_hue(self, angle: f32) -> Lab {
        Lab::from_color(Oklch::from_color(self).shift_hue(angle.to_degrees()))
    }

    fn with_chroma(self, chroma: f32) -> Lab {
        if self.chroma() == 0.0 {
            return self;
        }
        let mut c = Oklch::from_color(self);
        c.chroma = chroma;
        Lab::from_color(c)
    }

    fn with_lightness(self, l: f32) -> Lab {
        Lab { l, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use palette::Mix;

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
}

#[cfg(test)]
mod polar_tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn chroma_and_hue_read_the_polar_form_of_ab() {
        let c = Lab::new(0.5, 0.0, 0.1);
        assert!((c.chroma() - 0.1).abs() < 1e-6);
        assert!((c.hue() - PI / 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_grey_has_no_chroma_and_keeps_its_hue_at_zero() {
        let grey = Lab::new(0.5, 0.0, 0.0);
        assert_eq!(grey.chroma(), 0.0);
        assert_eq!(grey.hue(), 0.0);
    }

    #[test]
    fn rotating_the_hue_keeps_lightness_and_chroma() {
        let c = Lab::new(0.5, 0.1, 0.0).rotate_hue(PI / 2.0);
        assert_eq!(c.l, 0.5);
        assert!((c.chroma() - 0.1).abs() < 1e-6);
        assert!((c.hue() - PI / 2.0).abs() < 1e-6);
    }

    #[test]
    fn with_chroma_scales_ab_to_the_asked_chroma_and_keeps_hue() {
        let c = Lab::new(0.5, 0.03, 0.04).with_chroma(0.1);
        assert!((c.chroma() - 0.1).abs() < 1e-6);
        assert!((c.hue() - Lab::new(0.5, 0.03, 0.04).hue()).abs() < 1e-6);
    }

    #[test]
    fn with_chroma_on_a_grey_stays_grey() {
        assert_eq!(
            Lab::new(0.5, 0.0, 0.0).with_chroma(0.1),
            Lab::new(0.5, 0.0, 0.0)
        );
    }

    #[test]
    fn with_lightness_changes_only_l() {
        assert_eq!(
            Lab::new(0.5, 0.03, 0.04).with_lightness(0.8),
            Lab::new(0.8, 0.03, 0.04)
        );
    }
}
