# ADR-016: Disc content is submitted and judged, not refused

- Status: Accepted
- Amends: [ADR-007](ADR-007-metadata-provider.md)

## Context

ADR-007 decided that "unsupported container representations are deferred rather than guessed", and
M5 implemented it literally: a CHD, a CUE/BIN set, a GDI, or a disc-system single-file image was
refused inside `evidence_for_unit` before any request was built. The reasoning was that ScreenScraper
does not document which bytes are canonical for a disc container — a real gap, recorded as open
questions 8 and 9 of the ScreenScraper spike.

The refusal was more expensive than it looked. In a real library every cartridge system matched and
PlayStation had no match of any kind, in any format, ever — and the heuristic fallback that was
supposed to cover it had never produced a single candidate row.

Two working scrapers were examined:

- **ES-DE** appends `romnom` unconditionally and adds `md5` and `romtaille` only when a hash was
  computed. It has no format-specific handling for CHD, CUE, BIN or ISO.
- **Skyscraper** sends `crc`, `md5`, `sha1`, `romnom` and `romtaille` with no format-specific logic,
  and documents that ScreenScraper matches on **a checksum or an exact filename**.

Neither answers the canonical-representation question, because neither has to. Refusing to produce
evidence did not make matching safer; it removed the whole request, filename included, which was the
only route disc content had left.

## Decision

Producing local evidence is a claim that these are **the bytes we have**, not a claim that they are
the provider's canonical identity. Disc containers therefore submit evidence and reach the provider
like any other content.

Safety stays exactly where it was, one layer down: `classify_deterministic_match` is unchanged and
still requires an agreeing size and agreeing hashes before anything attaches. A hash the provider
has never seen costs nothing, because the same request carries the filename and the classifier
refuses the mismatch.

Consequently:

- A CUE descriptor matches deterministically when its hashes agree. The provider's CD records come
  from Redump, whose `.cue` files are standard text, so these bytes really are in its database.
- A CHD the provider has never hashed is *named* by the lookup and offered as a candidate the user
  confirms. It never attaches on its own. `MatchType` gains no new variant: a match is still either
  hash-verified or user-confirmed.
- When a lookup answers with a game whose content record cannot be compared, that answer becomes the
  candidate. It is a name match the provider made against the file's real basename, which beats a
  fuzzy search over a local title still carrying its filename decorations — so the title search is
  issued only when the answer names no game at all.
- Two refusals survive on their own merits. A playlist names other content rather than being
  content, so no file in it is the game. A GDI set is unverified, because no Dreamcast GDI content
  was available to establish what it hashes to; it is refused rather than guessed at.

## Consequences

Disc systems gain the matching they never had, and the honesty rule that made ADR-007 worth having —
nothing attaches without agreeing evidence — is untouched.

A CUE descriptor's hash only matches while the file is byte-identical to the provider's copy.
Renaming tracks rewrites the `FILE` lines inside the `.cue` and changes its hash, so such a set falls
back to the name-based suggestion like a CHD. This is a real and common case, not an edge case.

More content now reaches the provider, so more requests are issued for content that will not match
deterministically. A named answer suppresses the fallback title search, which returns one request on
the common path.

ADR-007's list of deferred capabilities is superseded for CHD, CUE/BIN, and disc-system single-file
images. GDI, M3U/multi-disc, RVZ and GCM remain deferred there.
