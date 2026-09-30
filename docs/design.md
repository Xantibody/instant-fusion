# instant-fusion design

A CLI that generates one geometric wallpaper from a base16 scheme. The
dotfiles' hyprpaper service calls it at every login, so each session starts
with a fresh wallpaper that matches the Stylix scheme.

## Pipeline

```
base16.yaml ─→ palette ─┐
seed ───────────────────┼─→ scene ─→ polygons ─→ rasterizer ─→ PNG
size ───────────────────┘   (kind)
```

- scene: returns an ordered list of flat, convex 2D polygons, each with one
  color. Depth is only ever suggested by the lightness a polygon is given
- rasterizer: paints the list back to front. It knows nothing about scenes
- Adding a kind touches only the scene side; the rasterizer stays a plain
  polygon filler

An earlier draft had generators return a scalar field `(x, y) → [0,1]` that a
colorizer mapped to color. That fits noise-like patterns, but the wallpapers
wanted here are crisp faceted surfaces: their edges are the picture, and a
field sampled per pixel can only approximate an edge.

## CLI

```
instant-fusion --scheme <base16.yaml> --size 1920x1200 [--seed N] [--kind facet|terrain|lowpoly] -o <out.png|->
```

- Without `--seed`, a random seed is used. The seed and kind are printed to
  stderr so a wallpaper worth keeping can be reproduced
- Without `--kind`, the kind is picked from the seed
- `-o -` writes the PNG to stdout
- On failure, exit 1 with a message on stderr (this stops hyprpaper's
  ExecStartPre)

## Colors

- The scheme supplies hues, not lightness. base16 does not order its colors
  by lightness (in dayfox, base03 `534c45` is dark while base07 `f4ece6` is
  light), so a scene that read lightness off the scheme would shade its faces
  inconsistently. Accents (base08–0F) are leveled to one lightness and their
  chroma is capped; shading then moves lightness only
- The tone follows base00: a light scheme gives a pale pastel wallpaper, a
  dark scheme a dim one
- Mix in OKLab (sRGB interpolation muddies the midpoints)

## Scenes

All three fill the screen with triangles.

| kind      | construction                                                  | look                                  |
| --------- | ------------------------------------------------------------- | ------------------------------------- |
| `facet`   | one hue; triangles split from a vertex to the opposite edge   | paneled facade with seams, sunk panels |
| `terrain` | jittered grid with random heights, flat-shaded; linear colors | strong low-poly relief                |
| `lowpoly` | same mesh, faint relief; colors blended around a pale center  | soft low-poly gradient                |

- `facet` does not carry a split into the neighboring triangle. The
  T-junctions this leaves are what make it read as panels rather than a mesh
- The mesh kinds sample their color field once per triangle, so every
  polygon stays flat

Deferred: isometric solids with cast shadows, constructivist compositions
(both worked in the proof of concept but are not the look wanted first), and
a Delaunay triangulation for less regular triangle sizes.

## Performance

- Target: under 1 second at 1920×1200 (hyprpaper waits during ExecStartPre)
- The rasterizer records which polygon owns each of 4×4 samples per pixel,
  then averages in linear light. Both passes are split by rows over
  `std::thread::scope`

## Technology

- Rust. Dependencies limited to `clap` (derive), `png` and `anyhow`
- The base16 YAML is a flat list of `key: "hex"`, so it is parsed by hand
  (no serde_yaml)
- The seed must reproduce the same image, so the RNG is an in-house
  SplitMix64: a few lines, and its output can never change under a dependency
  update
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

- Colors: base16 parsing, OKLab round trip, palette leveling
- Rasterizer: coverage, paint order, antialiased edges
- Scenes: same seed → same polygons, every polygon convex, screen fully
  covered
- CLI: invalid `--size` or a missing scheme exits non-zero
