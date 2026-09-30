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
- Lightness is a hierarchy, ordered by the weight a color carries:
  ground → secondary (the quiet figure) → primary (the main one) → accent
  (the strongest, for the smallest area). Darker than a pale ground,
  lighter than a dim one
- Chroma is capped at 0.07 everywhere, and the accent loses a quarter of
  it on a dark scheme, so a loud scheme still gives a quiet wallpaper
- Mix in OKLab (sRGB interpolation muddies the midpoints)

## Scenes

Three looks that must read as different pictures: layered movement,
fragments of huge off-screen geometry, large architectural planes. All
share one principle: a hierarchy of one dominant element, quieter
secondaries and at most a small accent, with uneven spacing and a wide
quiet area, so nothing reads as "N equal shapes placed at random". Every
scene starts with one background polygon so no pixel is left uncovered.

| kind    | harmony                    | construction                                                                   |
| ------- | -------------------------- | ------------------------------------------------------------------------------ |
| `flow`  | mono / analogous           | one master curve; 2–4 bands ride it at their own width, scale and slight wave  |
| `orbit` | analogous, rarely compl.   | one dominant ring a screen wide or more, centered off screen; 1–2 derived      |
| `facet` | mostly mono                | 1–2 cuts edge to edge, then a few faces cut once more; shaded as one fold      |

- `flow`: the master curve is two low-frequency waves and a tilt, so it
  bends once or twice and may leave the screen. Widths are one dominant,
  secondaries at 55–80% of it and an optional narrow accent; gaps are
  uneven and a band may lie over its neighbor. Separation is measured
  along the screen, so bands either keep a clear gap or clearly overlap;
  a draft with free amplitudes pinched the ground between two bands into
  crescent slivers
- `orbit`: a secondary shares the dominant's center almost exactly at a
  nearby radius; a rare accent shifts well away at a larger one. Each ring
  is one color; an accent sweep painted along part of a ring read as tape.
  Only the angles that reach the screen become quads
- `facet`: the second long cut is either nearly parallel and well apart or
  across the first but off center, so the two never meet in the middle;
  a hub is what made the old fan read as the generator. Cuts stop at the
  face they split (T-junctions read as planes, not a mesh) and are refused
  when a half is under a quarter of its parent, under 2.5% of the image
  or thinner than a 1:5 rectangle. Faces are convex polygons, not
  triangles
- Curves are sampled so that no joint bends more than about 2.3°. That
  puts a band or ring at up to 200 quads, more than the few dozen first
  proposed, but the quads are one color and cost nothing visible; fewer
  would show the straight pieces on a wide curve

### Composition scoring

Each kind draws a few candidates per seed (four or five) and keeps the best
by a score computed from the polygons alone: which polygon is on top at
each point of a 48×30 grid gives the ground's share and each element's.
Flow steers the ground toward half the image with the dominant band on
screen; orbit toward about 72% and refuses rings crossing more than once,
three rings in the middle, alike visible lengths or an oversized accent;
facet wants its largest face at 25–45%, no face under 2% and no point in
the middle where four or more faces meet. This spares the seeds that would
have drawn a poor composition without touching determinism: every
candidate draws from the scene stream.

Removed: the `terrain` and `lowpoly` mesh kinds (fields of small
triangles, the crowded look the set above avoids) and the facade's seams
and sunk panels. Both are in the history before `feat!: drop the terrain
and lowpoly kinds` if wanted again.

Deferred: isometric solids with cast shadows and constructivist
compositions (both worked in the proof of concept but are not the look
wanted first).

## Randomness

- Every consumer of the seed draws from its own SplitMix64 stream: the
  kind, the palette (harmony first, then colors) and the scene, candidates
  included. A change in how many numbers a scene takes never shifts the
  palette
- The same scheme, kind, seed and size give the same polygons and the same
  PNG

## Performance

- Target: under 1 second at 1920×1200 (hyprpaper waits during ExecStartPre);
  measured around 0.07 s for every kind, candidates included
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
  hue relations, the chroma cap, the lightness order, the calmer dark
  accent, the primary's hue being a scheme accent
- Rasterizer: coverage, paint order, antialiased edges
- Scenes, for every kind over many seeds: same seed → same polygons, every
  polygon convex with an area, screen fully covered; the candidate picker
  and the grid of owners
- `flow`: 2–5 bands with one dominant and clearly different widths, every
  pair either clearly apart or clearly overlapping, quads bounded and never
  thinner than the minimum
- `orbit`: 2–3 rings with one dominant, every center clearly off screen,
  every ring reaching the screen in one color; the chosen scene has at
  most three colors and a bounded quad count; the crossing counter on
  known circles
- `facet`: 5–10 faces covering the frame exactly, one at 18% or more, none
  under 2.5% or thinner than the roundness floor; neighbors at least 0.05
  apart in lightness; one hue under a monochromatic palette
- CLI: every kind can be asked for, a seed reproduces its PNG, invalid
  `--size` or a missing scheme exits non-zero
