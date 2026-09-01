# Superseded Linux x86_64 Runtime Release generations

ADR-012 gives a Runtime Release an immutable id, a monotonically increasing sequence, and immutable
authenticated targets. A change to what a release ships is therefore a **new generation**, never an
edit to a published one.

The definitions in this directory are the historical record of superseded generations. They are kept
verbatim so an already-published release stays reconstructable and reviewable, and so the
supersession rule (`ReleaseDefinition::supersedes`) has a real previous generation to check the
active definition against.

They are **not** build inputs. `rf-runtime-release` is only ever run against the active definition
one directory up. In particular, `runtime-release-001.json` still pins four rolling
`buildbot.libretro.com/nightly/…/latest/` core URLs whose bytes have already changed upstream; that
is precisely why Release 002 exists, and re-pinning a historical definition would destroy the record
rather than fix anything.

| Generation | Release id | Sequence | Superseded by | Why |
| --- | --- | --- | --- | --- |
| `runtime-release-001.json` | `rf-runtime-1.22.2-linux-x86_64-001` | 1 | Release 002 | Added the managed `joypad-autoconfig` component, which changes the authenticated contents, and replaced the four rolling nightly core inputs with the version-addressed stable core bundle. |
