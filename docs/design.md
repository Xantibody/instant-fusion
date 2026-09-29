# instant-fusion design

A CLI that generates one mathematical wallpaper from a base16 scheme. The
dotfiles' hyprpaper service calls it at every login, so each session starts
with a fresh wallpaper that matches the Stylix scheme.

## Pipeline

```
base16.yaml ─┐
seed ────────┼─→ generator ─→ Field ─→ colorizer ─→ PNG
size ────────┘   (math)                 (palette)
```

- generator: returns a field `(x, y) → f32 in [0,1]`. It knows nothing about color
- colorizer: maps field values to RGB. It knows nothing about the math
- Adding a pattern touches only the generator side; tuning colors touches only
  the colorizer side

## CLI

```
instant-fusion --scheme <base16.yaml> --size 1920x1200 [--seed N] [--kind flow|contour|voronoi] -o <out.png|->
```

- Without `--seed`, a random seed is used. The seed and kind are printed to
  stderr so a wallpaper worth keeping can be reproduced
- Without `--kind`, the kind is picked from the seed
- `-o -` writes the PNG to stdout
- On failure, exit 1 with a message on stderr (this stops hyprpaper's
  ExecStartPre)

## Colors

- base16 does not guarantee that base00→07 are ordered by lightness (in dayfox,
  base03 `534c45` is dark while base07 `f4ece6` is light). base00–07 are
  re-sorted by OKLab L before use
- Most of the screen stays close to the background (base00); accents
  (base08–0F) appear only as lines or small regions
- Interpolate in OKLab (sRGB interpolation muddies the midpoints)

## Generators (first three)

| kind      | math                        | look                 |
| --------- | --------------------------- | -------------------- |
| `flow`    | fBm + domain warping        | fluid, marble        |
| `contour` | contour lines of noise      | topographic map      |
| `voronoi` | Worley noise / Voronoi cells | cells, stained glass |

Deferred: strange attractors (their iteration count would delay login),
Truchet tiles, Julia sets.

## Performance

- Target: under 1 second at 1920×1200 (hyprpaper waits during ExecStartPre)
- Parallelize by rows with `std::thread::scope`

## Technology

- Rust. Dependencies limited to `clap` (derive), `png` and `anyhow`
- The base16 YAML is a flat list of `key: "hex"`, so it is parsed by hand
  (no serde_yaml)
- The seed must reproduce the same image, so any RNG is a deterministic one
  such as `rand_chacha`. Noise is implemented in-house (hash-based value /
  gradient noise is enough)
- The flake uses flake-parts + rust-overlay. Besides the devShell it exposes
  `packages.default` via `rustPlatform.buildRustPackage`. No home-manager
  module

## dotfiles integration

- Add as a flake input and replace `pick-wallpaper` in
  `modules/desktop/hyprland/hyprland.nix`:
  ```nix
  ExecStartPre = "${lib.getExe inputs.instant-fusion.packages.${system}.default} --scheme ${config.stylix.base16Scheme} --size 1920x1200 -o ${wallpaper}";
  ```
- The runCommand copying KDE wallpapers (a 255MB package) is no longer needed

## Testing

- Colors: base16 parsing, OKLab round trip, lightness sort
- Generators: same seed → same field (determinism), values within [0,1]
- CLI: invalid `--size` or a missing scheme exits non-zero
