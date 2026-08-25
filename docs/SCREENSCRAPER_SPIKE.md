# ScreenScraper Authentication Spike

## Goal
Determine how RetroFrontier can legally and technically use ScreenScraper as the first metadata provider in an open-source desktop application.

## Model
Use **GPT Luna Max**. Escalate only for significant security/architecture issues.

## Questions
Determine:
- developer authentication fields
- expected client identification
- whether developer credentials may be distributed in an open-source desktop client
- whether user credentials are optional/required
- request limits
- concurrency/thread limits
- retry/rate-limit behavior
- caching expectations
- offline/failure behavior
- whether a backend/proxy is required or discouraged

## Security Principle
Do not assume a credential compiled into an open-source desktop application can remain secret.

## Product Principle
Metadata failure must not make the local game library unusable.

## Deliverable
Document:
1. supported auth flow
2. credential ownership
3. storage requirements
4. rate-limit model
5. cache/retry model
6. relevant usage constraints
7. recommended provider architecture
