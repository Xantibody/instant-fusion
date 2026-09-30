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
  polygon filler. Curves are not a rasterizer concern: a scene samples a
  curve and hands over quads, which the antialiasing joins seamlessly

An earlier draft had generators return a scalar field `(x, y) → [0,1]` that a
colorizer mapped to color. That fits noise-like patterns, but the wallpapers
wanted here are crisp shapes: their edges are the picture, and a field
sampled per pixel can only approximate an edge.

## CLI

```
instant-fusion --scheme <base16.yaml> --size 1920x1200 [--seed N] [--kind flow|orbit|facet] -o <out.png|->
```

- Without `--seed`, a random seed is used. The seed and kind are printed to
  stderr so a wallpaper worth keeping can be reproduced
- Without `--kind`, the kind is picked from the seed
- `-o -` writes the PNG to stdout
- On failure, exit 1 with a message on stderr (this stops hyprpaper's
  ExecStartPre)

## Colors

- The scheme is an anchor, not a palette. One of its accents (base08–0F)
  gives the hue and base00 the tone; the four semantic colors (background,
  primary, secondary, accent) are derived from that in OKLab. A scheme
  need not contain the colors that appear: a navy scheme may yield a muted
  beige
- A harmony, chosen per kind from the seed, says how the hues relate:
  monochromatic (one hue, lightness steps only), analogous (secondary and
  accent within 45° on either side) or muted complementary (accent across
  the wheel, chroma capped lower still)
- Chroma is capped at 0.07 everywhere, so a loud scheme still gives a
  quiet wallpaper. Lightness steps outward from the background: darker
  than a pale ground, lighter than a dim one
- The accent is for small areas only; a scene keeps it under about 15% of
  the image
- Mix in OKLab (sRGB interpolation muddies the midpoints)

## Scenes

Three looks that must read as different pictures: a curve, an arc, a plane.
All are minimal and large-scale; nothing smaller than a band or a face is
drawn, and every scene starts with one background polygon so no pixel is
left uncovered.

| kind    | harmony                  | construction                                                 | look                                    |
| ------- | ------------------------ | ------------------------------------------------------------ | --------------------------------------- |
| `flow`  | mono / analogous         | 2–5 wide bands on one shared wave, stacked with clear gaps   | a slow current across a quiet ground    |
| `orbit` | analogous / muted compl. | 2–4 rings far larger than the screen, centered off it        | sweeps of huge rims; an accent on one   |
| `facet` | mostly mono              | the screen cut into 6–10 convex faces shaded by pseudo normal | a pyramid, a box corner, a folded sheet |

- `flow` bands never cross. A draft with independent amplitudes pinched the
  ground between two bands into crescent slivers. Bands take at most 60%
  of the height together
- `orbit` draws its radius between the nearest and farthest distance from
  the center to the screen, so the rim is guaranteed to cross it. Only
  the angles that reach the screen become quads
- `facet` keeps the old facade's T-junctions: a cut stops at the face it
  splits, which reads as planes rather than a mesh. Cuts that would leave a
  half under 30% of its parent or too thin are rejected, so slivers cannot
  accumulate
- Curves are sampled so that no joint bends more than about 2.3°. That puts
  a band or arc at up to 160 quads, more than the few dozen first
  proposed, but the quads are one color and cost nothing visible; fewer
  would show the straight pieces on a wide curve

Removed: the `terrain` and `lowpoly` mesh kinds (fields of small
triangles, the crowded look the set above avoids) and the facade's seams
and sunk panels. Both are in the history before `feat!: drop the terrain
and lowpoly kinds` if wanted again.

Deferred: isometric solids with cast shadows and constructivist
compositions (both worked in the proof of concept but are not the look
wanted first).

## Randomness

- Every consumer of the seed draws from its own SplitMix64 stream: the
  kind, the palette (harmony first, then colors) and the scene. A change in
  how many numbers a scene takes never shifts the palette
- The same scheme, kind, seed and size give the same polygons and the same
  PNG

## Performance

- Target: under 1 second at 1920×1200 (hyprpaper waits during ExecStartPre);
  measured around 0.08 s for every kind
- A scene is a few hundred polygons at most, none of them small
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

- Colors: base16 parsing, OKLab round trip and polar helpers; each harmony's
  hue relations, the chroma cap, the primary's hue being a scheme accent
- Rasterizer: coverage, paint order, antialiased edges
- Scenes, for every kind over many seeds: same seed → same polygons, every
  polygon convex with an area, screen fully covered
- `flow`: 2–5 bands within the width bounds, never crossing, never thinner
  than the minimum; quad count bounded
- `orbit`: 2–4 arcs centered off screen within the radius and width
  bounds, each reaching the screen; quad count bounded; the accent on at
  most one arc and at most 35% of it
- `facet`: 6–10 faces, none thinner than the roundness floor; one hue under
  a monochromatic palette
- CLI: every kind can be asked for, a seed reproduces its PNG, invalid
  `--size` or a missing scheme exits non-zero
