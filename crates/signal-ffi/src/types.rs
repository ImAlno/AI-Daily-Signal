use signal_core::{
    AiSummaryFields, ApiDialect, BriefingItem, CredentialRef, GenerationReport, ModelProfile,
    MoneyMicros, ProfileLimits, ProviderKind, ScoreBreakdown, SourceKind, SourceOrigin,
    SourceRecord, StateRevision, StoreStatus, Story, SummaryVariant, TodayView, display_safe_url,
};

use crate::CompanionError;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiStateRevision {
    pub data_generation: u64,
    pub source_config_revision: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FfiCollectionState {
    NotInitialized,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiRefreshMetadata {
    pub last_refresh_at: String,
    pub story_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiCollectionStatus {
    pub state: FfiCollectionState,
    pub refresh: Option<FfiRefreshMetadata>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiScore {
    pub recency: f64,
    pub source_weight: f64,
    pub corroboration: f64,
    pub total: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiStory {
    pub id: String,
    pub title: String,
    pub canonical_url: String,
    pub excerpt: String,
    pub category: String,
    pub published_at: Option<String>,
    pub source_ids: Vec<String>,
    pub score: FfiScore,
    pub smart_summary: String,
    pub is_read: bool,
    pub is_saved: bool,
    pub selected_summary: Option<FfiSummaryVariant>,
    pub summary_variants: Vec<FfiSummaryVariant>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiSummaryFields {
    pub what_happened: String,
    pub why_it_matters: String,
    pub caveat: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FfiProviderKind {
    OpenAi,
    Anthropic,
    Gemini,
    OpenAiCompatible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FfiApiDialect {
    Responses,
    ChatCompletions,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiSummaryVariant {
    pub id: String,
    pub story_id: String,
    pub profile_id: Option<String>,
    pub provider: FfiProviderKind,
    pub model: String,
    pub dialect: Option<FfiApiDialect>,
    pub fields: FfiSummaryFields,
    pub generated_at: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiBriefingItem {
    pub position: u32,
    pub section: String,
    pub is_stale: bool,
    pub story: FfiStory,
    pub selected_summary: Option<FfiSummaryVariant>,
    pub summary_variants: Vec<FfiSummaryVariant>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiBriefing {
    pub date: String,
    pub generated_at: String,
    pub is_stale: bool,
    pub items: Vec<FfiBriefingItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FfiSourceOrigin {
    Standard,
    Personal,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiSource {
    pub id: String,
    pub name: String,
    pub category: String,
    pub enabled: bool,
    pub weight: f64,
    pub feed_url: String,
    pub origin: FfiSourceOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FfiCredentialSourceKind {
    SystemStore,
    Environment,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiProfileLimits {
    pub max_summaries_per_refresh: u32,
    pub max_daily_cost_microusd: Option<u64>,
    pub input_cost_microusd_per_million: Option<u64>,
    pub output_cost_microusd_per_million: Option<u64>,
    pub max_output_tokens: u32,
    pub timeout_seconds: u64,
    pub max_retries: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiModelProfile {
    pub id: String,
    pub name: String,
    pub provider: FfiProviderKind,
    pub model: String,
    pub endpoint: Option<String>,
    pub dialect: Option<FfiApiDialect>,
    pub credential_source: FfiCredentialSourceKind,
    pub consented_at: Option<String>,
    pub enabled: bool,
    pub limits: FfiProfileLimits,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct AddFeedSourceRequest {
    pub name: String,
    pub category: String,
    pub url: String,
    pub weight: f64,
    pub enabled: bool,
}

#[derive(Clone, uniffi::Enum)]
pub enum AddCredentialRequest {
    SystemStore { secret: String },
    Environment { variable: String },
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct FfiProfileLimitsInput {
    pub max_summaries_per_refresh: u32,
    pub max_daily_cost_usd: Option<String>,
    pub input_cost_usd_per_million: Option<String>,
    pub output_cost_usd_per_million: Option<String>,
    pub max_output_tokens: u32,
    pub timeout_seconds: u64,
    pub max_retries: u32,
}

#[derive(Clone, uniffi::Record)]
pub struct AddModelProfileRequest {
    pub name: String,
    pub provider: FfiProviderKind,
    pub model: String,
    pub endpoint: Option<String>,
    pub dialect: Option<FfiApiDialect>,
    pub credential: AddCredentialRequest,
    pub consent_provider_data_sharing: bool,
    pub limits: FfiProfileLimitsInput,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiStoryMutation {
    pub story: FfiStory,
    pub revision: FfiStateRevision,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiSourceMutation {
    pub source: FfiSource,
    pub revision: FfiStateRevision,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiModelMutation {
    pub profile: FfiModelProfile,
    pub revision: FfiStateRevision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FfiCredentialDeletionStatus {
    Deleted,
    NotApplicable,
    DeleteFailed,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiModelRemoval {
    pub profile: FfiModelProfile,
    pub credential_deletion: FfiCredentialDeletionStatus,
    pub revision: FfiStateRevision,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiModelTestMutation {
    pub profile: FfiModelProfile,
    pub cost_may_apply: bool,
    pub revision: FfiStateRevision,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiGenerationReport {
    pub eligible: u64,
    pub generated: u64,
    pub cache_hits: u64,
    pub skipped_cap: u64,
    pub skipped_budget: u64,
    pub missing_credentials: u64,
    pub provider_failures: u64,
    pub malformed_outputs: u64,
    pub smart_fallbacks: u64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiRefreshResult {
    pub briefing: FfiBriefing,
    pub successful_sources: u64,
    pub failed_sources: u64,
    pub generation: FfiGenerationReport,
    pub revision: FfiStateRevision,
}

impl TryFrom<GenerationReport> for FfiGenerationReport {
    type Error = CompanionError;

    fn try_from(report: GenerationReport) -> Result<Self, Self::Error> {
        Ok(Self {
            eligible: ffi_count(report.eligible)?,
            generated: ffi_count(report.generated)?,
            cache_hits: ffi_count(report.cache_hits)?,
            skipped_cap: ffi_count(report.skipped_cap)?,
            skipped_budget: ffi_count(report.skipped_budget)?,
            missing_credentials: ffi_count(report.missing_credentials)?,
            provider_failures: ffi_count(report.provider_failures)?,
            malformed_outputs: ffi_count(report.malformed_outputs)?,
            smart_fallbacks: ffi_count(report.smart_fallbacks)?,
        })
    }
}

fn ffi_count(value: usize) -> Result<u64, CompanionError> {
    u64::try_from(value).map_err(|_| CompanionError::StorageUnavailable)
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CompanionSnapshot {
    pub revision: FfiStateRevision,
    pub status: FfiCollectionStatus,
    pub today: Option<FfiBriefing>,
    pub latest: Vec<FfiStory>,
    pub saved: Vec<FfiStory>,
    pub sources: Vec<FfiSource>,
    pub model_profiles: Vec<FfiModelProfile>,
    pub default_model_profile_id: Option<String>,
    pub has_usable_ai_profile: bool,
}

impl From<StateRevision> for FfiStateRevision {
    fn from(revision: StateRevision) -> Self {
        Self {
            data_generation: revision.data_generation,
            source_config_revision: revision.source_config_revision,
        }
    }
}

impl From<StoreStatus> for FfiCollectionStatus {
    fn from(status: StoreStatus) -> Self {
        let refresh = status
            .last_refresh_at
            .map(|last_refresh_at| FfiRefreshMetadata {
                last_refresh_at: last_refresh_at.to_rfc3339(),
                story_count: status.story_count,
            });
        Self {
            state: if refresh.is_some() || status.story_count > 0 {
                FfiCollectionState::Ready
            } else {
                FfiCollectionState::NotInitialized
            },
            refresh,
        }
    }
}

impl From<ScoreBreakdown> for FfiScore {
    fn from(score: ScoreBreakdown) -> Self {
        Self {
            recency: score.recency,
            source_weight: score.source_weight,
            corroboration: score.corroboration,
            total: score.total,
        }
    }
}

pub(crate) fn story(
    story: Story,
    selected_summary: Option<FfiSummaryVariant>,
    summary_variants: Vec<FfiSummaryVariant>,
) -> Result<FfiStory, CompanionError> {
    let canonical_url = display_safe_url(&story.canonical_url).map_err(CompanionError::from)?;
    Ok(FfiStory {
        id: story.id,
        title: story.title,
        canonical_url,
        excerpt: story.excerpt,
        category: story.category,
        published_at: story.published_at.map(|value| value.to_rfc3339()),
        source_ids: story.source_ids,
        score: story.score.into(),
        smart_summary: story.smart_summary,
        is_read: story.is_read,
        is_saved: story.is_saved,
        selected_summary,
        summary_variants,
    })
}

impl From<AiSummaryFields> for FfiSummaryFields {
    fn from(fields: AiSummaryFields) -> Self {
        Self {
            what_happened: fields.what_happened,
            why_it_matters: fields.why_it_matters,
            caveat: fields.caveat,
        }
    }
}

impl From<ProviderKind> for FfiProviderKind {
    fn from(provider: ProviderKind) -> Self {
        match provider {
            ProviderKind::OpenAi => Self::OpenAi,
            ProviderKind::Anthropic => Self::Anthropic,
            ProviderKind::Gemini => Self::Gemini,
            ProviderKind::OpenAiCompatible => Self::OpenAiCompatible,
        }
    }
}

impl From<ApiDialect> for FfiApiDialect {
    fn from(dialect: ApiDialect) -> Self {
        match dialect {
            ApiDialect::Responses => Self::Responses,
            ApiDialect::ChatCompletions => Self::ChatCompletions,
        }
    }
}

impl From<SummaryVariant> for FfiSummaryVariant {
    fn from(summary: SummaryVariant) -> Self {
        Self {
            id: summary.id.hyphenated().to_string(),
            story_id: summary.story_id,
            profile_id: summary.profile_id.map(|id| id.hyphenated().to_string()),
            provider: summary.provider.into(),
            model: summary.model,
            dialect: summary.dialect.map(Into::into),
            fields: summary.fields.into(),
            generated_at: summary.generated_at.to_rfc3339(),
        }
    }
}

pub(crate) fn briefing(
    today: TodayView,
    summary_variants: Vec<Vec<FfiSummaryVariant>>,
) -> Result<FfiBriefing, CompanionError> {
    let TodayView { briefing, is_stale } = today;
    let items = briefing
        .items
        .into_iter()
        .zip(summary_variants)
        .map(|(item, summary_variants)| briefing_item(item, summary_variants))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FfiBriefing {
        date: briefing.date.to_string(),
        generated_at: briefing.generated_at.to_rfc3339(),
        is_stale,
        items,
    })
}

fn briefing_item(
    item: BriefingItem,
    summary_variants: Vec<FfiSummaryVariant>,
) -> Result<FfiBriefingItem, CompanionError> {
    let selected_summary = item.selected_summary.map(FfiSummaryVariant::from);
    Ok(FfiBriefingItem {
        position: item.position,
        section: item.section,
        is_stale: item.is_stale,
        story: story(
            item.story,
            selected_summary.clone(),
            summary_variants.clone(),
        )?,
        selected_summary,
        summary_variants,
    })
}

impl TryFrom<SourceRecord> for FfiSource {
    type Error = CompanionError;

    fn try_from(record: SourceRecord) -> Result<Self, Self::Error> {
        let feed_url = match record.source.kind {
            SourceKind::Feed { url } => display_safe_url(&url).map_err(CompanionError::from)?,
        };
        Ok(Self {
            id: record.source.id,
            name: record.source.name,
            category: record.source.category,
            enabled: record.source.enabled,
            weight: record.source.weight,
            feed_url,
            origin: match record.origin {
                SourceOrigin::Standard => FfiSourceOrigin::Standard,
                SourceOrigin::Personal => FfiSourceOrigin::Personal,
            },
        })
    }
}

impl From<ProfileLimits> for FfiProfileLimits {
    fn from(limits: ProfileLimits) -> Self {
        Self {
            max_summaries_per_refresh: limits.max_summaries_per_refresh,
            max_daily_cost_microusd: limits.max_daily_cost_microusd,
            input_cost_microusd_per_million: limits.input_cost_microusd_per_million,
            output_cost_microusd_per_million: limits.output_cost_microusd_per_million,
            max_output_tokens: limits.max_output_tokens,
            timeout_seconds: limits.timeout_seconds,
            max_retries: limits.max_retries,
        }
    }
}

impl TryFrom<FfiProfileLimitsInput> for ProfileLimits {
    type Error = CompanionError;

    fn try_from(limits: FfiProfileLimitsInput) -> Result<Self, Self::Error> {
        Ok(Self {
            max_summaries_per_refresh: limits.max_summaries_per_refresh,
            max_daily_cost_microusd: parse_optional_usd(limits.max_daily_cost_usd)?,
            input_cost_microusd_per_million: parse_optional_usd(limits.input_cost_usd_per_million)?,
            output_cost_microusd_per_million: parse_optional_usd(
                limits.output_cost_usd_per_million,
            )?,
            max_output_tokens: limits.max_output_tokens,
            timeout_seconds: limits.timeout_seconds,
            max_retries: limits.max_retries,
        })
    }
}

fn parse_optional_usd(value: Option<String>) -> Result<Option<u64>, CompanionError> {
    value
        .map(|value| MoneyMicros::parse_usd(&value).map(MoneyMicros::as_micros))
        .transpose()
        .map_err(CompanionError::from)
}

impl From<FfiProviderKind> for ProviderKind {
    fn from(provider: FfiProviderKind) -> Self {
        match provider {
            FfiProviderKind::OpenAi => Self::OpenAi,
            FfiProviderKind::Anthropic => Self::Anthropic,
            FfiProviderKind::Gemini => Self::Gemini,
            FfiProviderKind::OpenAiCompatible => Self::OpenAiCompatible,
        }
    }
}

impl From<FfiApiDialect> for ApiDialect {
    fn from(dialect: FfiApiDialect) -> Self {
        match dialect {
            FfiApiDialect::Responses => Self::Responses,
            FfiApiDialect::ChatCompletions => Self::ChatCompletions,
        }
    }
}

impl TryFrom<ModelProfile> for FfiModelProfile {
    type Error = CompanionError;

    fn try_from(profile: ModelProfile) -> Result<Self, Self::Error> {
        let endpoint = profile
            .endpoint
            .as_ref()
            .map(|endpoint| display_safe_url(endpoint.as_str()))
            .transpose()
            .map_err(CompanionError::from)?;
        Ok(Self {
            id: profile.id.hyphenated().to_string(),
            name: profile.name,
            provider: profile.provider.into(),
            model: profile.model,
            endpoint,
            dialect: profile.dialect.map(Into::into),
            credential_source: match profile.credential {
                CredentialRef::SystemStore { .. } => FfiCredentialSourceKind::SystemStore,
                CredentialRef::Environment { .. } => FfiCredentialSourceKind::Environment,
            },
            consented_at: profile.consented_at.map(|value| value.to_rfc3339()),
            enabled: profile.enabled,
            limits: profile.limits.into(),
            created_at: profile.created_at.to_rfc3339(),
            updated_at: profile.updated_at.to_rfc3339(),
        })
    }
}
