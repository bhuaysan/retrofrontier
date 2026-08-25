# RetroFrontier Security Policy

## Project Stage
RetroFrontier is in early development. Security-sensitive functionality, especially runtime downloads/updates, is not production-ready until explicitly reviewed.

## Reporting
When private vulnerability reporting is enabled on GitHub, use it for security issues.

Do not publish sensitive exploit details, credentials, signing material, or private user data in a public issue.

Non-sensitive bugs can use normal issues.

## Security-Sensitive Areas
- downloading executable runtime components
- manifest authenticity
- integrity verification
- archive extraction/path traversal
- executable activation
- rollback/recovery
- RetroArch/core launch
- ScreenScraper credentials
- app updater/signing
- DB migrations affecting user data

## Runtime Downloads
Production runtime downloads must define:
- approved sources
- version pinning
- integrity verification
- authenticity/signing where required
- safe extraction
- staged install
- activation only after validation
- rollback

Never activate a partial/unverified runtime.

## Secrets
Never commit or log:
- API secrets
- passwords
- tokens
- signing keys
- certificate secrets
- ScreenScraper developer passwords
- other credentials

## User Content
V1 must not automatically rename, move, convert, or delete ROM content.

Runtime update/repair must not delete ROMs, BIOS, saves, states, metadata, or DB.

## Third-Party Components
Track versions and licenses for RetroArch, cores, and other dependencies. Consider both security fixes and compatibility risk when updating.
