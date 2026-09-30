//! base16 schemes. The YAML is a list of `baseXX: "rrggbb"` lines, so it is
//! read line by line rather than through a YAML parser. The same reading also
//! covers the newer tinted-theming layout, which nests those lines under
//! `palette:` and writes the hex with a `#`.

use anyhow::{Context, Result, bail};

use crate::color::Lab;

#[derive(Clone, Debug, PartialEq)]
pub struct Scheme {
    pub base: [Lab; 16],
}

impl Scheme {
    pub fn parse(src: &str) -> Result<Scheme> {
        let mut base = [None; 16];
        for line in src.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let Some(idx) = key
                .trim()
                .strip_prefix("base")
                .and_then(|h| usize::from_str_radix(h, 16).ok())
            else {
                continue;
            };
            if idx >= 16 {
                continue;
            }
            let hex = value.split_whitespace().next().unwrap_or("");
            let hex = hex
                .trim_matches(|c| c == '"' || c == '\'')
                .trim_start_matches('#');
            let rgb = parse_hex(hex)
                .with_context(|| format!("base{idx:02X}: {:?} is not a hex color", value.trim()))?;
            base[idx] = Some(Lab::from_srgb(rgb));
        }
        let mut out = [Lab::new(0.0, 0.0, 0.0); 16];
        for (idx, color) in base.iter().enumerate() {
            match color {
                Some(c) => out[idx] = *c,
                None => bail!("base{idx:02X} is missing"),
            }
        }
        Ok(Scheme { base: out })
    }
}

fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    if hex.len() != 6 {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::DAYFOX;

    #[test]
    fn an_empty_file_is_not_a_scheme() {
        assert!(Scheme::parse("").is_err());
    }

    #[test]
    fn a_flat_scheme_gives_all_sixteen_colors() {
        let scheme = Scheme::parse(DAYFOX).unwrap();
        assert_eq!(scheme.base[0x0].to_srgb(), [0xf6, 0xf2, 0xee]);
        assert_eq!(scheme.base[0xA].to_srgb(), [0xac, 0x54, 0x02]);
        assert_eq!(scheme.base[0xF].to_srgb(), [0xa4, 0x40, 0xb5]);
    }

    #[test]
    fn the_nested_tinted_theming_layout_reads_the_same() {
        let nested: String = DAYFOX
            .lines()
            .map(|l| match l.split_once(": \"") {
                Some((k, v)) if k.starts_with("base") => {
                    format!("  {k}: '#{}  # comment\n", v.trim_end_matches('"'))
                }
                _ => format!("{l}\n"),
            })
            .collect();
        let nested = nested.replacen("base00", "palette:\n  base00", 1);
        assert_eq!(
            Scheme::parse(&nested).unwrap(),
            Scheme::parse(DAYFOX).unwrap()
        );
    }

    #[test]
    fn a_missing_color_is_named_in_the_error() {
        let err = Scheme::parse(&DAYFOX.replace("base0C", "notbase")).unwrap_err();
        assert_eq!(err.to_string(), "base0C is missing");
    }

    #[test]
    fn a_malformed_hex_is_an_error() {
        assert!(Scheme::parse(&DAYFOX.replace("a5222f", "a5222")).is_err());
        assert!(Scheme::parse(&DAYFOX.replace("a5222f", "zz222f")).is_err());
    }
}
