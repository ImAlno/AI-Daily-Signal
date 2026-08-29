CREATE TABLE summary_variants (
    id TEXT PRIMARY KEY,
    story_id TEXT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    profile_id TEXT REFERENCES model_profiles(id) ON DELETE SET NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    endpoint TEXT,
    dialect TEXT,
    prompt_version TEXT NOT NULL,
    cache_key TEXT NOT NULL,
    what_happened TEXT NOT NULL,
    why_it_matters TEXT NOT NULL,
    caveat TEXT,
    input_tokens INTEGER CHECK (input_tokens IS NULL OR input_tokens >= 0),
    output_tokens INTEGER CHECK (output_tokens IS NULL OR output_tokens >= 0),
    cost_microusd INTEGER NOT NULL CHECK (cost_microusd >= 0),
    generated_at TEXT NOT NULL
);

CREATE INDEX summary_variants_cache_lookup
ON summary_variants (cache_key, generated_at DESC, id ASC);

CREATE INDEX summary_variants_story
ON summary_variants (story_id, generated_at DESC, id ASC);

CREATE TRIGGER summary_variants_immutable_content
BEFORE UPDATE OF
    id, story_id, provider, model, endpoint, dialect, prompt_version, cache_key,
    what_happened, why_it_matters, caveat, input_tokens, output_tokens, cost_microusd, generated_at
ON summary_variants
BEGIN
    SELECT RAISE(ABORT, 'summary variants are immutable');
END;

CREATE TABLE generation_attempts (
    id TEXT PRIMARY KEY,
    profile_id TEXT REFERENCES model_profiles(id) ON DELETE SET NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    endpoint TEXT,
    dialect TEXT,
    usage_date TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('reserved', 'completed', 'failed')),
    final_outcome TEXT CHECK (
        final_outcome IS NULL OR final_outcome IN (
            'completed', 'failed_charged', 'failed_uncharged'
        )
    ),
    estimated_cost_microusd INTEGER NOT NULL CHECK (estimated_cost_microusd >= 0),
    actual_cost_microusd INTEGER CHECK (actual_cost_microusd IS NULL OR actual_cost_microusd >= 0),
    input_tokens INTEGER CHECK (input_tokens IS NULL OR input_tokens >= 0),
    output_tokens INTEGER CHECK (output_tokens IS NULL OR output_tokens >= 0),
    failure_kind TEXT CHECK (
        failure_kind IS NULL OR failure_kind IN (
            'credential_missing', 'authentication', 'rate_limited', 'timeout', 'transport',
            'provider_rejected', 'provider_unavailable', 'malformed_output'
        )
    ),
    reserved_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    finalized_at TEXT,
    CHECK (
        (status = 'reserved' AND final_outcome IS NULL AND actual_cost_microusd IS NULL
            AND input_tokens IS NULL AND output_tokens IS NULL AND failure_kind IS NULL
            AND finalized_at IS NULL)
        OR (status = 'completed' AND final_outcome IS NOT NULL
            AND final_outcome = 'completed'
            AND actual_cost_microusd IS NOT NULL AND failure_kind IS NULL
            AND finalized_at IS NOT NULL)
        OR (status = 'failed' AND final_outcome IS NOT NULL
            AND final_outcome = 'failed_charged'
            AND actual_cost_microusd IS NOT NULL
            AND input_tokens IS NULL AND output_tokens IS NULL
            AND failure_kind IS NOT NULL AND finalized_at IS NOT NULL)
        OR (status = 'failed' AND final_outcome IS NOT NULL
            AND final_outcome = 'failed_uncharged'
            AND actual_cost_microusd = 0
            AND input_tokens IS NULL AND output_tokens IS NULL
            AND failure_kind IS NOT NULL AND finalized_at IS NOT NULL)
    )
);

CREATE INDEX generation_attempts_budget_lookup
ON generation_attempts (usage_date, profile_id, status);

ALTER TABLE briefing_items
ADD COLUMN selected_summary_variant_id TEXT REFERENCES summary_variants(id) ON DELETE SET NULL;
