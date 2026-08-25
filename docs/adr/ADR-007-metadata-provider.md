# ADR-007: Metadata provider abstraction with ScreenScraper first
- Status: Accepted

## Context
ScreenScraper is first, but provider-specific APIs should not define the whole app.

## Decision
Introduce a MetadataProvider boundary. ScreenScraper is first implementation. Normalize responses. Metadata is optional to basic local library use.

## Open Detail
Credential handling requires a dedicated spike.
