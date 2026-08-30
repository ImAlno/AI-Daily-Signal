PRAGMA foreign_keys = OFF;
CREATE TABLE generation_attempts_replacement (
 id TEXT PRIMARY KEY, profile_id TEXT REFERENCES model_profiles(id) ON DELETE SET NULL, provider TEXT NOT NULL, model TEXT NOT NULL, endpoint TEXT, dialect TEXT, usage_date TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('reserved','completed','failed')), final_outcome TEXT CHECK(final_outcome IS NULL OR final_outcome IN ('completed','failed_charged','failed_uncharged')),
 estimated_cost_microusd INTEGER NOT NULL CHECK(estimated_cost_microusd>=0), actual_cost_microusd INTEGER CHECK(actual_cost_microusd IS NULL OR actual_cost_microusd>=0), input_tokens INTEGER CHECK(input_tokens IS NULL OR input_tokens>=0), output_tokens INTEGER CHECK(output_tokens IS NULL OR output_tokens>=0),
 failure_kind TEXT CHECK(failure_kind IS NULL OR failure_kind IN ('cancelled','credential_missing','authentication','rate_limited','timeout','transport','provider_rejected','provider_unavailable','malformed_output')), reserved_at TEXT NOT NULL, expires_at TEXT NOT NULL, finalized_at TEXT,
 CHECK ((status='reserved' AND final_outcome IS NULL AND actual_cost_microusd IS NULL AND input_tokens IS NULL AND output_tokens IS NULL AND failure_kind IS NULL AND finalized_at IS NULL) OR (status='completed' AND final_outcome IS NOT NULL AND final_outcome='completed' AND actual_cost_microusd IS NOT NULL AND failure_kind IS NULL AND finalized_at IS NOT NULL) OR (status='failed' AND final_outcome IS NOT NULL AND final_outcome='failed_charged' AND actual_cost_microusd IS NOT NULL AND input_tokens IS NULL AND output_tokens IS NULL AND failure_kind IS NOT NULL AND finalized_at IS NOT NULL) OR (status='failed' AND final_outcome IS NOT NULL AND final_outcome='failed_uncharged' AND actual_cost_microusd=0 AND input_tokens IS NULL AND output_tokens IS NULL AND failure_kind IS NOT NULL AND finalized_at IS NOT NULL))
);
INSERT INTO generation_attempts_replacement SELECT * FROM generation_attempts;
DROP TABLE generation_attempts;
ALTER TABLE generation_attempts_replacement RENAME TO generation_attempts;
CREATE INDEX generation_attempts_budget_lookup ON generation_attempts (usage_date, profile_id, status);
PRAGMA foreign_keys = ON;
