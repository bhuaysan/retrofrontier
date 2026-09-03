-- M9 Save-State provenance and durable launch baselines.
--
-- These tables hold *proved provenance*, never discovered filesystem facts. A row exists only
-- because a controlled launch established which game, content unit, play session, core, exact core
-- binary, and Runtime Release produced the exact physical state content it names. Nothing here is
-- ever written from a filename resemblance, a slot suffix, a directory name, or a timestamp.
--
-- The split of authority is deliberate and is enforced by both sides needing to agree:
--   * the filesystem is authoritative for physical existence and bytes;
--   * SQLite is authoritative for RetroFrontier provenance, lifecycle history, identity, and the
--     registered file identity.
-- A row with no matching physical file is not loadable, and a file with no proved provenance is
-- not a managed Save State.
--
-- Deletes stay restrictive for the same reason M4/M5/M7 use RESTRICT: missing content and removed
-- roots must not cascade away user-owned history. There is deliberately no cascade anywhere in
-- this migration, including between a baseline and its own entries.

CREATE TABLE IF NOT EXISTS save_states (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Content-unit provenance is mandatory, so a Disc 1 state can never be offered as a Disc 2
    -- state and no cross-disc compatibility is ever inferred.
    game_id INTEGER NOT NULL,
    content_unit_id INTEGER NOT NULL,
    play_session_id INTEGER NOT NULL,

    core_id TEXT NOT NULL,
    core_component_id TEXT NOT NULL,
    -- The decisive core identity. Immutable for the life of the row: no repository method updates
    -- it, and a proved change of core binary at the same physical path supersedes this row and
    -- inserts a new one rather than rewriting this value.
    core_binary_sha256 TEXT NOT NULL CHECK (length(core_binary_sha256) = 64),
    -- Trustworthy human-readable labels from the authenticated release manifest, recorded so a
    -- state stays describable after its originating Runtime Release is gone. Never the load
    -- identity.
    core_display_version TEXT,
    core_source_revision TEXT,
    originating_runtime_release_id TEXT NOT NULL,

    -- Only manual slots are managed. RetroArch's unnumbered base state (slot 0) and its automatic
    -- state are out of scope, and the database refuses to represent either.
    slot INTEGER NOT NULL CHECK (slot BETWEEN 1 AND 999),

    state_relative_path TEXT NOT NULL,
    state_sha256 TEXT NOT NULL CHECK (length(state_sha256) = 64),
    state_size INTEGER NOT NULL CHECK (state_size >= 0),

    -- A thumbnail is proved as a whole or not at all, so its three columns move together.
    thumbnail_relative_path TEXT,
    thumbnail_sha256 TEXT CHECK (thumbnail_sha256 IS NULL OR length(thumbnail_sha256) = 64),
    thumbnail_size INTEGER CHECK (thumbnail_size IS NULL OR thumbnail_size >= 0),

    status TEXT NOT NULL
        CHECK (status IN ('available', 'missing', 'superseded', 'deleted')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    CHECK ((thumbnail_relative_path IS NULL) = (thumbnail_sha256 IS NULL)),
    CHECK ((thumbnail_relative_path IS NULL) = (thumbnail_size IS NULL)),

    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT,
    FOREIGN KEY (content_unit_id) REFERENCES content_units (id) ON DELETE RESTRICT,
    FOREIGN KEY (play_session_id) REFERENCES play_sessions (id) ON DELETE RESTRICT
);

-- Reconciliation is idempotent by construction: one session may register one exact physical
-- identity once. Replaying a completed reconciliation therefore cannot produce a duplicate row,
-- whatever the application layer does.
CREATE UNIQUE INDEX IF NOT EXISTS idx_save_states_session_identity
    ON save_states (play_session_id, state_relative_path, state_sha256);

-- At most one *available* row may claim one physical path. A superseded predecessor keeps sitting
-- beside its successor as history, which is exactly the "same slot, different core binary"
-- shape, but only one row ever claims the live file.
CREATE UNIQUE INDEX IF NOT EXISTS idx_save_states_available_path
    ON save_states (state_relative_path) WHERE status = 'available';

CREATE INDEX IF NOT EXISTS idx_save_states_game_recent
    ON save_states (game_id, updated_at DESC, id DESC);

-- The durable pre-launch baseline.
--
-- It must exist durably *before* RetroArch is spawned and must survive a RetroFrontier restart, so
-- a process adopted after a crash can still be reconciled once it certainly ends. If it cannot be
-- created, the launch fails before anything is spawned.
CREATE TABLE IF NOT EXISTS launch_state_baselines (
    play_session_id INTEGER PRIMARY KEY,

    game_id INTEGER NOT NULL,
    content_unit_id INTEGER NOT NULL,
    core_id TEXT NOT NULL,
    core_component_id TEXT NOT NULL,
    core_binary_sha256 TEXT NOT NULL CHECK (length(core_binary_sha256) = 64),
    core_display_version TEXT,
    core_source_revision TEXT,
    runtime_installation_id TEXT NOT NULL,
    runtime_release_id TEXT NOT NULL,

    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    -- How many times reconciliation ran for this baseline without reaching a deterministic
    -- outcome. Bounded by the application layer so a permanently indeterminate baseline cannot
    -- leak forever.
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    captured_at INTEGER NOT NULL,

    FOREIGN KEY (play_session_id) REFERENCES play_sessions (id) ON DELETE RESTRICT,
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT,
    FOREIGN KEY (content_unit_id) REFERENCES content_units (id) ON DELETE RESTRICT
);

-- One baseline entry per state-tree file that existed before the launch.
--
-- Cheap physical identity rather than a digest, deliberately: the approved reconciliation order
-- computes SHA-256 after the process ended, and pre-hashing an entire state tree before every
-- launch would add unbounded launch latency without improving provenance. A size-, mtime- and
-- inode-preserving external rewrite is therefore invisible to the delta — which fails closed,
-- because such a file is simply never attributed to the session.
CREATE TABLE IF NOT EXISTS launch_state_baseline_entries (
    play_session_id INTEGER NOT NULL,
    relative_path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    mtime_nanos TEXT NOT NULL,
    inode INTEGER NOT NULL,

    PRIMARY KEY (play_session_id, relative_path),
    FOREIGN KEY (play_session_id)
        REFERENCES launch_state_baselines (play_session_id) ON DELETE RESTRICT
);
