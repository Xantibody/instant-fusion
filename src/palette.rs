//! The colors a scene may use. The scheme is only an anchor: one of its
//! accents gives the hue and base00 gives the tone. Everything else is
//! built from that in OKLab, so a loud scheme still yields a muted,
//! harmonic set, and a color the scheme does not contain (a beige next to
//! a navy) is allowed to appear.

use std::f32::consts::PI;

use crate::color::Lab;
use crate::rng::Rng;
use crate::scheme::Scheme;

/// How the colors relate to the anchor hue
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harmony {
    /// One hue; the colors differ in lightness only
    Monochromatic,
    /// Neighboring hues, each within about 45° of the anchor
    Analogous,
    /// A neighboring hue, plus a dull accent from across the wheel
    MutedComplementary,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// base00 is light. Sets the tone of the whole wallpaper
    pub bright: bool,
    pub harmony: Harmony,
    /// The quiet ground most of the image shows
    pub background: Lab,
    /// The main figure color
    pub primary: Lab,
    /// A quieter figure color, between the ground and the primary
    pub secondary: Lab,
    /// For small areas only: a scene keeps it under about 15% of the image
    pub accent: Lab,
}

/// Loud colors are never wanted, whatever the scheme
const MAX_CHROMA: f32 = 0.07;

fn radians(degrees: f32) -> f32 {
    degrees * PI / 180.0
}

impl Palette {
    pub fn new(scheme: &Scheme, harmony: Harmony, rng: &mut Rng) -> Palette {
        let bright = scheme.base[0].l > 0.6;
        let anchor = scheme.base[8 + rng.below(8)];
        let jitter = rng.range(-0.02, 0.02) as f32;
        // Lightness steps outward from the background, in the order the
        // colors are meant to carry weight: the secondary is the quietest
        // figure, the primary the main one, the accent the strongest and
        // the smallest. Darker than a pale ground, lighter than a dim one
        let (bg, p, s, a) = if bright {
            (0.91, 0.70, 0.80, 0.58)
        } else {
            (0.21, 0.38, 0.30, 0.52)
        };
        let sign = if rng.coin(0.5) { 1.0 } else { -1.0 };
        let (turn_s, turn_a, chroma_a) = match harmony {
            Harmony::Monochromatic => (0.0, 0.0, 0.06),
            Harmony::Analogous => {
                let s = sign * rng.range(25.0, 45.0) as f32;
                (s, -sign * rng.range(25.0, 45.0) as f32, MAX_CHROMA)
            }
            Harmony::MutedComplementary => (
                sign * rng.range(20.0, 35.0) as f32,
                180.0 + rng.range(-25.0, 25.0) as f32,
                0.045,
            ),
        };
        // On a dim ground a colorful accent glows; keep it calmer there
        let chroma_a = if bright { chroma_a } else { chroma_a * 0.75 };
        let color = |l: f32, turn: f32, chroma: f32| {
            anchor
                .rotate_hue(radians(turn))
                .with_chroma(chroma.min(MAX_CHROMA))
                .with_lightness(l + jitter)
        };
        Palette {
            bright,
            harmony,
            background: color(bg, 0.0, if bright { 0.02 } else { 0.025 }),
            primary: color(p, 0.0, 0.055),
            secondary: color(s, turn_s, 0.05),
            accent: color(a, turn_a, chroma_a),
        }
    }

    /// Every color, background first
    pub fn colors(&self) -> [Lab; 4] {
        [self.background, self.primary, self.secondary, self.accent]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{dark, dayfox};

    const HARMONIES: [Harmony; 3] = [
        Harmony::Monochromatic,
        Harmony::Analogous,
        Harmony::MutedComplementary,
    ];

    /// Signed hue difference from `a` to `b` in degrees, in (-180, 180]
    fn turn(a: Lab, b: Lab) -> f32 {
        let d = (b.hue() - a.hue()).rem_euclid(2.0 * PI);
        let d = d * 180.0 / PI;
        if d > 180.0 { d - 360.0 } else { d }
    }

    fn all_palettes() -> impl Iterator<Item = Palette> {
        [dayfox(), dark()].into_iter().flat_map(|scheme| {
            HARMONIES.into_iter().flat_map(move |harmony| {
                let scheme = scheme.clone();
                (0..40).map(move |seed| Palette::new(&scheme, harmony, &mut Rng::new(seed)))
            })
        })
    }

    #[test]
    fn a_light_scheme_gives_a_pale_ground_and_a_dark_scheme_a_dim_one() {
        let pale = Palette::new(&dayfox(), Harmony::Analogous, &mut Rng::new(1));
        assert!(pale.bright && pale.background.l > 0.85);
        let dim = Palette::new(&dark(), Harmony::Analogous, &mut Rng::new(1));
        assert!(!dim.bright && dim.background.l < 0.3);
    }

    #[test]
    fn the_primary_takes_its_hue_from_one_of_the_accents() {
        let scheme = dayfox();
        for palette in all_palettes() {
            assert!(
                (8..16).any(|i| turn(scheme.base[i], palette.primary).abs() < 0.01),
                "{palette:?}"
            );
        }
    }

    #[test]
    fn no_color_is_ever_loud() {
        for palette in all_palettes() {
            for c in palette.colors() {
                assert!(c.chroma() <= MAX_CHROMA + 1e-6, "{c:?}");
            }
        }
    }

    #[test]
    fn a_monochromatic_palette_keeps_one_hue() {
        for palette in all_palettes().filter(|p| p.harmony == Harmony::Monochromatic) {
            for c in palette.colors() {
                assert!(turn(palette.primary, c).abs() < 0.01, "{palette:?}");
            }
        }
    }

    #[test]
    fn an_analogous_palette_puts_the_secondary_and_accent_on_either_side() {
        for palette in all_palettes().filter(|p| p.harmony == Harmony::Analogous) {
            let s = turn(palette.primary, palette.secondary);
            let a = turn(palette.primary, palette.accent);
            assert!((20.0..=50.0).contains(&s.abs()), "{s}");
            assert!((20.0..=50.0).contains(&a.abs()), "{a}");
            assert!(s.signum() != a.signum());
        }
    }

    #[test]
    fn a_muted_complementary_accent_sits_across_the_wheel_and_stays_dull() {
        for palette in all_palettes().filter(|p| p.harmony == Harmony::MutedComplementary) {
            let a = turn(palette.primary, palette.accent).abs();
            assert!((150.0..=180.0).contains(&a), "{a}");
            assert!(palette.accent.chroma() <= 0.05);
            let s = turn(palette.primary, palette.secondary).abs();
            assert!((15.0..=40.0).contains(&s), "{s}");
        }
    }

    #[test]
    fn lightness_steps_from_the_ground_through_secondary_and_primary_to_accent() {
        for palette in all_palettes() {
            let [bg, p, s, a] = palette.colors().map(|c| c.l);
            let off = |c: f32| (c - bg).abs();
            assert!(off(s) >= 0.08 && off(p) - off(s) >= 0.06, "{palette:?}");
            assert!(off(a) - off(p) >= 0.06, "{palette:?}");
        }
    }

    #[test]
    fn the_accent_is_calmer_on_a_dark_scheme() {
        for harmony in HARMONIES {
            let pale = Palette::new(&dayfox(), harmony, &mut Rng::new(4));
            let dim = Palette::new(&dark(), harmony, &mut Rng::new(4));
            assert!(dim.accent.chroma() < pale.accent.chroma(), "{harmony:?}");
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_palette() {
        let scheme = dayfox();
        assert_eq!(
            Palette::new(&scheme, Harmony::Analogous, &mut Rng::new(9)),
            Palette::new(&scheme, Harmony::Analogous, &mut Rng::new(9))
        );
    }
}
