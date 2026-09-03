-- M8.5 user-initiated metadata scrape runs.
--
-- A scrape run is the *user-initiated batch operation*; `metadata_jobs` remains the concrete
-- provider work M5 executes. This schema owns the first and only feeds the second, so no provider
-- policy, matching rule, or quota behaviour moves here.

CREATE TABLE IF NOT EXISTS metadata_scrape_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL,
    mode TEXT NOT NULL CHECK (mode IN ('missing_metadata', 'refresh_matched')),
    status TEXT NOT NULL CHECK (status IN (
        'preparing', 'running', 'stopping', 'completed', 'stopped'
    )),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    finished_at INTEGER,
    -- A finished run must record when it finished, and an unfinished one must not pretend to have.
    CHECK ((status IN ('completed', 'stopped')) = (finished_at IS NOT NULL))
);

-- At most one active run per provider, enforced by the database rather than by Rust alone: two
-- concurrent starts would otherwise both pass an application-level check and both feed the queue.
CREATE UNIQUE INDEX IF NOT EXISTS idx_metadata_scrape_runs_active
    ON metadata_scrape_runs (provider_id)
    WHERE status IN ('preparing', 'running', 'stopping');

CREATE INDEX IF NOT EXISTS idx_metadata_scrape_runs_recent
    ON metadata_scrape_runs (provider_id, id DESC);

-- The run's fixed target set, decided once when the run starts.
--
-- Membership is never appended to. Games discovered by a later library scan belong to a future run,
-- not to the one already in progress, so an active run cannot grow without bound as the library
-- changes.
CREATE TABLE IF NOT EXISTS metadata_scrape_run_items (
    run_id INTEGER NOT NULL,
    game_id INTEGER NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'pending', 'queued', 'running',
        'matched', 'needs_review', 'no_match', 'unsupported', 'failed'
    )),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (run_id, game_id),
    FOREIGN KEY (run_id) REFERENCES metadata_scrape_runs (id) ON DELETE CASCADE,
    -- Unlike provider state, a run item is bookkeeping about a batch operation rather than a
    -- provider relationship. It must never be the reason local library identity cannot be
    -- reconciled, so it follows the game rather than restraining it.
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE CASCADE
);

-- Serves both the bounded feeder (pending items for one run) and progress aggregation
-- (count per state for one run).
CREATE INDEX IF NOT EXISTS idx_metadata_scrape_run_items_state
    ON metadata_scrape_run_items (run_id, state, game_id);

-- Which bulk run exclusively owns a queued job, when one does.
--
-- NULL means the job is interactive: either it was created by an explicit per-game action, or a
-- later explicit action promoted it out of its run. Stopping a run detaches only the jobs it still
-- owns, so work the user has claimed by hand is never cancelled underneath them.
ALTER TABLE metadata_jobs
    ADD COLUMN bulk_run_id INTEGER
    REFERENCES metadata_scrape_runs (id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_metadata_jobs_bulk_run
    ON metadata_jobs (bulk_run_id, state)
    WHERE bulk_run_id IS NOT NULL;
