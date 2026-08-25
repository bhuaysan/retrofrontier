# ADR-005: Game and content are separate domain concepts
- Status: Accepted

## Context
A flat `games(file_path)` model fails for CUE/BIN, multi-disc, M3U, CHD, regions, revisions, and multiple copies.

## Decision
Model logical Games separately from playable Content Units and physical Content Files.

## Consequences
Correct disc/content modeling and more resilient metadata, at the cost of scanner/persistence complexity.
