use std::path::PathBuf;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_instant-fusion"))
        .args(args)
        .output()
        .unwrap()
}

/// A fresh directory for one test's files
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("instant-fusion-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_missing_scheme_fails_and_says_so() {
    let dir = scratch("missing");
    let scheme = dir.join("nope.yaml");
    let out = run(&[
        "--scheme",
        scheme.to_str().unwrap(),
        "--size",
        "64x40",
        "-o",
        "-",
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope.yaml"));
}

const DAYFOX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/dayfox.yaml");

fn decode(png_bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    (info.width, info.height, buf)
}

#[test]
fn a_malformed_size_fails() {
    for size in ["1920", "0x1200", "axb", "1920x"] {
        let out = run(&["--scheme", DAYFOX, "--size", size, "-o", "-"]);
        assert_eq!(out.status.code(), Some(1), "{size}");
    }
}

#[test]
fn it_writes_a_png_of_the_asked_size() {
    let dir = scratch("size");
    let path = dir.join("out.png");
    let out = run(&[
        "--scheme",
        DAYFOX,
        "--size",
        "64x40",
        "--seed",
        "3",
        "-o",
        path.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (w, h, _) = decode(&std::fs::read(path).unwrap());
    assert_eq!((w, h), (64, 40));
}

#[test]
fn a_seed_reproduces_its_wallpaper_and_is_reported() {
    let args = [
        "--scheme", DAYFOX, "--size", "64x40", "--seed", "11", "-o", "-",
    ];
    let (a, b) = (run(&args), run(&args));
    assert_eq!(a.stdout, b.stdout);
    let stderr = String::from_utf8_lossy(&a.stderr);
    assert!(stderr.contains("seed=11"), "{stderr}");
    assert!(stderr.contains("kind="), "{stderr}");
}

#[test]
fn every_kind_can_be_asked_for() {
    for kind in ["flow", "facet"] {
        let out = run(&[
            "--scheme", DAYFOX, "--size", "64x40", "--kind", kind, "-o", "-",
        ]);
        assert!(out.status.success(), "{kind}");
        assert!(String::from_utf8_lossy(&out.stderr).contains(&format!("kind={kind}")));
        assert_eq!(decode(&out.stdout).0, 64);
    }
}
