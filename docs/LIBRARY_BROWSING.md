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

Search, Favorites and M8.5's `NEEDS REVIEW` all apply to shelves and all keep shelf mode. Results
are never flattened into one mixed-system grid, and matching is never reimplemented in React.

| Filter state | Result |
| --- | --- |
| All Systems + Search | Shelf mode; each shelf shows its own matches; systems with none disappear |
| All Systems + Favorites | Shelf mode, favorites only |
| All Systems + any combination | Shelf mode; the filters compose exactly as they do in the grid |
| A system selected + any filter | The existing paginated grid, unchanged |
| Nothing matches | The existing `NO GAMES MATCH FILTERS` state with its reset action — never a list of empty headings |

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
| `landscapeBox` | 4 / 3 | SNES, Nintendo 64 |
| `portraitBox` | 3 / 4 | NES, Game Boy, Game Boy Color, Game Boy Advance, Mega Drive |
| `dvdBox` | 2 / 3 | GameCube, Dreamcast |
| `standard` | 3 / 4 | PlayStation, Saturn, and every unknown or future system |

These are **presentation profiles tuned for RetroFrontier's artwork, not historical packaging
specifications.** PlayStation and Saturn stay on the neutral frame deliberately: jewel-case artwork
differs between regions, and RetroFrontier will not assert a shape it cannot know.

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

Within a shelf, card **width** is derived from one shared media height and the system's ratio, so a
wide SNES card and a narrow GameCube card still line up at the same height and the vertical rhythm
down the page stays coherent. A per-card width floor keeps the narrowest profile legible; it is set
close to that profile's own derived width so it barely perturbs the shared height.

## Deliberately not in M8.6

Infinite horizontal scrolling through a full system; user-ordered or drag-reordered systems; cover
ratio settings; region-specific box profiles; provider-specific artwork logic; ratio detection from
image dimensions; per-game ratio overrides; new artwork types; carousels, autoplay, or animation;
recently-played, favorites-only, genre, or custom-collection shelves.
