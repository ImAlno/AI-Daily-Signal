CREATE TABLE model_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    endpoint TEXT,
    dialect TEXT,
    credential_kind TEXT NOT NULL,
    credential_service TEXT,
    credential_account TEXT,
    credential_variable TEXT,
    consented_at TEXT,
    enabled INTEGER NOT NULL,
    max_summaries_per_refresh INTEGER NOT NULL,
    max_daily_cost_microusd INTEGER,
    input_cost_microusd_per_million INTEGER,
    output_cost_microusd_per_million INTEGER,
    max_output_tokens INTEGER NOT NULL,
    timeout_seconds INTEGER NOT NULL,
    max_retries INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX model_profiles_name_case_insensitive
ON model_profiles (lower(name));

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value TEXT
);
