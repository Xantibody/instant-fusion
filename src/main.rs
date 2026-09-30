use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::Parser;

use instant_fusion::scene::Kind;
use instant_fusion::scheme::Scheme;

/// Generate a geometric wallpaper from a base16 scheme
#[derive(Parser)]
#[command(version)]
struct Args {
    /// base16 scheme (YAML)
    #[arg(long)]
    scheme: PathBuf,
    /// Image size as WIDTHxHEIGHT
    #[arg(long, value_parser = parse_size)]
    size: (usize, usize),
    /// Seed; a random one when omitted
    #[arg(long)]
    seed: Option<u64>,
    /// Kind of wallpaper; picked from the seed when omitted
    #[arg(long, value_enum)]
    kind: Option<Kind>,
    /// Output PNG, or - for stdout
    #[arg(short, long)]
    output: PathBuf,
}

const MAX_SIDE: usize = 8192;

fn parse_size(s: &str) -> Result<(usize, usize)> {
    let (w, h) = s
        .split_once('x')
        .context("expected WIDTHxHEIGHT, e.g. 1920x1200")?;
    let (w, h): (usize, usize) = (w.parse()?, h.parse()?);
    if !(1..=MAX_SIDE).contains(&w) || !(1..=MAX_SIDE).contains(&h) {
        bail!("each side must be between 1 and {MAX_SIDE}");
    }
    Ok((w, h))
}

fn run(args: Args) -> Result<()> {
    let src = std::fs::read_to_string(&args.scheme)
        .with_context(|| format!("cannot read {}", args.scheme.display()))?;
    let scheme = Scheme::parse(&src)
        .with_context(|| format!("{} is not a base16 scheme", args.scheme.display()))?;
    let seed = args.seed.unwrap_or_else(random_seed);
    let kind = args.kind.unwrap_or_else(|| instant_fusion::pick_kind(seed));
    // Printed so a wallpaper worth keeping can be made again
    eprintln!("seed={seed} kind={}", kind.name());

    let (width, height) = args.size;
    let rgb = instant_fusion::generate(&scheme, kind, seed, width, height);
    if args.output.as_os_str() == "-" {
        let mut out = std::io::stdout().lock();
        instant_fusion::write_png(&mut out, &rgb, width, height)?;
        out.flush()?;
    } else {
        let file = File::create(&args.output)
            .with_context(|| format!("cannot write {}", args.output.display()))?;
        instant_fusion::write_png(BufWriter::new(file), &rgb, width, height)?;
    }
    Ok(())
}

fn random_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    nanos ^ (std::process::id() as u64).rotate_left(32)
}

fn main() -> ExitCode {
    // Every failure exits 1, argument errors included: hyprpaper's
    // ExecStartPre only needs to see that it failed
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(e) => {
            let _ = e.print();
            return if e.use_stderr() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("instant-fusion: {e:#}");
            ExitCode::FAILURE
        }
    }
}
