pub const CURRENT_SCHEMA_VERSION: i64 = 1;

pub const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;

CREATE TABLE IF NOT EXISTS app_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS previews (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    snapshot_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS plans (
    id TEXT PRIMARY KEY,
    preview_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    plan_json TEXT NOT NULL,
    FOREIGN KEY(preview_id) REFERENCES previews(id)
);

CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    plan_id TEXT NOT NULL,
    state TEXT NOT NULL,
    sequence INTEGER NOT NULL DEFAULT 0,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    message TEXT NOT NULL DEFAULT '',
    summary_json TEXT,
    FOREIGN KEY(plan_id) REFERENCES plans(id)
);

CREATE TABLE IF NOT EXISTS job_items (
    job_id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    state TEXT NOT NULL,
    outcome TEXT,
    message TEXT NOT NULL DEFAULT '',
    error_json TEXT,
    item_json TEXT NOT NULL,
    PRIMARY KEY(job_id, item_id),
    FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    resource TEXT NOT NULL,
    state TEXT NOT NULL,
    depends_json TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    attempt INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL,
    weight REAL NOT NULL,
    progress_json TEXT,
    message TEXT NOT NULL DEFAULT '',
    error_json TEXT,
    started_at TEXT,
    finished_at TEXT,
    PRIMARY KEY(job_id, id),
    FOREIGN KEY(job_id, item_id) REFERENCES job_items(job_id, item_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    path TEXT NOT NULL,
    checksum TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    artifact_json TEXT NOT NULL,
    FOREIGN KEY(job_id, item_id) REFERENCES job_items(job_id, item_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS cache_entries (
    cache_key TEXT PRIMARY KEY,
    item_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    path TEXT NOT NULL,
    checksum TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    completed INTEGER NOT NULL,
    last_used_at TEXT NOT NULL,
    metadata_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    job_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    occurred_at TEXT NOT NULL,
    event_type TEXT NOT NULL,
    item_id TEXT,
    task_id TEXT,
    event_json TEXT NOT NULL,
    PRIMARY KEY(job_id, sequence),
    FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_state (
    tool TEXT PRIMARY KEY,
    state_json TEXT NOT NULL,
    checked_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jobs_started ON jobs(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_tasks_job_state ON tasks(job_id, state);
CREATE INDEX IF NOT EXISTS idx_tasks_item ON tasks(job_id, item_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_item_kind ON artifacts(item_id, kind, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_cache_last_used ON cache_entries(last_used_at);
CREATE INDEX IF NOT EXISTS idx_events_job_sequence ON events(job_id, sequence);

PRAGMA user_version = 1;
"#;
