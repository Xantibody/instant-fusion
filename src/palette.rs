//! The colors a scene may use, derived from a scheme. The scheme gives
//! hues; lightness is fixed here, because base16 does not order its colors
//! by lightness and a scene shades by moving lightness alone.

use crate::color::Lab;
use crate::rng::Rng;
use crate::scheme::Scheme;

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// base00 is light. Sets the tone of the whole wallpaper
    pub bright: bool,
    /// Three different accents leveled to one lightness, for gradients
    pub hues: [Lab; 3],
    /// The pale point a gradient can fade toward
    pub glow: Lab,
    /// A barely colored hue for scenes in one color
    pub tint: Lab,
}

impl Palette {
    pub fn new(scheme: &Scheme, rng: &mut Rng) -> Palette {
        let bright = scheme.base[0].l > 0.6;
        let (l, cap) = if bright { (0.78, 0.11) } else { (0.40, 0.075) };
        // Spread over the accent wheel so the three never repeat
        let first = rng.below(8);
        let picks = [first, first + 1 + rng.below(3), first + 4 + rng.below(3)];
        let hues = picks.map(|i| scheme.base[8 + i % 8].level(l, cap));
        let glow = if bright {
            hues[1].level(0.95, 0.02)
        } else {
            hues[1].level(0.62, 0.04)
        };
        let tint = scheme.base[8 + rng.below(8)].level(l, 0.03);
        Palette {
            bright,
            hues,
            glow,
            tint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{dark, dayfox};

    #[test]
    fn a_light_scheme_gives_a_bright_palette() {
        assert!(Palette::new(&dayfox(), &mut Rng::new(1)).bright);
    }

    #[test]
    fn a_dark_scheme_gives_a_dim_palette() {
        let palette = Palette::new(&dark(), &mut Rng::new(1));
        assert!(!palette.bright);
        assert!(palette.hues.iter().all(|h| h.l < 0.5));
    }

    #[test]
    fn the_hues_are_three_different_accents_at_one_lightness() {
        let scheme = dayfox();
        let angle = |c: Lab| c.b.atan2(c.a);
        for seed in 0..50 {
            let palette = Palette::new(&scheme, &mut Rng::new(seed));
            let accents: Vec<usize> = palette
                .hues
                .iter()
                .map(|h| {
                    (8..16)
                        .position(|i| (angle(scheme.base[i]) - angle(*h)).abs() < 1e-4)
                        .expect("every hue is one of the accents")
                })
                .collect();
            assert!(
                accents[0] != accents[1] && accents[1] != accents[2] && accents[0] != accents[2]
            );
            assert!(palette.hues.iter().all(|h| h.l == palette.hues[0].l));
        }
    }

    #[test]
    fn the_glow_is_paler_than_the_hues() {
        for scheme in [dayfox(), dark()] {
            let palette = Palette::new(&scheme, &mut Rng::new(3));
            let chroma = |c: Lab| c.a.hypot(c.b);
            assert!(palette.glow.l > palette.hues[0].l);
            assert!(
                palette
                    .hues
                    .iter()
                    .all(|h| chroma(palette.glow) < chroma(*h))
            );
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_palette() {
        let scheme = dayfox();
        assert_eq!(
            Palette::new(&scheme, &mut Rng::new(9)),
            Palette::new(&scheme, &mut Rng::new(9))
        );
    }
}
