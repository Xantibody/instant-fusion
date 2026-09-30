pub mod color;
#[cfg(test)]
mod fixtures;
pub mod palette;
pub mod raster;
pub mod rng;
pub mod scene;
pub mod scheme;

use std::io::Write;

use anyhow::Result;

use palette::Palette;
use rng::Rng;
use scene::Kind;
use scheme::Scheme;

// Each consumer of a seed draws from its own stream, so a change in how
// many numbers one takes never shifts what the others get
const KIND_STREAM: u64 = 0x1c1d;
const PALETTE_STREAM: u64 = 0xc010;
const SCENE_STREAM: u64 = 0x9017;

/// The kind a seed picks when none is asked for
pub fn pick_kind(seed: u64) -> Kind {
    Kind::ALL[Rng::new(seed ^ KIND_STREAM).below(Kind::ALL.len())]
}

/// 8-bit RGB rows of one wallpaper
pub fn generate(scheme: &Scheme, kind: Kind, seed: u64, width: usize, height: usize) -> Vec<u8> {
    let mut prng = Rng::new(seed ^ PALETTE_STREAM);
    let palette = Palette::new(scheme, kind.harmony(&mut prng), &mut prng);

    let polygons = scene::compose(
        kind,
        &palette,
        &mut Rng::new(seed ^ SCENE_STREAM),
        width as f64,
        height as f64,
    );
    raster::render(&polygons, width, height)
}

pub fn write_png(out: impl Write, rgb: &[u8], width: usize, height: usize) -> Result<()> {
    let mut encoder = png::Encoder::new(out, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgb)?;
    Ok(())
}
