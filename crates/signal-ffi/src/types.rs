use signal_core::{
    AiSummaryFields, ApiDialect, BriefingItem, CredentialRef, ModelProfile, ProfileLimits,
    ProviderKind, ScoreBreakdown, SourceKind, SourceOrigin, SourceRecord, StateRevision,
    StoreStatus, Story, SummaryVariant, TodayView,
};

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
) -> FfiStory {
    FfiStory {
        id: story.id,
        title: story.title,
        canonical_url: story.canonical_url,
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
    }
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
) -> FfiBriefing {
    let TodayView { briefing, is_stale } = today;
    let items = briefing
        .items
        .into_iter()
        .zip(summary_variants)
        .map(|(item, summary_variants)| briefing_item(item, summary_variants))
        .collect();
    FfiBriefing {
        date: briefing.date.to_string(),
        generated_at: briefing.generated_at.to_rfc3339(),
        is_stale,
        items,
    }
}

fn briefing_item(item: BriefingItem, summary_variants: Vec<FfiSummaryVariant>) -> FfiBriefingItem {
    let selected_summary = item.selected_summary.map(FfiSummaryVariant::from);
    FfiBriefingItem {
        position: item.position,
        section: item.section,
        is_stale: item.is_stale,
        story: story(
            item.story,
            selected_summary.clone(),
            summary_variants.clone(),
        ),
        selected_summary,
        summary_variants,
    }
}

impl From<SourceRecord> for FfiSource {
    fn from(record: SourceRecord) -> Self {
        let feed_url = match record.source.kind {
            SourceKind::Feed { url } => url,
        };
        Self {
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
        }
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

impl From<ModelProfile> for FfiModelProfile {
    fn from(profile: ModelProfile) -> Self {
        Self {
            id: profile.id.hyphenated().to_string(),
            name: profile.name,
            provider: profile.provider.into(),
            model: profile.model,
            endpoint: profile.endpoint.map(|endpoint| endpoint.to_string()),
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
        }
    }
}
