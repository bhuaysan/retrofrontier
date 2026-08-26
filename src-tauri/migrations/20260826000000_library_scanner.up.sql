CREATE TABLE IF NOT EXISTS content_roots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('managed', 'external')),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    system_hint TEXT,
    availability TEXT NOT NULL DEFAULT 'unavailable'
        CHECK (availability IN ('available', 'partially_available', 'unavailable', 'disabled', 'unsafe')),
    last_scan_at INTEGER,
    last_successful_scan_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_content_roots_enabled ON content_roots (enabled);

CREATE TABLE IF NOT EXISTS games (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    system_id TEXT NOT NULL,
    local_title TEXT NOT NULL,
    availability TEXT NOT NULL DEFAULT 'unavailable'
        CHECK (availability IN ('available', 'unavailable')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_games_system_availability
    ON games (system_id, availability);

CREATE TABLE IF NOT EXISTS content_units (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id INTEGER NOT NULL,
    root_id INTEGER NOT NULL,
    system_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('single_file', 'chd', 'cue_bin', 'gdi', 'm3u')),
    local_title TEXT NOT NULL,
    primary_relative_path TEXT NOT NULL,
    fingerprint TEXT,
    availability TEXT NOT NULL DEFAULT 'missing'
        CHECK (availability IN ('available', 'incomplete', 'missing')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (root_id, primary_relative_path),
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT,
    FOREIGN KEY (root_id) REFERENCES content_roots (id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_content_units_root_availability
    ON content_units (root_id, availability);
CREATE INDEX IF NOT EXISTS idx_content_units_game ON content_units (game_id);
CREATE INDEX IF NOT EXISTS idx_content_units_fingerprint
    ON content_units (system_id, kind, fingerprint);

CREATE TABLE IF NOT EXISTS content_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    relative_path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    modified_at INTEGER NOT NULL,
    crc32 TEXT,
    md5 TEXT,
    sha1 TEXT,
    availability TEXT NOT NULL DEFAULT 'missing'
        CHECK (availability IN ('available', 'unavailable', 'missing')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (root_id, relative_path),
    FOREIGN KEY (root_id) REFERENCES content_roots (id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_content_files_root_availability
    ON content_files (root_id, availability);
CREATE INDEX IF NOT EXISTS idx_content_files_identity
    ON content_files (root_id, size_bytes, crc32, md5, sha1);

CREATE TABLE IF NOT EXISTS content_unit_files (
    content_unit_id INTEGER NOT NULL,
    content_file_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    role TEXT NOT NULL CHECK (role IN (
        'standalone', 'descriptor', 'track', 'playlist', 'disc', 'disc_descriptor', 'disc_track'
    )),
    PRIMARY KEY (content_unit_id, ordinal),
    FOREIGN KEY (content_unit_id) REFERENCES content_units (id) ON DELETE RESTRICT,
    FOREIGN KEY (content_file_id) REFERENCES content_files (id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_content_unit_files_file
    ON content_unit_files (content_file_id);

CREATE TABLE IF NOT EXISTS scan_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed')),
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    roots_discovered INTEGER NOT NULL DEFAULT 0,
    roots_completed INTEGER NOT NULL DEFAULT 0,
    files_discovered INTEGER NOT NULL DEFAULT 0,
    files_processed INTEGER NOT NULL DEFAULT 0,
    files_hashed INTEGER NOT NULL DEFAULT 0,
    bytes_hashed INTEGER NOT NULL DEFAULT 0,
    issues_found INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_scan_runs_started ON scan_runs (started_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS scan_issues (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_run_id INTEGER NOT NULL,
    root_id INTEGER,
    kind TEXT NOT NULL,
    relative_path TEXT,
    related_path TEXT,
    detail TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (scan_run_id) REFERENCES scan_runs (id) ON DELETE RESTRICT,
    FOREIGN KEY (root_id) REFERENCES content_roots (id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_scan_issues_run ON scan_issues (scan_run_id, id);
CREATE INDEX IF NOT EXISTS idx_scan_issues_root ON scan_issues (root_id, id);
