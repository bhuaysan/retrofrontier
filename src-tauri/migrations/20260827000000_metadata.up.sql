-- M5 metadata schema.
--
-- Provider state is strictly downstream of the M4 local library. Every table references
-- games (id) with ON DELETE RESTRICT so provider rows can never cascade into local identity, and
-- no table stores credential values, authenticated provider URLs, or raw provider payloads.

CREATE TABLE IF NOT EXISTS provider_matches (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id INTEGER NOT NULL,
    provider_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN (
        'pending', 'matched', 'no_match', 'ambiguous', 'deferred', 'failed', 'stale'
    )),
    match_type TEXT CHECK (match_type IN (
        'deterministic_sha1', 'deterministic_md5', 'deterministic_crc32', 'heuristic_user_confirmed'
    )),
    provider_game_id TEXT,
    provider_rom_id TEXT,
    unsupported_reason TEXT,
    last_failure TEXT,
    last_checked_at INTEGER,
    last_matched_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (game_id, provider_id),
    -- An accepted match must carry both the evidence class that justified it and the provider
    -- identity it points at. Enforced here as well as in Rust so a hand-edited or corrupted row
    -- can never be loaded as a trusted relationship.
    CHECK (status != 'matched' OR (match_type IS NOT NULL AND provider_game_id IS NOT NULL)),
    CHECK (unsupported_reason IS NULL OR unsupported_reason IN (
        'system_not_mapped', 'chd_representation_undefined', 'cue_bin_representation_undefined',
        'gdi_representation_undefined', 'playlist_is_not_identity',
        'container_representation_undefined', 'missing_content_evidence',
        'no_primary_content_file'
    )),
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_provider_matches_provider_status
    ON provider_matches (provider_id, status);

-- Evidence that justified the current provider relationship. A match is only trusted while this
-- snapshot still agrees with the live M4 evidence for the same content unit.
CREATE TABLE IF NOT EXISTS provider_match_evidence (
    provider_match_id INTEGER PRIMARY KEY,
    game_id INTEGER NOT NULL,
    content_unit_id INTEGER NOT NULL,
    system_id TEXT NOT NULL,
    content_unit_kind TEXT NOT NULL,
    content_file_id INTEGER,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    crc32 TEXT,
    md5 TEXT,
    sha1 TEXT,
    fingerprint TEXT,
    match_type TEXT NOT NULL CHECK (match_type IN (
        'deterministic_sha1', 'deterministic_md5', 'deterministic_crc32', 'heuristic_user_confirmed'
    )),
    evidence_version INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    -- Every stored snapshot must carry at least one comparable hash, or it could never be
    -- revalidated against replaced content.
    CHECK (crc32 IS NOT NULL OR md5 IS NOT NULL OR sha1 IS NOT NULL
           OR match_type = 'heuristic_user_confirmed'),
    FOREIGN KEY (provider_match_id) REFERENCES provider_matches (id) ON DELETE CASCADE,
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT,
    FOREIGN KEY (content_unit_id) REFERENCES content_units (id) ON DELETE RESTRICT,
    FOREIGN KEY (content_file_id) REFERENCES content_files (id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_provider_match_evidence_unit
    ON provider_match_evidence (content_unit_id);

-- Heuristic name-search candidates. These are inspectable suggestions and never an attachment.
CREATE TABLE IF NOT EXISTS provider_match_candidates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_match_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    provider_game_id TEXT NOT NULL,
    title TEXT NOT NULL,
    release_date TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE (provider_match_id, ordinal),
    FOREIGN KEY (provider_match_id) REFERENCES provider_matches (id) ON DELETE CASCADE
);

-- Replaceable, provider-derived normalized metadata. Kept deliberately small for M6.
CREATE TABLE IF NOT EXISTS provider_metadata (
    game_id INTEGER NOT NULL,
    provider_id TEXT NOT NULL,
    provider_game_id TEXT NOT NULL,
    title TEXT NOT NULL,
    sort_title TEXT,
    synopsis TEXT,
    release_date TEXT,
    developer TEXT,
    publisher TEXT,
    genre TEXT,
    players TEXT,
    region TEXT,
    source_credit TEXT,
    fetched_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (game_id, provider_id),
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT
);

-- Exactly one primary cover per game and provider in V1. The row records the app-owned cache
-- identity, never a credential-bearing provider URL.
CREATE TABLE IF NOT EXISTS provider_media_assets (
    game_id INTEGER NOT NULL,
    provider_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('cover')),
    state TEXT NOT NULL CHECK (state IN ('cached', 'missing', 'failed')),
    provider_media_type TEXT,
    region TEXT,
    cache_relative_path TEXT,
    content_type TEXT,
    size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    content_sha256 TEXT,
    provider_crc32 TEXT,
    provider_md5 TEXT,
    provider_sha1 TEXT,
    source_credit TEXT,
    last_failure TEXT,
    fetched_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (game_id, provider_id, kind),
    -- A cached asset must name the file that backs it.
    CHECK (state != 'cached' OR cache_relative_path IS NOT NULL),
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT
);

-- Restart-safe provider work. At most one live job per game, provider, and kind.
CREATE TABLE IF NOT EXISTS metadata_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id INTEGER NOT NULL,
    provider_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('identify', 'refresh_metadata', 'refresh_cover')),
    state TEXT NOT NULL CHECK (state IN (
        'pending', 'running', 'deferred', 'failed', 'completed'
    )),
    priority INTEGER NOT NULL DEFAULT 100,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_failure TEXT,
    earliest_next_attempt_at INTEGER,
    claimed_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (game_id, provider_id, kind),
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_metadata_jobs_ready
    ON metadata_jobs (provider_id, state, earliest_next_attempt_at, priority, id);

-- One dynamic quota/scheduling snapshot per provider. Values are provider-reported and mutable;
-- no researched maximum is hard-coded into the schema.
CREATE TABLE IF NOT EXISTS provider_scheduler_state (
    provider_id TEXT NOT NULL PRIMARY KEY,
    max_threads INTEGER,
    max_requests_per_minute INTEGER,
    max_requests_per_day INTEGER,
    max_negative_requests_per_day INTEGER,
    requests_today INTEGER,
    negative_requests_today INTEGER,
    observed_at INTEGER,
    deferred_until INTEGER,
    defer_reason TEXT,
    consecutive_transport_failures INTEGER NOT NULL DEFAULT 0
        CHECK (consecutive_transport_failures >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Non-secret record of an optional personal provider account. The password never reaches SQLite;
-- `vault_reference` is an opaque key into the OS credential vault.
CREATE TABLE IF NOT EXISTS provider_user_accounts (
    provider_id TEXT NOT NULL PRIMARY KEY,
    vault_reference TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('configured', 'invalid')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- User-owned decisions are stored separately from replaceable provider-derived data. A provider
-- refresh must never create, change, or delete a row here.
CREATE TABLE IF NOT EXISTS user_provider_selections (
    game_id INTEGER NOT NULL,
    provider_id TEXT NOT NULL,
    provider_game_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (game_id, provider_id),
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT
);
