-- M6.1 user-owned library UI state.
--
-- Scanner reconciliation owns local library identity in `games`; this table is deliberately
-- separate so a scan can never reset a user's favorite decision.
CREATE TABLE IF NOT EXISTS game_user_state (
    game_id INTEGER PRIMARY KEY,
    favorite INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_game_user_state_favorite
    ON game_user_state (favorite, game_id);
