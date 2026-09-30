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
- Mix in OKLab (sRGB interpolation muddies the midpoints). Facet is
  monochromatic always: its faces differ in lightness only, since that
  lightness is the light on their planes

## Scenes

Three looks that must read as different pictures: layered movement,
fragments of huge off-screen geometry, a fragment of one folded solid.
All share two principles. A hierarchy of one dominant element, quieter
secondaries and at most a small accent, with uneven spacing and a wide
quiet area, so nothing reads as "N equal shapes placed at random". And
the screen is a crop of something larger: focal points, centers and
apexes lie outside it and every element enters and leaves through its
edges, so nothing reads as a motif, logo or badge placed on the canvas.
Every scene starts with one background polygon so no pixel is left
uncovered.

| kind    | harmony                    | construction                                                                   |
| ------- | -------------------------- | ------------------------------------------------------------------------------ |
| `flow`  | mono / analogous           | one master curve at a slant; 2–4 bands ride it at their own width and wave     |
| `orbit` | analogous, rarely compl.   | one ring 1.5–4 screens wide grazing the screen, centered off it; 1–2 derived   |
| `facet` | mono                       | one ridge across the screen, 0–2 creases across it; planes lit by their normals |

- `flow`: the master curve is two low-frequency waves and a tilt, so it
  bends once or twice and may leave the screen. A band scales it by
  0.8–1.2 and adds a wave of up to 5% of the height, so no two are
  parallel copies. Widths are one dominant, secondaries at 55–80% of it
  and an optional narrow accent; gaps are uneven and a band may lie over
  its neighbor when the two are shaped alike (bands that differ more would
  swap sides and cross). Separation is measured along the screen, so bands
  either keep a clear gap or clearly overlap; a draft with free amplitudes
  pinched the ground between two bands into crescent slivers. The whole
  current runs at a slant of up to 25° either way, and the stack may
  overrun the edges by 10–45% of the height, so nearly every scene has a
  band cut by an edge; bands all shown end to end read as stripes
- `orbit`: the dominant ring passes through a point on the edge of the
  screen or a little beyond it, and a ring is kept only when no more than
  a fifth of it reaches the screen, so what shows is the part of a huge
  ellipse that happens to pass; a ring built through a point inside the
  screen was an arc on display. A secondary shifts its center by 10–35%
  of the width at 0.6–1.4 times the radius; a rare accent shifts well
  away at a larger one. Each ring is one color; an accent sweep painted
  along part of a ring read as tape. Only the sweeps that reach the screen
  become quads, cut by lyon_geom's flattening to a tolerance of 0.3 px
- `facet`: the structure comes first and the faces follow. One ridge
  crosses the middle third of the screen at any angle with a plane on
  either side; one or two creases cross it through a point off the
  middle, and the largest open face takes a crease of its own only while
  there are under five faces or it holds over 45% of the image; no three
  folds share a point. The result is five to eight convex faces. Each
  face carries a 3D normal: the first plane leans a little, and a cut
  turns the two halves 20–45° apart about the fold line, away from each
  other across a ridge and toward each other across a valley. One light
  20–40° over the screen gives each face its lightness from that normal
  and nothing else, so neighbors differ because they face the light
  differently. Two earlier facets are gone: unrelated long cuts shaded on
  their own read as a mosaic, and ridges fanning from one off-screen apex
  read as a shard of that fan; both also pushed alike neighbors apart in
  lightness after the fact, which made polygons of the planes. Cuts are
  refused when a half is under a quarter of its parent, under 2.5% of the
  image or thinner than a 1:5 rectangle
- Curves are sampled so that no joint bends more than about 2.3°. That
  puts a band or ring at up to 200 quads, more than the few dozen first
  proposed, but the quads are one color and cost nothing visible; fewer
  would show the straight pieces on a wide curve

### Composition scoring

Each kind draws a few candidates per seed (four or five) and keeps the best
by a score computed from the polygons alone: which polygon is on top at
each point of a 48×30 grid gives the ground's share and each element's.
Flow steers the ground toward half the image with the dominant band on
screen, some band leaving through an edge and no two bands parallel;
orbit toward about 80%, refuses rings crossing more than once, a ring
through the middle, alike visible lengths or an oversized accent, and
prefers no crossing to one; facet wants its largest face at 25–45%, no
face under 2%, no point in the middle where four or more faces meet, and
as few neighbors as possible that the light leaves alike. This spares
the seeds that would have drawn a poor composition without touching
determinism: every candidate draws from the scene stream.

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

- Rust. Dependencies: `clap` (derive), `png`, `anyhow`, `palette` for the
  OKLab conversions and `lyon_geom` for curve flattening and transforms;
  the composition, the RNG and the convex rasterizer stay in the crate.
  `kurbo` would have served in place of `lyon_geom`; `geo` was left out
  because it pulls in far more than the four polygon helpers it would
  replace, `tiny-skia` because it would replace the rasterizer itself
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
- `flow`: 2–4 bands with one dominant and clearly different widths, every
  pair either clearly apart or clearly overlapping, quads bounded and never
  thinner than the minimum; at least half the scenes let a band leave
  through an edge
- `orbit`: 2–3 rings with one dominant, every center clearly off screen,
  every ring reaching the screen with at most a fifth of itself, in one
  color; the chosen scene has at
  most three colors and a bounded quad count; the crossing counter on
  known circles
- `facet`: 5–8 faces covering the frame exactly, one at 18% or more, none
  under 2.5% or thinner than the roundness floor; the ridge crossing the
  screen with faces on both sides and no three folds through one point;
  the halves of a cut turned the fold's angle apart, away across a ridge
  and toward across a valley; the shade being the light on the normal;
  one hue and one chroma across the faces
- CLI: every kind can be asked for, a seed reproduces its PNG, invalid
  `--size` or a missing scheme exits non-zero
