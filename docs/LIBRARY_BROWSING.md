# Library browsing and cover presentation (M8.6)

The Library has two browse presentations, not one. This document is the contract for which one is
shown when, what each one promises, and how a system's cover artwork is framed.

Scanning and identity are [`docs/LIBRARY_SCANNER.md`](LIBRARY_SCANNER.md). Metadata and covers are
[`docs/METADATA.md`](METADATA.md). Controller navigation across shelves is
[`docs/CONTROLLER_AND_FOCUS.md`](CONTROLLER_AND_FOCUS.md).

## The two presentations

| Library state | Presentation | Purpose |
| --- | --- | --- |
| No system selected (`systemId === null`) | **Bounded system shelves** | Discovery — see what the library holds |
| A system selected | **The paginated grid**, unchanged | Traversal — work through one system's complete collection |

Selecting a system is what asks for a complete collection. Everything that made the paginated grid
work — pages, the result range, ordering, filters — is untouched inside it.

All Systems is deliberately *not* a complete list. It shows a short preview of each system and hands
the user to the grid for the rest. That is why it has no pagination: a bounded preview has no pages,
and offering navigation for a view that cannot be navigated would be a lie.

## Shelves

Each shelf is one system: a heading, a bounded preview of that system's matching games, and a View
All control. A shelf never wraps onto a second line.

| Property | Contract |
| --- | --- |
| Order | The system catalog's own order — the same one the sidebar uses. There is no second ordering table in the Library feature |
| Unknown system | A system the catalog does not contain is **appended**, never dropped. Its games stay reachable |
| Empty system | A system with no match has no shelf at all. A list of empty headings is not a browse view |
| Preview size | 6 games (`DEFAULT_LIBRARY_SHELF_PREVIEW`), capped at 12 by the backend. Never derived from library size |
| Count | The heading states the system's **full** match count, not the preview length — "84 GAMES" above six cards |
| Overflow | A preview wider than the window scrolls horizontally, with a faded edge so a clipped row reads as continuing. It is never an endless scroll through the whole system |

The catalog list can be empty when the catalog query itself fails. Shelves are then shown in the
backend's order rather than hidden, because losing the whole browse view to a sidebar problem would
turn one failure into a "you have no games" lie.

### View All

View All is a real control, not decorative text. It is **not a route**: it sets exactly the system
filter the sidebar sets, which is why the sidebar's active row follows by itself and `back` out of
the main area keeps working unchanged. Its accessible name carries the system and the total, so it
is distinguishable among a dozen View All controls on one screen.

## Filters

Search, Favorites, `HIDE MISSING` and M8.5's `NEEDS REVIEW` all apply to shelves and all keep shelf
mode. Results are never flattened into one mixed-system grid, and matching is never reimplemented in
React.

| Filter state | Result |
| --- | --- |
| All Systems + Search | Shelf mode; each shelf shows its own matches; systems with none disappear |
| All Systems + Favorites | Shelf mode, favorites only |
| All Systems + Hide Missing | Shelf mode, available local content only |
| All Systems + any combination | Shelf mode; the filters compose exactly as they do in the grid |
| A system selected + any filter | The existing paginated grid, unchanged |
| Nothing matches | The existing `NO GAMES MATCH FILTERS` state with its reset action — never a list of empty headings |

### Hide Missing

Deleting a file does not delete its game. A scan reconciles the vanished path, marks the content
units and files missing and flips `games.availability` to `unavailable`, but the logical game, its
metadata, its cover and every user-owned decision about it are kept — `DOMAIN.md` invariant 2. Those
games stay in the Library carrying the card's `MISSING` flag, and launching them is refused.

`HIDE MISSING` binds the existing `availability` filter to `available` on both surfaces. It is a
query and never a deletion: nothing is removed, clearing the filter brings every hidden game back,
and a root that could not be enumerated marks nothing missing in the first place, so an unplugged
drive cannot make a library look empty. Removing a game's record for good is a separate, explicitly
confirmed operation that does not exist yet.

The Library's own query state owns every user filter choice and the search debounce. The shelf model
is handed those committed values, so both presentations commit at the same moment and there is no
second search box or duplicate filter state.

## The bounded shelf query

`query_library_shelves` is one set-oriented read, never one per system.

```text
one query
  → rank each system's matches in the grid's own title order   (ROW_NUMBER OVER PARTITION BY)
  → count each system's matches                                (COUNT(*) OVER PARTITION BY)
  → return only rows inside the preview rank
```

* The response is bounded by `system count × preview limit` and does not grow with the library.
  Tests prove this at 5,000 and 20,000 games.
* The shelf request derives a `LibraryQuery` and binds the **grid's own** filter predicate and list
  projection, so a game cannot match on one surface and not the other. The backend tests assert this
  by asking both surfaces the same question and comparing them.
* Shelves are grouped from whatever system identities the data holds, not from a known-system list.
* Both surfaces go through the same live provider-evidence validation.
* Metadata invalidation follows M6's bounded rule: an event costs a refresh only when it names a game
  a visible preview really contains, and events are debounced with a max wait. A whole-library scrape
  cannot become a request storm.

Ownership on the frontend is split to match: `useLibraryQuery` stays authoritative for the paginated
grid and for every filter choice, and `useLibraryShelves` owns only the shelf request and its own
loading, error and refresh state. While All Systems is shown the grid query does not fetch at all,
because nothing renders its result.

### Loading and failure

| State | Behaviour |
| --- | --- |
| First load | The shelf skeleton |
| Refresh | The committed shelves stay on screen |
| Refresh failed | The bounded shelves stay — they are last-known-good — with a refresh error and a retry |
| First load failed | The Library-query-unavailable error. The shell, sidebar and local scan controls stay available |
| Empty library | The existing empty-library onboarding state, unchanged |

## Cover presentation profiles

One 3:4 frame served DVD-shaped systems well and everything else badly. Artwork whose natural shape
did not match was cropped to fill the frame.

A system identity now maps to a presentation profile, and the profile decides the **media frame
only**. The card's title and system/year copy keep the ordinary layout, so title length can never
alter a cover's geometry.

| Profile | Ratio | Systems |
| --- | --- | --- |
| `landscapeBox` | 11 / 8 | SNES, Nintendo 64 |
| `portraitBox` | 3 / 4 | NES, Mega Drive |
| `squareBox` | 1 / 1 | Game Boy, Game Boy Color, Game Boy Advance, Saturn, Dreamcast |
| `jewelCaseBox` | 7 / 6 | PlayStation |
| `dvdBox` | 2 / 3 | GameCube |
| `standard` | 3 / 4 | Every unknown or future system |

These are **presentation profiles tuned for RetroFrontier's artwork, not historical packaging
specifications.** PlayStation and Saturn stay on the neutral frame deliberately: jewel-case artwork
differs between regions, and RetroFrontier will not assert a shape it cannot know.

Saturn and Dreamcast were expected to share PlayStation's frame, since all three ship in jewel
cases. Measurement said otherwise: three cached covers are 680x680 exactly, because the provider
crops PlayStation with its case spine and the other two without it. The crop is what gets framed,
so they sit with the handhelds on `squareBox` — a shared shape, not a shared platform. Dreamcast in
particular came off `dvdBox`, whose 2:3 frame spent a third of its height on nothing.

`jewelCaseBox` is the same correction on the other axis. ScreenScraper delivers PlayStation
artwork as the whole case wrap, spine included, so it is wider than tall — the cached cover measures
792 x 680, or 1.165, against the 0.750 the neutral frame assumed. Over a third of every PlayStation
frame's height was empty. Evidence here is a single cover, so this is the profile most worth
re-checking as the library grows; Saturn shares the physical packaging but no Saturn artwork was
available, and sharing a case is not evidence about the scan, so it stays neutral.

`squareBox` is what that principle looks like when the artwork disagrees with the frame. The three
Game Boy generations began on `portraitBox`; measured on the rendered shelves, the covers the
provider actually delivers sit at roughly 1.03–1.05, so over a quarter of each frame's height was
empty well above and below the art. The profile is 1:1 rather than a measured 1.04 — the frame is
an approximation of a shape, not a fit to one scan.

Profiles are measured against the artwork the provider actually delivers, not against physical
packaging. Cached covers in one real library read: handhelds 1.000-1.007 across 28 covers, Saturn and
Dreamcast 1.000 (3), SNES 1.360-1.368 (4), Nintendo 64 bimodal at 1.365 and ~1.45 (15), NES 0.731,
Mega Drive 0.712, GameCube 0.715, PlayStation 1.165. GameCube's 0.715 against a 2:3 frame is the
remaining known gap, on one cover.
Nintendo 64's scans are bimodal — seven at 1.365 and eight newer ones near 1.45 — so no single
frame fits it well. `landscapeBox` was retuned from 4:3 to 11:8 as the best compromise across both
cartridge-landscape systems, which roughly halves the average waste; the residual worst case on an
N64 cover is about 6%. Splitting SNES and Nintendo 64 into separate profiles would recover under a
percent and was not done.

Every declared profile must carry a CSS rule, and its `--cover-aspect-scale` must equal its
`--cover-aspect` as a number, since a shelf derives card width from the scale. A test asserts both:
a profile added to the mapping without a rule would otherwise render silently on the default 3:4.

An unknown or future system identity resolves to `standard`. It never throws, and it never requires a
backend change — the backend supplies `systemId` and nothing about artwork shape.

### Containment, not cropping

Library card artwork is **contained**, never cropped to fill. The frame keeps the system's shape and
an image of a different shape is letterboxed or pillarboxed inside it. Card geometry is decided by the
system, never by the image's own dimensions, so a shelf stays geometrically stable.

The rule is scoped to the Library card's own cover class. `GameCover` stays provider- and
system-agnostic — a test guards that — and Game Detail keeps its separate cover presentation.

### One profile, both presentations

A profile is a property of a system's card presentation, not of shelf mode. A GameCube card is
DVD-shaped on its shelf and DVD-shaped in the full GameCube grid.

Within a shelf, a card is never narrower than a readable title column; above that floor its
**width** is derived from one shared media height and the system's ratio. The media frame carries
the ratio and resolves its own height, so lifting a narrow profile off the floor makes that card
proportionally taller rather than squeezing its title — a `dvdBox` card stays DVD-shaped and simply
grows. Shelves of different systems therefore differ in height, as the mixed-system grid already
does; cards *within* one shelf always align, which is what scanning a row depends on.

## Deliberately not in M8.6

Infinite horizontal scrolling through a full system; user-ordered or drag-reordered systems; cover
ratio settings; region-specific box profiles; provider-specific artwork logic; ratio detection from
image dimensions; per-game ratio overrides; new artwork types; carousels, autoplay, or animation;
recently-played, favorites-only, genre, or custom-collection shelves.
