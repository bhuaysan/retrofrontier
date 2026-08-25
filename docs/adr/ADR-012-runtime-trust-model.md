# ADR-012: Managed runtime trust and anti-rollback model
- Status: Accepted

## Context
RetroFrontier downloads and executes RetroArch plus libretro cores, which are native code. HTTPS and an archive hash detect ordinary transfer corruption, but do not authenticate an artifact when a manifest host, download origin, CDN, DNS/TLS path, or signing key is compromised. A home-grown single-signature manifest also leaves key rotation, revocation, replay, freeze, and metadata mix-and-match behavior underspecified.

OS code signing is not a substitute for RetroFrontier release approval: it authenticates an OS-recognized publisher and platform policy, while RetroFrontier must authenticate the exact approved runtime/core set and compatibility policy.

## Decision
Use a TUF 1.0-compatible repository profile and a conforming client implementation rather than a bespoke signed-manifest protocol.

The signed RetroFrontier application ships the initial trusted `root` metadata. The repository uses:

- offline root keys with a 2-of-3 threshold;
- offline targets/release-approval keys with a 2-of-3 threshold;
- separately scoped snapshot and timestamp keys, which may be online because they cannot authorize arbitrary target content;
- consistent-snapshot, version, length, hash, and expiration checks;
- sequential root rotation signed by the thresholds of both the old and new roots.

Keys are not reused across roles, and threshold keys require independent custody. Private keys and signing operations stay outside the client, repository host, and source repository.

Ed25519 is the only V1 metadata signature scheme. SHA-256 is the V1 target and component digest. Supporting metadata is bounded before parsing, duplicate/ambiguous JSON keys are rejected, and the selected TUF implementation's documented canonical metadata encoding is authoritative.

Each per-platform/architecture Runtime Release manifest, runtime archive, core archive, and external file inventory is an immutable TUF target. The manifest itself uses strict I-JSON serialized with RFC 8785 JCS so its SHA-256 is reproducible. It has no embedded self-referential signature object. TUF targets metadata authenticates the exact length and digest of every downloadable target.

The manifest references component target paths and repeats their exact lengths and SHA-256 digests; the client requires those values to equal trusted TUF targets metadata before downloading. This deliberate check keeps a self-contained release record without creating a second trust decision. V1 resolves target paths only through configured RetroFrontier HTTPS repositories/mirrors and a bounded redirect allowlist; it never treats a manifest value as an arbitrary URL fetch.

The authenticated release manifest contains every value that can affect download, extraction, validation, or launch, including:

- schema and immutable release ID;
- monotonically increasing release sequence scoped to channel, platform, and architecture;
- compatible RetroFrontier versions;
- exact RetroArch and core identities, upstream source/build revisions, licenses, and approved core/system mappings;
- TUF component target paths and approved mirror/redirect policy;
- archive formats, exact byte lengths, and SHA-256 digests matching TUF targets metadata;
- normalized expected roots, extraction limits, executable-bit policy, allowed launch paths, and format-specific link policy;
- an exact extracted-file inventory of relative path, type, size, digest, and executable status as applicable, plus the normalized relative target of every permitted symbolic link, or the digest of a separate immutable inventory target;
- OS code-signing identity and notarization requirements where applicable;
- compatibility facts needed for validation, without mutable local health or activation state.

An authenticated, versioned `runtime-policy` TUF target carries revoked release IDs and a minimum-safe release sequence per channel/platform/architecture. The client persists the highest trusted metadata versions and highest security floor it has observed. Automatic and manual rollback must satisfy that floor. Semantic versions and timestamps are not used as monotonic security counters. Once received, a revocation may block a vulnerable active runtime even when no replacement is currently downloadable; this is an intentional security-over-availability decision and must produce a clear repair-required state.

Expiration applies to fetching new metadata and artifacts. Initial maximum lifetimes are seven days for timestamp, 31 days for snapshot, 90 days for targets, and 366 days for root metadata; publication automation refreshes online metadata well before expiry, and application releases provide a recovery path before root expiry. Changing these bounds is a security/availability policy change, not a server-side convenience.

Cryptographic verification requires no network once trusted metadata and target bytes are present. A previously authenticated installed runtime does not stop launching merely because the device is offline or metadata has expired. Full reconstruction may reuse a previously verified local target cache at or above the locally known security floor, but discovering or approving a new release requires a successful current-metadata refresh. Offline clients cannot learn new revocations; this limitation is shown when connectivity returns and is inherent rather than hidden.

Trusted root/metadata versions, the highest accepted security floors, and release revocations are security state, not disposable runtime cache. Runtime uninstall, repair, rollback, and cache cleanup preserve them. Only an explicit whole-application-data reset may remove them; same-user or administrator deletion remains outside the updater's enforceable boundary.

## Key lifecycle and emergency response
- Multiple root and targets keys are active according to their threshold, enabling overlap during routine rotation without accepting any one signature.
- A compromised key below threshold is removed through a sequential root rotation and replaced before further releases.
- A compromised targets threshold can authorize malicious native code and malicious release-policy counters. Rotate the targets keys through a new root, publish new metadata above every accepted version, revoke affected releases, and raise the legitimate minimum-safe sequence. If an attacker poisoned a client's accepted version/floor beyond repository recovery or controls availability, recovery requires an independently authenticated application update; never silently lower persisted floors.
- Compromise of the root threshold cannot be repaired safely through that runtime repository. Recovery requires an independently authenticated RetroFrontier application update or other out-of-band bootstrap.
- Runtime metadata and revocation state should also ship with application updates so a compromised or unavailable runtime host is not the only recovery channel.

## What this model does and does not provide
Together, HTTPS, TUF-authenticated metadata and component descriptions, SHA-256 verification, safe extraction, and platform code-signing checks defend against remote substitution, compromised mirrors/CDNs/manifest hosts below key threshold, corruption, replay/freeze detection, and many interrupted-update states.

They do not prove that approved upstream code is benign, contain a malicious core, protect a client after enough signing keys or the application update trust root are compromised, guarantee update availability, or defeat an administrator/root account or malware already running as the same OS user. Static file verification can detect many local modifications while the application and trust root remain intact, but it cannot eliminate same-user process injection or every verify-to-execute race.

## Platform code signing
- Windows Authenticode/Smart App Control and macOS Developer ID/Gatekeeper/notarization are mandatory platform-policy inputs where required, but are independently verified in addition to this manifest trust model.
- Signing RetroFrontier itself does not sign or approve a later-downloaded RetroArch executable or core.
- Linux normally has no equivalent mandatory publisher check for this path, so RetroFrontier metadata remains the principal artifact-authenticity mechanism.

## Consequences
The trust system has more metadata roles than a single detached signature, but each role directly addresses an explicit native-code updater threat: root/key compromise, target authorization, mix-and-match, replay, or freeze. There is no certificate authority, OCSP dependency, or per-user PKI. Production key custody, recovery drills, client-library selection, and repository publication remain implementation/release-readiness tasks.
