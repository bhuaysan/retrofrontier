# ADR-007: Metadata provider abstraction with ScreenScraper first

- Status: Accepted
- Amended by: [ADR-016](ADR-016-disc-content-lookup.md), which reverses this record's treatment of
  disc containers. The container-format clauses below are kept as written for the history; read
  ADR-016 for what the code does now.

## Context

ScreenScraper is first, but provider-specific APIs should not define the whole app.

## Decision

Introduce a MetadataProvider boundary. ScreenScraper is first implementation. Normalize responses. Metadata is optional to basic local library use.

For V1, the ScreenScraper adapter runs in Rust and communicates directly with ScreenScraper; no
RetroFrontier cloud/proxy service is introduced. RetroFrontier uses only its own application
developer credential. Development values remain in ignored local environment configuration.
Release values are injected from protected CI/release secrets at build time and are never committed
as source or generated source. Because a distributed client necessarily makes the application
credential recoverable, it is not a cryptographic secret boundary; optional obfuscation is only an
accidental-disclosure measure.

A personal ScreenScraper account is optional. Rust owns personal credentials and persists them only
through the OS credential vault/keychain. SQLite may store non-secret configuration or an opaque
reference, and normal read IPC never returns secrets.

M5 stores replaceable normalized metadata, provider identity/provenance, evidence-bound match state,
and one selected primary cover for local/offline use. It does not persist raw authenticated
responses or credential-bearing media URLs by default, bundle or redistribute provider data, or
scrape broad media. Exact visible attribution is implemented in M6; M5 preserves enough provider
and source information to support it.

Automatic matching requires an unambiguous system mapping, a concrete returned provider ROM record,
agreement with current M4 hashes/size, and no conflict. Name search remains candidate-only.
Unsupported container representations are deferred rather than guessed. Provider failure and stale
evidence can change only provider-specific state and never local `Game`, `ContentUnit`, or
`ContentFile` identity/availability.

## Implementation status

M5 implements this decision. `MetadataProvider` is the provider-neutral boundary,
`ScreenScraperProvider` is the only implementation, and every provider-specific detail — endpoint
construction, provider system identifiers, response parsing, media selection, quota extraction, HTTP
status interpretation, credential injection, and URL redaction — stays inside that adapter.
Application credentials come from an ignored local environment during development and from
build-time injection for releases; optional personal credentials go through a `CredentialVault`
abstraction backed by the OS keychain, with a session-only fallback and an injectable
implementation for tests. Provider state, evidence, normalized metadata, media, jobs, quota, and
user-owned decisions are persisted in separate tables that all reference `games (id)` restrictively.
Automatic attachment requires agreeing returned content evidence; heuristic results remain
candidates. Details are recorded in [`docs/METADATA.md`](../METADATA.md).

## Consequences

M5 can proceed without waiting indefinitely for every provider-policy or container-format question.
The direct-client credential distribution and narrow local cache remain consciously accepted
provider-policy/legal risks, not verified ScreenScraper permission. CHD, CUE/BIN, GDI,
M3U/multi-disc, and RVZ automatic matching, broad media, and portable provider-cache export are
non-blocking deferred capabilities. The ScreenScraper spike records the evidence, ES-DE precedent,
project decisions, and residual risks.
