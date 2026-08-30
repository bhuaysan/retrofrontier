-- M7 launch persistence.
--
-- Both tables hold user-owned or product-history state. They are deliberately separate from the
-- scanner-owned library tables and the provider-owned metadata tables so a scan or a metadata
-- refresh can never reset a user's core choice or delete play history. Deletes stay restrictive
-- for the same reason M4/M5 use RESTRICT: missing content must not cascade away history.
CREATE TABLE IF NOT EXISTS game_launch_overrides (
    game_id INTEGER PRIMARY KEY,
    core_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS play_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id INTEGER NOT NULL,
    content_unit_id INTEGER NOT NULL,
    core_id TEXT NOT NULL,
    runtime_installation_id TEXT NOT NULL,
    runtime_release_id TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    exit_code INTEGER,
    outcome TEXT NOT NULL
        CHECK (outcome IN ('running', 'completed', 'failed_to_start', 'crashed', 'interrupted')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    -- An open session has no end; a closed session always has one. This keeps history readable
    -- without making the row an authority on whether a process is alive.
    CHECK ((outcome = 'running' AND ended_at IS NULL)
        OR (outcome <> 'running' AND ended_at IS NOT NULL)),
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT,
    FOREIGN KEY (content_unit_id) REFERENCES content_units (id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_play_sessions_game
    ON play_sessions (game_id, started_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_play_sessions_open
    ON play_sessions (id) WHERE outcome = 'running';
