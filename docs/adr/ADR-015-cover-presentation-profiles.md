# ADR-015: Cover presentation profiles measured from delivered artwork

- Status: Accepted

## Context

A Library card frames its cover art. One shared frame forced every system through the same shape,
which either cropped the wide artwork or stranded the tall artwork in empty space. M8.6 replaced it
with per-system presentation profiles, and derived each profile's ratio from the system's **physical
packaging**: a SNES box is wider than tall, a GameCube keepcase is tall and narrow, a jewel case
could not be claimed at all.

Packaging turned out to be the wrong source. RetroFrontier does not frame a box; it frames a scan
that a metadata provider chose how to crop and normalize. The two disagree, sometimes severely:

- Game Boy, Color and Advance artwork is square. On a 3:4 portrait frame, a quarter of every
  frame's height was empty.
- PlayStation artwork is *wider* than tall, because the provider includes the case spine beside the
  front. On the neutral 3:4 frame, over a third of the height was empty.
- Dreamcast artwork is square, not keepcase-shaped. On a 2:3 frame, a third of the height was empty.
- Saturn and Dreamcast share PlayStation's physical packaging and do **not** share its ratio,
  because the provider crops them without the spine.

The last point is the decisive one: two systems can ship in the same case and still need different
frames. Packaging cannot predict the crop, and the crop is what gets framed.

## Decision

A cover presentation profile's ratio is derived from **measuring the artwork the provider actually
delivers**, never from the physical media or packaging.

Rules that follow from it:

- A system moves onto a measured profile only when there is artwork of its own to measure. Sharing a
  case, a generation, or a manufacturer with a measured system is not evidence.
- `standard` (3:4) is the frame a system holds while nothing has been measured for it, and the frame
  an unknown or future system identity resolves to. It is a held position, not a claim. No V1 system
  sits on it today.
- Ratios stay small whole-number fractions close to the measurement, not exact fits to it. A profile
  approximates a shape; fitting it to one provider's normalization would claim a precision the
  artwork does not have and would break the moment that provider re-scans.
- Profiles are shared where systems share a shape. `squareBox` frames three handheld generations
  plus Saturn and Dreamcast, because their artwork is the same shape — not because the platforms are
  alike.
- The mapping is frontend policy. No Rust DTO carries a ratio, a shape, or a packaging format, and
  the backend supplies only an authoritative `systemId`.

## Implementation status

`src/features/library/systemCoverPresentation.ts` owns the mapping;
`src/styles/index.css` owns each profile's `--cover-aspect` and its `--cover-aspect-scale`
counterpart, from which a shelf derives card width. A test walks `COVER_PRESENTATIONS` and requires
every declared profile to carry both custom properties and to keep them numerically in step, so a
profile added without a CSS rule cannot render silently on the default frame.

Measured against one real library's cached covers:

| Profile | Ratio | Systems | Measured | n |
| --- | --- | --- | --- | --- |
| `landscapeBox` | 11 / 8 | SNES, Nintendo 64 | 1.360–1.46 | 19 |
| `portraitBox` | 3 / 4 | NES, Mega Drive | 0.712–0.731 | 2 |
| `squareBox` | 1 / 1 | Game Boy, Color, Advance, Saturn, Dreamcast | 1.000–1.007 | 31 |
| `jewelCaseBox` | 7 / 6 | PlayStation | 1.1647 | 2 |
| `dvdBox` | 5 / 7 | GameCube | 0.7147 | 2 |
| `standard` | 3 / 4 | fallback only | — | — |

## Consequences

Frames now sit within a fraction of a percent of the artwork they hold, with one exception that no
decision can fix: Nintendo 64's own provider scans disagree by 7%, seven at 1.365 and eight near
1.45, so `landscapeBox` is a compromise rather than a fit.

Shelves of different systems differ in height, because a card's width comes from a shared media
height and its own ratio. Cards *within* one shelf always align, which is what scanning a row
depends on, and the mixed-system grid already varied this way before the profiles existed.

The evidence base is one contributor's library, which is thin for `portraitBox` (two covers) and
`jewelCaseBox` (two). Profiles are cheap to re-measure and are expected to be revisited as libraries
grow; `jewelCaseBox` was set from a single cover and later confirmed by a second that matched it to
the pixel. A provider changing how it crops a system would invalidate that system's profile, which
is an accepted cost of framing the scan rather than the box.
