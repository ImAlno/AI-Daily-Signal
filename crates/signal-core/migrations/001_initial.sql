CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS stories (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    canonical_url TEXT NOT NULL UNIQUE,
    excerpt TEXT NOT NULL,
    category TEXT NOT NULL,
    published_at TEXT,
    source_ids_json TEXT NOT NULL,
    score_json TEXT NOT NULL,
    smart_summary TEXT NOT NULL,
    is_read INTEGER NOT NULL DEFAULT 0,
    is_saved INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS briefings (
    date TEXT PRIMARY KEY,
    generated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS briefing_items (
    briefing_date TEXT NOT NULL REFERENCES briefings(date) ON DELETE CASCADE,
    story_id TEXT NOT NULL REFERENCES stories(id),
    position INTEGER NOT NULL,
    section TEXT NOT NULL,
    PRIMARY KEY (briefing_date, position)
);
CREATE TABLE IF NOT EXISTS refresh_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    successful_sources INTEGER NOT NULL DEFAULT 0,
    failed_sources INTEGER NOT NULL DEFAULT 0,
    error_json TEXT
);
INSERT OR IGNORE INTO metadata(key, value) VALUES ('data_generation', '0');
