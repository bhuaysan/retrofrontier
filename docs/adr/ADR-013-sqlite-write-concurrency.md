# ADR-013: SQLite write concurrency for background metadata writes

- Status: Accepted

## Context

Through M4 the database had one effective writer: an interactive scan or root change. M5 adds a
background metadata worker that writes provider matches, evidence, normalized metadata, media rows,
job transitions, and quota snapshots while the user is browsing the library. The M4 review deferred
the write-concurrency question to this milestone.

SQLite in its default rollback journal mode blocks readers during a write and returns
`SQLITE_BUSY` immediately when a second writer arrives. With a background writer present, that would
surface as intermittent "database is unavailable" errors during ordinary library use.

## Decision

Open the pool with `journal_mode = WAL`, `synchronous = NORMAL`, a 10 second busy timeout, and
foreign keys enforced, and keep every writer short.

- **WAL** lets interactive reads proceed while a metadata transaction commits, which is the actual
  contention shape: many short reads, occasional short writes.
- **Busy timeout** makes the losing writer wait instead of failing. SQLite still permits one writer
  at a time; the timeout is what turns that into a queue rather than an error.
- **`NORMAL` synchronous** is the documented safe pairing for WAL. It can lose the most recent
  commits after an OS or power failure but never corrupts the database. Provider metadata is
  replaceable and refetchable, and the local library is rebuilt from a scan, so this trade is
  acceptable.
- **Short transactions.** No provider request is issued while a transaction is open. Job claiming is
  one transaction, and each persistence step is its own small transaction.
- **Bounded worker concurrency**, further limited by the provider's advertised thread count, caps
  how many writers can compete at all.

Serializing all writes through a single dedicated writer task was considered and rejected as
premature: it would add a channel and a lifecycle to own for contention that WAL plus a busy timeout
already handles, and it would not remove the need for either setting.

The behaviour is asserted by tests rather than assumed: the pragmas are verified after open, and a
regression test runs a background metadata writer concurrently with interactive library reads and
writes and requires all of them to succeed.

## Consequences

Background enrichment cannot make ordinary library operations unreliable. The database gains `-wal`
and `-shm` sidecar files, which are already git-ignored and must be included wherever the database
file is copied. WAL requires a local filesystem with working shared memory, which matches the
existing V1 constraint that application data is not placed on a network share or a
cloud-synchronized root. The database layer is otherwise unchanged.
