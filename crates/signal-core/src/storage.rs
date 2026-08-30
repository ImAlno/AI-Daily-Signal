use std::{path::PathBuf, time::Duration};

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{
    AiSummaryFields, ApiDialect, AttemptOutcome, Briefing, BriefingItem, BudgetDecision,
    BudgetReservation, CredentialRef, GenerationAttempt, GenerationFailureKind,
    GenerationOutcomeKind, GenerationStatus, ModelProfile, ProfileLimits, ProviderKind, Result,
    ScoreBreakdown, SignalError, Story, SummarySettings, SummaryVariant,
};

const INITIAL_MIGRATION: &str = include_str!("../migrations/001_initial.sql");
const MIGRATIONS: &[(i64, &str)] = &[
    (
        2,
        include_str!("../migrations/002_briefing_item_staleness.sql"),
    ),
    (3, include_str!("../migrations/003_model_profiles.sql")),
    (4, include_str!("../migrations/004_ai_summaries.sql")),
];

const MODEL_PROFILE_COLUMNS: &str = "
    id, name, provider, model, endpoint, dialect, credential_kind, credential_service,
    credential_account, credential_variable, consented_at, enabled, max_summaries_per_refresh,
    max_daily_cost_microusd, input_cost_microusd_per_million,
    output_cost_microusd_per_million, max_output_tokens, timeout_seconds, max_retries,
    created_at, updated_at";

const SUMMARY_VARIANT_COLUMNS: &str = "
    id, story_id, profile_id, provider, model, endpoint, dialect, prompt_version, cache_key,
    what_happened, why_it_matters, caveat, input_tokens, output_tokens, cost_microusd,
    generated_at";

const GENERATION_ATTEMPT_COLUMNS: &str = "
    id, profile_id, provider, model, endpoint, dialect, usage_date, status, final_outcome,
    estimated_cost_microusd, actual_cost_microusd, input_tokens, output_tokens, failure_kind,
    reserved_at, expires_at, finalized_at";

#[derive(Clone, Debug)]
pub struct Store {
    path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreStatus {
    pub story_count: u64,
    pub last_refresh_at: Option<DateTime<Utc>>,
    pub data_generation: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefreshRun {
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub successful_sources: u64,
    pub failed_sources: u64,
}

impl Store {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }

        let store = Self { path };
        store.apply_migrations()?;
        Ok(store)
    }

    pub fn upsert_stories(&self, stories: &[Story]) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        upsert_stories_in_transaction(&transaction, stories)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn commit_refresh(&self, stories: &[Story], briefing: &Briefing) -> Result<()> {
        self.commit_refresh_transaction(stories, briefing, None)
    }

    pub fn commit_refresh_with_counts(
        &self,
        stories: &[Story],
        briefing: &Briefing,
        successful_sources: usize,
        failed_sources: usize,
    ) -> Result<()> {
        let successful_sources = count_as_i64(successful_sources)?;
        let failed_sources = count_as_i64(failed_sources)?;
        self.commit_refresh_transaction(
            stories,
            briefing,
            Some((successful_sources, failed_sources)),
        )
    }

    fn commit_refresh_transaction(
        &self,
        stories: &[Story],
        briefing: &Briefing,
        source_counts: Option<(i64, i64)>,
    ) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

        upsert_stories_in_transaction(&transaction, stories)?;
        transaction.execute(
            "INSERT INTO briefings (date, generated_at) VALUES (?1, ?2)
             ON CONFLICT(date) DO UPDATE SET generated_at = excluded.generated_at",
            params![
                briefing.date.to_string(),
                briefing.generated_at.to_rfc3339()
            ],
        )?;
        transaction.execute(
            "DELETE FROM briefing_items WHERE briefing_date = ?1",
            [briefing.date.to_string()],
        )?;
        for item in &briefing.items {
            if let Some(selected) = &item.selected_summary
                && selected.story_id != item.story.id
            {
                return Err(SignalError::Storage(
                    "selected summary belongs to a different story".to_owned(),
                ));
            }
            transaction.execute(
                "INSERT INTO briefing_items (
                     briefing_date, story_id, position, section, is_stale,
                     selected_summary_variant_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    briefing.date.to_string(),
                    item.story.id,
                    i64::from(item.position),
                    item.section,
                    item.is_stale,
                    item.selected_summary
                        .as_ref()
                        .map(|variant| variant.id.hyphenated().to_string()),
                ],
            )?;
        }
        bump_data_generation(&transaction)?;
        if let Some((successful_sources, failed_sources)) = source_counts {
            insert_refresh_run(
                &transaction,
                briefing.generated_at,
                successful_sources,
                failed_sources,
                None,
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn record_refresh_failure(
        &self,
        occurred_at: DateTime<Utc>,
        failed_sources: usize,
    ) -> Result<()> {
        let failed_sources = count_as_i64(failed_sources)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        insert_refresh_run(
            &transaction,
            occurred_at,
            0,
            failed_sources,
            Some(r#"{"kind":"all_sources_failed"}"#),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn create_model_profile(&self, profile: &ModelProfile) -> Result<()> {
        profile.validate()?;
        let profile = profile.normalized();
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        insert_model_profile(&transaction, &profile)?;
        bump_data_generation(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn find_cached_summary(&self, cache_key: &str) -> Result<Option<SummaryVariant>> {
        let connection = self.connect()?;
        find_summary_variant(
            &connection,
            &format!(
                "SELECT {SUMMARY_VARIANT_COLUMNS} FROM summary_variants
                 WHERE cache_key = ?1 ORDER BY generated_at DESC, id ASC LIMIT 1"
            ),
            cache_key,
        )
    }

    pub fn insert_summary_variant(&self, variant: &SummaryVariant) -> Result<()> {
        validate_summary_variant(variant)?;
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO summary_variants (
                 id, story_id, profile_id, provider, model, endpoint, dialect, prompt_version,
                 cache_key, what_happened, why_it_matters, caveat, input_tokens, output_tokens,
                 cost_microusd, generated_at
             ) VALUES (
                 ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
             )",
            params![
                variant.id.hyphenated().to_string(),
                variant.story_id,
                variant
                    .profile_id
                    .map(|value| value.hyphenated().to_string()),
                variant.provider.as_storage(),
                variant.model,
                variant.endpoint,
                variant.dialect.map(ApiDialect::as_storage),
                variant.prompt_version,
                variant.cache_key,
                variant.fields.what_happened,
                variant.fields.why_it_matters,
                variant.fields.caveat,
                optional_integer_as_i64(variant.input_tokens)?,
                optional_integer_as_i64(variant.output_tokens)?,
                integer_as_i64(variant.cost_microusd)?,
                variant.generated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_summary_variants(&self, story_id: &str) -> Result<Vec<SummaryVariant>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(&format!(
            "SELECT {SUMMARY_VARIANT_COLUMNS} FROM summary_variants
             WHERE story_id = ?1 ORDER BY generated_at DESC, id ASC"
        ))?;
        let rows = statement.query_map([story_id], summary_variant_row)?;
        collect_summary_variants(rows)
    }

    pub fn reserve_generation(
        &self,
        profile: &ModelProfile,
        attempt_id: uuid::Uuid,
        now: DateTime<Utc>,
        estimated_cost_microusd: u64,
        expires_at: DateTime<Utc>,
    ) -> Result<BudgetDecision> {
        profile.validate()?;
        if expires_at <= now {
            return Err(SignalError::InvalidConfiguration(
                "budget reservation expiry must be after its start".to_owned(),
            ));
        }
        let estimate = integer_as_i64(estimated_cost_microusd)?;
        let daily_limit = profile
            .limits
            .max_daily_cost_microusd
            .map(integer_as_i64)
            .transpose()?;
        let usage_date = now.date_naive();
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let committed_or_reserved = transaction.query_row(
            "SELECT COALESCE(SUM(
                 CASE
                     WHEN status = 'reserved' AND expires_at > ?3
                         THEN estimated_cost_microusd
                     WHEN status IN ('completed', 'failed')
                         THEN actual_cost_microusd
                     ELSE 0
                 END
             ), 0)
             FROM generation_attempts
             WHERE profile_id = ?1 AND usage_date = ?2",
            params![
                profile.id.hyphenated().to_string(),
                usage_date.to_string(),
                now.to_rfc3339(),
            ],
            |row| row.get::<_, i64>(0),
        )?;
        let projected = committed_or_reserved.checked_add(estimate).ok_or_else(|| {
            SignalError::Storage("daily generation budget arithmetic overflow".to_owned())
        })?;
        if daily_limit.is_some_and(|limit| projected > limit) {
            transaction.commit()?;
            return Ok(BudgetDecision::Exhausted);
        }

        transaction.execute(
            "INSERT INTO generation_attempts (
                 id, profile_id, provider, model, endpoint, dialect, usage_date, status,
                 final_outcome, estimated_cost_microusd, actual_cost_microusd, input_tokens,
                 output_tokens, failure_kind, reserved_at, expires_at, finalized_at
             ) VALUES (
                 ?1, ?2, ?3, ?4, ?5, ?6, ?7, 'reserved', NULL, ?8, NULL, NULL, NULL, NULL,
                 ?9, ?10, NULL
             )",
            params![
                attempt_id.hyphenated().to_string(),
                profile.id.hyphenated().to_string(),
                profile.provider.as_storage(),
                profile.model,
                profile.endpoint.as_ref().map(url::Url::as_str),
                profile.dialect.map(ApiDialect::as_storage),
                usage_date.to_string(),
                estimate,
                now.to_rfc3339(),
                expires_at.to_rfc3339(),
            ],
        )?;
        transaction.commit()?;
        Ok(BudgetDecision::Reserved(BudgetReservation {
            attempt_id,
            profile_id: profile.id,
            usage_date,
            estimated_cost_microusd,
            reserved_at: now,
            expires_at,
        }))
    }

    pub fn finalize_generation(
        &self,
        attempt_id: uuid::Uuid,
        finalized_at: DateTime<Utc>,
        outcome: AttemptOutcome,
    ) -> Result<GenerationAttempt> {
        let finalized = finalized_values(&outcome)?;
        let connection = self.connect()?;
        let changed = connection.execute(
            "UPDATE generation_attempts SET
                 status = ?1, final_outcome = ?2, actual_cost_microusd = ?3, input_tokens = ?4,
                 output_tokens = ?5, failure_kind = ?6, finalized_at = ?7
             WHERE id = ?8 AND status = 'reserved'",
            params![
                finalized.status.as_storage(),
                finalized.final_outcome.as_storage(),
                finalized.actual_cost,
                finalized.input_tokens,
                finalized.output_tokens,
                finalized
                    .failure_kind
                    .map(GenerationFailureKind::as_storage),
                finalized_at.to_rfc3339(),
                attempt_id.hyphenated().to_string(),
            ],
        )?;
        let attempt = find_generation_attempt(&connection, attempt_id)?
            .ok_or_else(|| SignalError::NotFound(format!("generation attempt {attempt_id}")))?;
        if changed == 0
            && (attempt.status != finalized.status
                || attempt.final_outcome != Some(finalized.final_outcome)
                || attempt.actual_cost_microusd
                    != Some(integer_as_u64(finalized.actual_cost, "actual cost")?)
                || attempt.input_tokens
                    != finalized
                        .input_tokens
                        .map(|value| integer_as_u64(value, "input tokens"))
                        .transpose()?
                || attempt.output_tokens
                    != finalized
                        .output_tokens
                        .map(|value| integer_as_u64(value, "output tokens"))
                        .transpose()?
                || attempt.failure_kind != finalized.failure_kind
                || attempt.finalized_at != Some(finalized_at))
        {
            return Err(SignalError::Storage(format!(
                "generation attempt {attempt_id} has a conflicting finalization"
            )));
        }
        Ok(attempt)
    }

    pub fn list_generation_attempts(&self) -> Result<Vec<GenerationAttempt>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(&format!(
            "SELECT {GENERATION_ATTEMPT_COLUMNS} FROM generation_attempts
             ORDER BY reserved_at ASC, id ASC"
        ))?;
        let rows = statement.query_map([], generation_attempt_row)?;
        let mut attempts = Vec::new();
        for row in rows {
            attempts.push(row?.into_generation_attempt()?);
        }
        Ok(attempts)
    }

    pub fn select_story_summary(
        &self,
        story_id: &str,
        summary_variant_id: uuid::Uuid,
    ) -> Result<()> {
        if self.select_story_summary_if_present(story_id, summary_variant_id)? {
            Ok(())
        } else {
            Err(SignalError::NotFound(format!(
                "briefing item for story {story_id}"
            )))
        }
    }

    pub fn select_story_summary_if_present(
        &self,
        story_id: &str,
        summary_variant_id: uuid::Uuid,
    ) -> Result<bool> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let variant_story_id = transaction
            .query_row(
                "SELECT story_id FROM summary_variants WHERE id = ?1",
                [summary_variant_id.hyphenated().to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| {
                SignalError::NotFound(format!("summary variant {summary_variant_id}"))
            })?;
        if variant_story_id != story_id {
            return Err(SignalError::Storage(
                "summary variant belongs to a different story".to_owned(),
            ));
        }
        let changed = transaction.execute(
            "UPDATE briefing_items SET selected_summary_variant_id = ?1
             WHERE briefing_date = (
                 SELECT bi.briefing_date FROM briefing_items bi
                 JOIN briefings b ON b.date = bi.briefing_date
                 WHERE bi.story_id = ?2
                 ORDER BY b.generated_at DESC, b.date DESC, bi.position ASC
                 LIMIT 1
             ) AND story_id = ?2",
            params![summary_variant_id.hyphenated().to_string(), story_id],
        )?;
        if changed > 0 {
            bump_data_generation(&transaction)?;
        }
        transaction.commit()?;
        Ok(changed > 0)
    }

    pub fn list_model_profiles(&self) -> Result<Vec<ModelProfile>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(&format!(
            "SELECT {MODEL_PROFILE_COLUMNS} FROM model_profiles ORDER BY lower(name), id"
        ))?;
        collect_model_profiles(statement.query_map([], model_profile_row)?)
    }

    pub fn find_model_profile(&self, id: uuid::Uuid) -> Result<Option<ModelProfile>> {
        let connection = self.connect()?;
        connection
            .query_row(
                &format!("SELECT {MODEL_PROFILE_COLUMNS} FROM model_profiles WHERE id = ?1"),
                [id.hyphenated().to_string()],
                model_profile_row,
            )
            .optional()?
            .map(StoredModelProfile::into_model_profile)
            .transpose()
    }

    pub fn find_model_profile_by_name(&self, name: &str) -> Result<Option<ModelProfile>> {
        let connection = self.connect()?;
        connection
            .query_row(
                &format!(
                    "SELECT {MODEL_PROFILE_COLUMNS} FROM model_profiles WHERE lower(name) = lower(?1)"
                ),
                [name.trim()],
                model_profile_row,
            )
            .optional()?
            .map(StoredModelProfile::into_model_profile)
            .transpose()
    }

    pub fn set_default_model_profile(&self, profile_id: Option<uuid::Uuid>) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match profile_id {
            Some(profile_id) => {
                let exists = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM model_profiles WHERE id = ?1)",
                    [profile_id.hyphenated().to_string()],
                    |row| row.get::<_, bool>(0),
                )?;
                if !exists {
                    return Err(SignalError::NotFound(format!("model profile {profile_id}")));
                }
                transaction.execute(
                    "INSERT INTO app_settings (key, value) VALUES ('default_model_profile_id', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [profile_id.hyphenated().to_string()],
                )?;
            }
            None => {
                transaction.execute(
                    "DELETE FROM app_settings WHERE key = 'default_model_profile_id'",
                    [],
                )?;
            }
        }
        bump_data_generation(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn default_model_profile(&self) -> Result<Option<ModelProfile>> {
        let connection = self.connect()?;
        let profile_id = connection
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'default_model_profile_id'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(profile_id) = profile_id else {
            return Ok(None);
        };
        let profile_id = uuid::Uuid::parse_str(&profile_id)
            .map_err(|error| SignalError::Serialization(error.to_string()))?;
        drop(connection);
        self.find_model_profile(profile_id)
    }

    pub fn remove_model_profile(&self, profile_id: uuid::Uuid) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let profile_id = profile_id.hyphenated().to_string();
        let removed =
            transaction.execute("DELETE FROM model_profiles WHERE id = ?1", [&profile_id])?;
        if removed == 0 {
            return Err(SignalError::NotFound(format!("model profile {profile_id}")));
        }
        transaction.execute(
            "DELETE FROM app_settings WHERE key = 'default_model_profile_id' AND value = ?1",
            [&profile_id],
        )?;
        bump_data_generation(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn latest_refresh_run(&self) -> Result<Option<RefreshRun>> {
        let connection = self.connect()?;
        let stored = connection
            .query_row(
                "SELECT started_at, finished_at, successful_sources, failed_sources
                 FROM refresh_runs
                 ORDER BY id DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;

        stored
            .map(
                |(started_at, finished_at, successful_sources, failed_sources)| {
                    Ok(RefreshRun {
                        started_at: parse_datetime(&started_at)?,
                        finished_at: finished_at
                            .map(|value| parse_datetime(&value))
                            .transpose()?,
                        successful_sources: count_as_u64(successful_sources)?,
                        failed_sources: count_as_u64(failed_sources)?,
                    })
                },
            )
            .transpose()
    }

    pub fn load_briefing(&self, date: NaiveDate) -> Result<Option<Briefing>> {
        let connection = self.connect()?;
        let generated_at = connection
            .query_row(
                "SELECT generated_at FROM briefings WHERE date = ?1",
                [date.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        let Some(generated_at) = generated_at else {
            return Ok(None);
        };

        let mut statement = connection.prepare(
            "SELECT bi.position, bi.section, bi.is_stale, bi.selected_summary_variant_id,
                    s.id, s.title, s.canonical_url, s.excerpt, s.category, s.published_at,
                    s.source_ids_json, s.score_json, s.smart_summary, s.is_read, s.is_saved
             FROM briefing_items bi
             JOIN stories s ON s.id = bi.story_id
             WHERE bi.briefing_date = ?1
             ORDER BY bi.position ASC",
        )?;
        let rows = statement.query_map([date.to_string()], briefing_item_row)?;
        let mut stored_items = Vec::new();
        for row in rows {
            stored_items.push(row?);
        }
        drop(statement);
        let mut items = Vec::new();
        for item in stored_items {
            let selected_summary = item
                .selected_summary_variant_id
                .as_deref()
                .map(|id| {
                    find_summary_variant(
                        &connection,
                        &format!(
                            "SELECT {SUMMARY_VARIANT_COLUMNS} FROM summary_variants WHERE id = ?1"
                        ),
                        id,
                    )?
                    .ok_or_else(|| {
                        SignalError::Serialization(format!("missing selected summary variant {id}"))
                    })
                })
                .transpose()?;
            items.push(item.into_briefing_item(selected_summary)?);
        }

        Ok(Some(Briefing {
            date,
            generated_at: parse_datetime(&generated_at)?,
            items,
        }))
    }

    pub fn load_latest_briefing(&self) -> Result<Option<Briefing>> {
        let connection = self.connect()?;
        let date = connection
            .query_row(
                "SELECT date FROM briefings
                 ORDER BY generated_at DESC, date DESC
                 LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(date) = date else {
            return Ok(None);
        };
        let date = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .map_err(|error| SignalError::Serialization(error.to_string()))?;
        drop(connection);
        self.load_briefing(date)
    }

    pub fn list_latest(&self) -> Result<Vec<Story>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, title, canonical_url, excerpt, category, published_at,
                    source_ids_json, score_json, smart_summary, is_read, is_saved
             FROM stories
             ORDER BY published_at IS NULL ASC, published_at DESC, updated_at DESC, id ASC",
        )?;
        collect_stories(statement.query_map([], story_row)?)
    }

    pub fn list_saved(&self) -> Result<Vec<Story>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, title, canonical_url, excerpt, category, published_at,
                    source_ids_json, score_json, smart_summary, is_read, is_saved
             FROM stories
             WHERE is_saved = 1
             ORDER BY published_at IS NULL ASC, published_at DESC, updated_at DESC, id ASC",
        )?;
        collect_stories(statement.query_map([], story_row)?)
    }

    pub fn find_story(&self, id: &str) -> Result<Option<Story>> {
        let connection = self.connect()?;
        let stored = connection
            .query_row(
                "SELECT id, title, canonical_url, excerpt, category, published_at,
                        source_ids_json, score_json, smart_summary, is_read, is_saved
                 FROM stories WHERE id = ?1",
                [id],
                story_row,
            )
            .optional()?;
        stored.map(StoredStory::into_story).transpose()
    }

    pub fn set_saved(&self, id: &str, saved: bool) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE stories SET is_saved = ?1 WHERE id = ?2",
            params![saved, id],
        )?;
        if changed == 0 {
            return Err(SignalError::NotFound(format!("story {id}")));
        }
        bump_data_generation(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn set_read(&self, id: &str, read: bool) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE stories SET is_read = ?1 WHERE id = ?2",
            params![read, id],
        )?;
        if changed == 0 {
            return Err(SignalError::NotFound(format!("story {id}")));
        }
        bump_data_generation(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn status(&self) -> Result<StoreStatus> {
        let connection = self.connect()?;
        let story_count = connection.query_row("SELECT COUNT(*) FROM stories", [], |row| {
            row.get::<_, i64>(0)
        })?;
        let last_refresh_at = connection
            .query_row("SELECT MAX(generated_at) FROM briefings", [], |row| {
                row.get::<_, Option<String>>(0)
            })?
            .map(|value| parse_datetime(&value))
            .transpose()?;
        let data_generation = connection.query_row(
            "SELECT value FROM metadata WHERE key = 'data_generation'",
            [],
            |row| row.get::<_, String>(0),
        )?;

        Ok(StoreStatus {
            story_count: u64::try_from(story_count)
                .map_err(|error| SignalError::Serialization(error.to_string()))?,
            last_refresh_at,
            data_generation: data_generation.parse().map_err(
                |error: std::num::ParseIntError| SignalError::Serialization(error.to_string()),
            )?,
        })
    }

    pub fn journal_mode(&self) -> Result<String> {
        let connection = self.connect()?;
        connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(Into::into)
    }

    fn connect(&self) -> Result<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Ok(connection)
    }

    fn apply_migrations(&self) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(INITIAL_MIGRATION)?;
        transaction.execute(
            "INSERT OR IGNORE INTO schema_migrations (version, applied_at)
             VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        for (version, migration) in MIGRATIONS {
            let applied = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [version],
                |row| row.get::<_, bool>(0),
            )?;
            if !applied {
                transaction.execute_batch(migration)?;
                transaction.execute(
                    "INSERT INTO schema_migrations (version, applied_at)
                     VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                    [version],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
}

fn bump_data_generation(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute(
        "UPDATE metadata SET value = CAST(value AS INTEGER) + 1 WHERE key = 'data_generation'",
        [],
    )?;
    Ok(())
}

fn insert_refresh_run(
    transaction: &Transaction<'_>,
    occurred_at: DateTime<Utc>,
    successful_sources: i64,
    failed_sources: i64,
    error_json: Option<&str>,
) -> Result<()> {
    let occurred_at = occurred_at.to_rfc3339();
    transaction.execute(
        "INSERT INTO refresh_runs (
             started_at, finished_at, successful_sources, failed_sources, error_json
         ) VALUES (?1, ?1, ?2, ?3, ?4)",
        params![occurred_at, successful_sources, failed_sources, error_json],
    )?;
    Ok(())
}

fn insert_model_profile(transaction: &Transaction<'_>, profile: &ModelProfile) -> Result<()> {
    let (credential_kind, credential_service, credential_account, credential_variable) =
        match &profile.credential {
            CredentialRef::SystemStore { service, account } => {
                ("system_store", Some(service), Some(account), None)
            }
            CredentialRef::Environment { variable } => ("environment", None, None, Some(variable)),
        };
    transaction.execute(
        "INSERT INTO model_profiles (
             id, name, provider, model, endpoint, dialect, credential_kind, credential_service,
             credential_account, credential_variable, consented_at, enabled,
             max_summaries_per_refresh, max_daily_cost_microusd,
             input_cost_microusd_per_million, output_cost_microusd_per_million,
             max_output_tokens, timeout_seconds, max_retries, created_at, updated_at
         ) VALUES (
             ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
             ?18, ?19, ?20, ?21
         )",
        params![
            profile.id.hyphenated().to_string(),
            profile.name,
            profile.provider.as_storage(),
            profile.model,
            profile.endpoint.as_ref().map(url::Url::as_str),
            profile.dialect.map(ApiDialect::as_storage),
            credential_kind,
            credential_service,
            credential_account,
            credential_variable,
            profile.consented_at.map(|value| value.to_rfc3339()),
            profile.enabled,
            i64::from(profile.limits.max_summaries_per_refresh),
            profile
                .limits
                .max_daily_cost_microusd
                .map(integer_as_i64)
                .transpose()?,
            profile
                .limits
                .input_cost_microusd_per_million
                .map(integer_as_i64)
                .transpose()?,
            profile
                .limits
                .output_cost_microusd_per_million
                .map(integer_as_i64)
                .transpose()?,
            i64::from(profile.limits.max_output_tokens),
            integer_as_i64(profile.limits.timeout_seconds)?,
            i64::from(profile.limits.max_retries),
            profile.created_at.to_rfc3339(),
            profile.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

fn integer_as_i64(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|error| SignalError::Serialization(error.to_string()))
}

fn optional_integer_as_i64(value: Option<u64>) -> Result<Option<i64>> {
    value.map(integer_as_i64).transpose()
}

fn count_as_i64(count: usize) -> Result<i64> {
    i64::try_from(count).map_err(|error| SignalError::Serialization(error.to_string()))
}

fn count_as_u64(count: i64) -> Result<u64> {
    u64::try_from(count).map_err(|error| SignalError::Serialization(error.to_string()))
}

fn collect_model_profiles(
    rows: rusqlite::MappedRows<
        '_,
        impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<StoredModelProfile>,
    >,
) -> Result<Vec<ModelProfile>> {
    let mut profiles = Vec::new();
    for row in rows {
        profiles.push(row?.into_model_profile()?);
    }
    Ok(profiles)
}

fn collect_summary_variants(
    rows: rusqlite::MappedRows<
        '_,
        impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<StoredSummaryVariant>,
    >,
) -> Result<Vec<SummaryVariant>> {
    let mut variants = Vec::new();
    for row in rows {
        variants.push(row?.into_summary_variant()?);
    }
    Ok(variants)
}

fn find_summary_variant(
    connection: &Connection,
    sql: &str,
    parameter: &str,
) -> Result<Option<SummaryVariant>> {
    connection
        .query_row(sql, [parameter], summary_variant_row)
        .optional()?
        .map(StoredSummaryVariant::into_summary_variant)
        .transpose()
}

fn summary_variant_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredSummaryVariant> {
    Ok(StoredSummaryVariant {
        id: row.get(0)?,
        story_id: row.get(1)?,
        profile_id: row.get(2)?,
        provider: row.get(3)?,
        model: row.get(4)?,
        endpoint: row.get(5)?,
        dialect: row.get(6)?,
        prompt_version: row.get(7)?,
        cache_key: row.get(8)?,
        what_happened: row.get(9)?,
        why_it_matters: row.get(10)?,
        caveat: row.get(11)?,
        input_tokens: row.get(12)?,
        output_tokens: row.get(13)?,
        cost_microusd: row.get(14)?,
        generated_at: row.get(15)?,
    })
}

fn find_generation_attempt(
    connection: &Connection,
    attempt_id: uuid::Uuid,
) -> Result<Option<GenerationAttempt>> {
    connection
        .query_row(
            &format!("SELECT {GENERATION_ATTEMPT_COLUMNS} FROM generation_attempts WHERE id = ?1"),
            [attempt_id.hyphenated().to_string()],
            generation_attempt_row,
        )
        .optional()?
        .map(StoredGenerationAttempt::into_generation_attempt)
        .transpose()
}

fn generation_attempt_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredGenerationAttempt> {
    Ok(StoredGenerationAttempt {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        provider: row.get(2)?,
        model: row.get(3)?,
        endpoint: row.get(4)?,
        dialect: row.get(5)?,
        usage_date: row.get(6)?,
        status: row.get(7)?,
        final_outcome: row.get(8)?,
        estimated_cost_microusd: row.get(9)?,
        actual_cost_microusd: row.get(10)?,
        input_tokens: row.get(11)?,
        output_tokens: row.get(12)?,
        failure_kind: row.get(13)?,
        reserved_at: row.get(14)?,
        expires_at: row.get(15)?,
        finalized_at: row.get(16)?,
    })
}

struct FinalizedValues {
    status: GenerationStatus,
    final_outcome: GenerationOutcomeKind,
    actual_cost: i64,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    failure_kind: Option<GenerationFailureKind>,
}

fn finalized_values(outcome: &AttemptOutcome) -> Result<FinalizedValues> {
    match outcome {
        AttemptOutcome::Completed {
            input_tokens,
            output_tokens,
            cost_microusd,
        } => Ok(FinalizedValues {
            status: GenerationStatus::Completed,
            final_outcome: GenerationOutcomeKind::Completed,
            actual_cost: integer_as_i64(*cost_microusd)?,
            input_tokens: optional_integer_as_i64(*input_tokens)?,
            output_tokens: optional_integer_as_i64(*output_tokens)?,
            failure_kind: None,
        }),
        AttemptOutcome::FailedCharged {
            category,
            cost_microusd,
        } => Ok(FinalizedValues {
            status: GenerationStatus::Failed,
            final_outcome: GenerationOutcomeKind::FailedCharged,
            actual_cost: integer_as_i64(*cost_microusd)?,
            input_tokens: None,
            output_tokens: None,
            failure_kind: Some(*category),
        }),
        AttemptOutcome::FailedUncharged { category } => Ok(FinalizedValues {
            status: GenerationStatus::Failed,
            final_outcome: GenerationOutcomeKind::FailedUncharged,
            actual_cost: 0,
            input_tokens: None,
            output_tokens: None,
            failure_kind: Some(*category),
        }),
    }
}

fn validate_summary_variant(variant: &SummaryVariant) -> Result<()> {
    let unrestricted = SummarySettings {
        what_happened_max_chars: u32::MAX,
        why_it_matters_max_chars: u32::MAX,
        caveat_max_chars: u32::MAX,
    };
    variant.fields.validate(&unrestricted)?;
    if variant.story_id.trim().is_empty()
        || variant.model.trim().is_empty()
        || variant.prompt_version.trim().is_empty()
        || variant.cache_key.trim().is_empty()
    {
        return Err(SignalError::InvalidConfiguration(
            "summary variant identity fields are required".to_owned(),
        ));
    }
    if let Some(endpoint) = &variant.endpoint {
        let endpoint = url::Url::parse(endpoint).map_err(|_| {
            SignalError::InvalidConfiguration("summary endpoint snapshot is invalid".to_owned())
        })?;
        if !endpoint.username().is_empty() || endpoint.password().is_some() {
            return Err(SignalError::InvalidConfiguration(
                "summary endpoint snapshot must not contain user info".to_owned(),
            ));
        }
    }
    integer_as_i64(variant.cost_microusd)?;
    optional_integer_as_i64(variant.input_tokens)?;
    optional_integer_as_i64(variant.output_tokens)?;
    Ok(())
}

fn model_profile_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredModelProfile> {
    Ok(StoredModelProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        provider: row.get(2)?,
        model: row.get(3)?,
        endpoint: row.get(4)?,
        dialect: row.get(5)?,
        credential_kind: row.get(6)?,
        credential_service: row.get(7)?,
        credential_account: row.get(8)?,
        credential_variable: row.get(9)?,
        consented_at: row.get(10)?,
        enabled: row.get(11)?,
        max_summaries_per_refresh: row.get(12)?,
        max_daily_cost_microusd: row.get(13)?,
        input_cost_microusd_per_million: row.get(14)?,
        output_cost_microusd_per_million: row.get(15)?,
        max_output_tokens: row.get(16)?,
        timeout_seconds: row.get(17)?,
        max_retries: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
    })
}

fn upsert_stories_in_transaction(transaction: &Transaction<'_>, stories: &[Story]) -> Result<()> {
    let updated_at = Utc::now().to_rfc3339();
    let mut statement = transaction.prepare(
        "INSERT INTO stories (
             id, title, canonical_url, excerpt, category, published_at, source_ids_json,
             score_json, smart_summary, is_read, is_saved, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(id) DO UPDATE SET
             title = excluded.title,
             canonical_url = excluded.canonical_url,
             excerpt = excluded.excerpt,
             category = excluded.category,
             published_at = excluded.published_at,
             source_ids_json = excluded.source_ids_json,
             score_json = excluded.score_json,
             smart_summary = excluded.smart_summary,
             updated_at = excluded.updated_at",
    )?;

    for story in stories {
        let source_ids = serialize(&story.source_ids)?;
        let score = serialize(&story.score)?;
        statement.execute(params![
            story.id,
            story.title,
            story.canonical_url,
            story.excerpt,
            story.category,
            story.published_at.map(|value| value.to_rfc3339()),
            source_ids,
            score,
            story.smart_summary,
            story.is_read,
            story.is_saved,
            updated_at,
        ])?;
    }
    Ok(())
}

fn serialize<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|error| SignalError::Serialization(error.to_string()))
}

fn deserialize<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|error| SignalError::Serialization(error.to_string()))
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| SignalError::Serialization(error.to_string()))
}

fn collect_stories(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<StoredStory>>,
) -> Result<Vec<Story>> {
    let mut stories = Vec::new();
    for row in rows {
        stories.push(row?.into_story()?);
    }
    Ok(stories)
}

fn story_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredStory> {
    Ok(StoredStory {
        id: row.get(0)?,
        title: row.get(1)?,
        canonical_url: row.get(2)?,
        excerpt: row.get(3)?,
        category: row.get(4)?,
        published_at: row.get(5)?,
        source_ids_json: row.get(6)?,
        score_json: row.get(7)?,
        smart_summary: row.get(8)?,
        is_read: row.get(9)?,
        is_saved: row.get(10)?,
    })
}

fn briefing_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredBriefingItem> {
    Ok(StoredBriefingItem {
        position: row.get(0)?,
        section: row.get(1)?,
        is_stale: row.get(2)?,
        selected_summary_variant_id: row.get(3)?,
        story: StoredStory {
            id: row.get(4)?,
            title: row.get(5)?,
            canonical_url: row.get(6)?,
            excerpt: row.get(7)?,
            category: row.get(8)?,
            published_at: row.get(9)?,
            source_ids_json: row.get(10)?,
            score_json: row.get(11)?,
            smart_summary: row.get(12)?,
            is_read: row.get(13)?,
            is_saved: row.get(14)?,
        },
    })
}

struct StoredStory {
    id: String,
    title: String,
    canonical_url: String,
    excerpt: String,
    category: String,
    published_at: Option<String>,
    source_ids_json: String,
    score_json: String,
    smart_summary: String,
    is_read: bool,
    is_saved: bool,
}

struct StoredModelProfile {
    id: String,
    name: String,
    provider: String,
    model: String,
    endpoint: Option<String>,
    dialect: Option<String>,
    credential_kind: String,
    credential_service: Option<String>,
    credential_account: Option<String>,
    credential_variable: Option<String>,
    consented_at: Option<String>,
    enabled: bool,
    max_summaries_per_refresh: i64,
    max_daily_cost_microusd: Option<i64>,
    input_cost_microusd_per_million: Option<i64>,
    output_cost_microusd_per_million: Option<i64>,
    max_output_tokens: i64,
    timeout_seconds: i64,
    max_retries: i64,
    created_at: String,
    updated_at: String,
}

struct StoredSummaryVariant {
    id: String,
    story_id: String,
    profile_id: Option<String>,
    provider: String,
    model: String,
    endpoint: Option<String>,
    dialect: Option<String>,
    prompt_version: String,
    cache_key: String,
    what_happened: String,
    why_it_matters: String,
    caveat: Option<String>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cost_microusd: i64,
    generated_at: String,
}

impl StoredSummaryVariant {
    fn into_summary_variant(self) -> Result<SummaryVariant> {
        Ok(SummaryVariant {
            id: uuid::Uuid::parse_str(&self.id)
                .map_err(|error| SignalError::Serialization(error.to_string()))?,
            story_id: self.story_id,
            profile_id: self
                .profile_id
                .map(|value| uuid::Uuid::parse_str(&value))
                .transpose()
                .map_err(|error| SignalError::Serialization(error.to_string()))?,
            provider: ProviderKind::from_storage(&self.provider)?,
            model: self.model,
            endpoint: self.endpoint,
            dialect: self
                .dialect
                .as_deref()
                .map(ApiDialect::from_storage)
                .transpose()?,
            prompt_version: self.prompt_version,
            cache_key: self.cache_key,
            fields: AiSummaryFields {
                what_happened: self.what_happened,
                why_it_matters: self.why_it_matters,
                caveat: self.caveat,
            },
            input_tokens: self
                .input_tokens
                .map(|value| integer_as_u64(value, "input tokens"))
                .transpose()?,
            output_tokens: self
                .output_tokens
                .map(|value| integer_as_u64(value, "output tokens"))
                .transpose()?,
            cost_microusd: integer_as_u64(self.cost_microusd, "summary cost")?,
            generated_at: parse_datetime(&self.generated_at)?,
        })
    }
}

struct StoredGenerationAttempt {
    id: String,
    profile_id: Option<String>,
    provider: String,
    model: String,
    endpoint: Option<String>,
    dialect: Option<String>,
    usage_date: String,
    status: String,
    final_outcome: Option<String>,
    estimated_cost_microusd: i64,
    actual_cost_microusd: Option<i64>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    failure_kind: Option<String>,
    reserved_at: String,
    expires_at: String,
    finalized_at: Option<String>,
}

impl StoredGenerationAttempt {
    fn into_generation_attempt(self) -> Result<GenerationAttempt> {
        Ok(GenerationAttempt {
            id: uuid::Uuid::parse_str(&self.id)
                .map_err(|error| SignalError::Serialization(error.to_string()))?,
            profile_id: self
                .profile_id
                .map(|value| uuid::Uuid::parse_str(&value))
                .transpose()
                .map_err(|error| SignalError::Serialization(error.to_string()))?,
            provider: ProviderKind::from_storage(&self.provider)?,
            model: self.model,
            endpoint: self.endpoint,
            dialect: self
                .dialect
                .as_deref()
                .map(ApiDialect::from_storage)
                .transpose()?,
            usage_date: NaiveDate::parse_from_str(&self.usage_date, "%Y-%m-%d")
                .map_err(|error| SignalError::Serialization(error.to_string()))?,
            status: GenerationStatus::from_storage(&self.status)?,
            final_outcome: self
                .final_outcome
                .as_deref()
                .map(GenerationOutcomeKind::from_storage)
                .transpose()?,
            estimated_cost_microusd: integer_as_u64(
                self.estimated_cost_microusd,
                "estimated cost",
            )?,
            actual_cost_microusd: self
                .actual_cost_microusd
                .map(|value| integer_as_u64(value, "actual cost"))
                .transpose()?,
            input_tokens: self
                .input_tokens
                .map(|value| integer_as_u64(value, "input tokens"))
                .transpose()?,
            output_tokens: self
                .output_tokens
                .map(|value| integer_as_u64(value, "output tokens"))
                .transpose()?,
            failure_kind: self
                .failure_kind
                .as_deref()
                .map(GenerationFailureKind::from_storage)
                .transpose()?,
            reserved_at: parse_datetime(&self.reserved_at)?,
            expires_at: parse_datetime(&self.expires_at)?,
            finalized_at: self
                .finalized_at
                .map(|value| parse_datetime(&value))
                .transpose()?,
        })
    }
}

impl StoredModelProfile {
    fn into_model_profile(self) -> Result<ModelProfile> {
        let credential = match self.credential_kind.as_str() {
            "system_store" => CredentialRef::SystemStore {
                service: required_database_value(self.credential_service, "credential service")?,
                account: required_database_value(self.credential_account, "credential account")?,
            },
            "environment" => CredentialRef::Environment {
                variable: required_database_value(self.credential_variable, "credential variable")?,
            },
            value => {
                return Err(SignalError::Serialization(format!(
                    "invalid credential kind {value:?}"
                )));
            }
        };
        let profile = ModelProfile {
            id: uuid::Uuid::parse_str(&self.id)
                .map_err(|error| SignalError::Serialization(error.to_string()))?,
            name: self.name,
            provider: ProviderKind::from_storage(&self.provider)?,
            model: self.model,
            endpoint: self
                .endpoint
                .map(|value| value.parse())
                .transpose()
                .map_err(|error: url::ParseError| SignalError::Serialization(error.to_string()))?,
            dialect: self
                .dialect
                .as_deref()
                .map(ApiDialect::from_storage)
                .transpose()?,
            credential,
            consented_at: self
                .consented_at
                .map(|value| parse_datetime(&value))
                .transpose()?,
            enabled: self.enabled,
            limits: ProfileLimits {
                max_summaries_per_refresh: integer_as_u32(
                    self.max_summaries_per_refresh,
                    "max summaries per refresh",
                )?,
                max_daily_cost_microusd: self
                    .max_daily_cost_microusd
                    .map(|value| integer_as_u64(value, "daily cost"))
                    .transpose()?,
                input_cost_microusd_per_million: self
                    .input_cost_microusd_per_million
                    .map(|value| integer_as_u64(value, "input cost"))
                    .transpose()?,
                output_cost_microusd_per_million: self
                    .output_cost_microusd_per_million
                    .map(|value| integer_as_u64(value, "output cost"))
                    .transpose()?,
                max_output_tokens: integer_as_u32(self.max_output_tokens, "max output tokens")?,
                timeout_seconds: integer_as_u64(self.timeout_seconds, "timeout seconds")?,
                max_retries: integer_as_u32(self.max_retries, "max retries")?,
            },
            created_at: parse_datetime(&self.created_at)?,
            updated_at: parse_datetime(&self.updated_at)?,
        };
        profile.validate()?;
        Ok(profile)
    }
}

fn required_database_value(value: Option<String>, field: &str) -> Result<String> {
    value.ok_or_else(|| SignalError::Serialization(format!("missing {field}")))
}

fn integer_as_u64(value: i64, field: &str) -> Result<u64> {
    u64::try_from(value)
        .map_err(|error| SignalError::Serialization(format!("invalid {field}: {error}")))
}

fn integer_as_u32(value: i64, field: &str) -> Result<u32> {
    u32::try_from(value)
        .map_err(|error| SignalError::Serialization(format!("invalid {field}: {error}")))
}

impl StoredStory {
    fn into_story(self) -> Result<Story> {
        Ok(Story {
            id: self.id,
            title: self.title,
            canonical_url: self.canonical_url,
            excerpt: self.excerpt,
            category: self.category,
            published_at: self
                .published_at
                .map(|value| parse_datetime(&value))
                .transpose()?,
            source_ids: deserialize(&self.source_ids_json)?,
            score: deserialize::<ScoreBreakdown>(&self.score_json)?,
            smart_summary: self.smart_summary,
            is_read: self.is_read,
            is_saved: self.is_saved,
        })
    }
}

struct StoredBriefingItem {
    position: u32,
    section: String,
    is_stale: bool,
    selected_summary_variant_id: Option<String>,
    story: StoredStory,
}

impl StoredBriefingItem {
    fn into_briefing_item(self, selected_summary: Option<SummaryVariant>) -> Result<BriefingItem> {
        Ok(BriefingItem {
            position: self.position,
            section: self.section,
            is_stale: self.is_stale,
            story: self.story.into_story()?,
            selected_summary,
        })
    }
}
