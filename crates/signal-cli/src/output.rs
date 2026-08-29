use signal_core::{
    AiSummaryFields, ApiDialect, Briefing, CredentialRef, GenerationAttempt, GenerationReport,
    ManualGenerationStatus, ModelProfile, ProfileLimits, ProviderKind, RefreshReport,
    RemoveModelReport, Source, StoreStatus, Story, SummarizeReport, SummaryVariant,
    TestModelReport, TodayView,
};

#[derive(serde::Serialize)]
pub struct ModelProfileData {
    pub id: String,
    pub name: String,
    pub provider: &'static str,
    pub model: String,
    pub endpoint_host: Option<String>,
    pub dialect: Option<&'static str>,
    pub credential_source: &'static str,
    pub consented: bool,
    pub enabled: bool,
    pub is_default: bool,
    pub limits: ProfileLimits,
}

#[derive(serde::Serialize)]
pub struct SummaryVariantData {
    pub provider: &'static str,
    pub model: String,
    pub endpoint_host: Option<String>,
    pub dialect: Option<&'static str>,
    pub fields: AiSummaryFields,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_microusd: u64,
    pub generated_at: String,
}

#[derive(serde::Serialize)]
pub struct GenerationAttemptData {
    pub provider: &'static str,
    pub model: String,
    pub endpoint_host: Option<String>,
    pub dialect: Option<&'static str>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_microusd: Option<u64>,
    pub failure_kind: Option<signal_core::GenerationFailureKind>,
}

#[derive(serde::Serialize)]
pub struct SummarizeData {
    pub story_id: String,
    pub status: &'static str,
    pub summary: Option<SummaryVariantData>,
    pub attempt: Option<GenerationAttemptData>,
    pub generation: GenerationReport,
}

#[derive(serde::Serialize)]
pub struct TestModelData {
    pub status: &'static str,
    pub cost_may_apply: bool,
    pub attempt: Option<GenerationAttemptData>,
    pub generation: GenerationReport,
}

#[derive(serde::Serialize)]
pub struct StoryData<'a> {
    #[serde(flatten)]
    pub story: StoryFieldsData<'a>,
    pub selected_summary: Option<SummaryVariantData>,
}

#[derive(serde::Serialize)]
pub struct StoryFieldsData<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub canonical_url: &'a str,
    pub excerpt: &'a str,
    pub category: &'a str,
    pub published_at: Option<String>,
    pub source_ids: &'a [String],
    pub score: ScoreData,
    pub smart_summary: &'a str,
    pub is_read: bool,
    pub is_saved: bool,
}

#[derive(serde::Serialize)]
pub struct ScoreData {
    pub recency: f64,
    pub source_weight: f64,
    pub corroboration: f64,
    pub total: f64,
}

#[derive(serde::Serialize)]
pub struct BriefingItemData<'a> {
    pub position: u32,
    pub section: &'a str,
    pub is_stale: bool,
    pub story: StoryFieldsData<'a>,
    pub selected_summary: Option<SummaryVariantData>,
}

#[derive(serde::Serialize)]
pub struct BriefingData<'a> {
    pub date: String,
    pub generated_at: String,
    pub items: Vec<BriefingItemData<'a>>,
}

#[derive(serde::Serialize)]
pub struct TodayData<'a> {
    #[serde(flatten)]
    pub briefing: BriefingData<'a>,
    pub is_stale: bool,
}

#[derive(serde::Serialize)]
pub struct RefreshData<'a> {
    pub briefing: BriefingData<'a>,
    pub successful_sources: usize,
    pub failures: Vec<SourceFailureData<'a>>,
    pub generation: &'a GenerationReport,
}

#[derive(serde::Serialize)]
pub struct SourceFailureData<'a> {
    pub source_id: &'a str,
    pub message: &'a str,
}

#[derive(serde::Serialize)]
pub struct JsonEnvelope<T> {
    pub schema_version: u32,
    pub data: T,
}

impl<T> JsonEnvelope<T> {
    pub fn new(data: T) -> Self {
        Self {
            schema_version: 1,
            data,
        }
    }
}

pub fn json<T: serde::Serialize>(data: T) -> signal_core::Result<String> {
    serde_json::to_string_pretty(&JsonEnvelope::new(data))
        .map_err(|error| signal_core::SignalError::Serialization(error.to_string()))
}

pub fn status(status: &StoreStatus) -> String {
    format!(
        "Stories: {}\nLast refresh: {}\nData generation: {}",
        status.story_count,
        status
            .last_refresh_at
            .map_or_else(|| "never".to_owned(), |value| value.to_rfc3339()),
        status.data_generation
    )
}

pub fn briefing_data(briefing: &Briefing) -> BriefingData<'_> {
    BriefingData {
        date: briefing.date.to_string(),
        generated_at: briefing.generated_at.to_rfc3339(),
        items: briefing
            .items
            .iter()
            .map(|item| BriefingItemData {
                position: item.position,
                section: &item.section,
                is_stale: item.is_stale,
                story: story_fields_data(&item.story),
                selected_summary: item.selected_summary.as_ref().map(summary_variant_data),
            })
            .collect(),
    }
}

pub fn briefing(briefing: &BriefingData<'_>) -> String {
    let mut rendered = format!(
        "Briefing for {}\nGenerated: {}\n",
        briefing.date, briefing.generated_at
    );
    for item in &briefing.items {
        let saved = if item.story.is_saved { " [saved]" } else { "" };
        let stale = if item.is_stale { " [stale]" } else { "" };
        rendered.push_str(&format!(
            "\n{}. {}{}{}\n{}\n{}\n",
            item.position,
            item.story.title,
            saved,
            stale,
            item.story.smart_summary,
            item.story.canonical_url
        ));
        if let Some(summary) = &item.selected_summary {
            rendered.push_str(&summary_variant(summary));
        }
    }
    rendered
}

pub fn today_data(view: &TodayView) -> TodayData<'_> {
    TodayData {
        briefing: briefing_data(&view.briefing),
        is_stale: view.is_stale,
    }
}

pub fn today(view: &TodayData<'_>) -> String {
    let status = if view.is_stale { "stale" } else { "fresh" };
    format!("Status: {status}\n{}", briefing(&view.briefing))
}

pub fn refresh_data(report: &RefreshReport) -> RefreshData<'_> {
    RefreshData {
        briefing: briefing_data(&report.briefing),
        successful_sources: report.successful_sources,
        failures: report
            .failures
            .iter()
            .map(|failure| SourceFailureData {
                source_id: &failure.source_id,
                message: &failure.message,
            })
            .collect(),
        generation: &report.generation,
    }
}

pub fn refresh(report: &RefreshData<'_>) -> String {
    format!(
        "Refreshed from {} source(s); {} failed\n{}\n{}",
        report.successful_sources,
        report.failures.len(),
        generation(report.generation),
        briefing(&report.briefing)
    )
}

pub fn story_data<'a>(
    story: &'a Story,
    selected_summary: Option<&SummaryVariant>,
) -> StoryData<'a> {
    StoryData {
        story: story_fields_data(story),
        selected_summary: selected_summary.map(summary_variant_data),
    }
}

fn story_fields_data(story: &Story) -> StoryFieldsData<'_> {
    StoryFieldsData {
        id: &story.id,
        title: &story.title,
        canonical_url: &story.canonical_url,
        excerpt: &story.excerpt,
        category: &story.category,
        published_at: story.published_at.map(|value| value.to_rfc3339()),
        source_ids: &story.source_ids,
        score: ScoreData {
            recency: story.score.recency,
            source_weight: story.score.source_weight,
            corroboration: story.score.corroboration,
            total: story.score.total,
        },
        smart_summary: &story.smart_summary,
        is_read: story.is_read,
        is_saved: story.is_saved,
    }
}

pub fn story(data: &StoryData<'_>) -> String {
    let saved = if data.story.is_saved { " [saved]" } else { "" };
    let mut rendered = format!(
        "{}{}\n{}\n{}",
        data.story.title, saved, data.story.smart_summary, data.story.canonical_url
    );
    if let Some(summary) = &data.selected_summary {
        rendered.push_str(&format!(
            "\nSummary mode: AI\nProvider: {}\nModel: {}\nWhat happened: {}\nWhy it matters: {}",
            summary.provider,
            summary.model,
            summary.fields.what_happened,
            summary.fields.why_it_matters
        ));
        if let Some(caveat) = &summary.fields.caveat {
            rendered.push_str(&format!("\nCaveat: {caveat}"));
        }
    }
    rendered
}

pub fn stories(stories: &[Story]) -> String {
    if stories.is_empty() {
        return "No stories stored".to_owned();
    }

    stories
        .iter()
        .enumerate()
        .map(|(index, story)| {
            let saved = if story.is_saved { " [saved]" } else { "" };
            format!(
                "{}. {}{}\n{}\n{}",
                index + 1,
                story.title,
                saved,
                story.smart_summary,
                story.canonical_url
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn sources(sources: &[Source]) -> String {
    sources
        .iter()
        .map(|source| {
            let state = if source.enabled {
                "enabled"
            } else {
                "disabled"
            };
            format!("{}\t{}\t{}", source.id, state, source.name)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn model_profile_data(profile: &ModelProfile, is_default: bool) -> ModelProfileData {
    ModelProfileData {
        id: profile.id.hyphenated().to_string(),
        name: profile.name.clone(),
        provider: provider(profile.provider),
        model: profile.model.clone(),
        endpoint_host: profile
            .endpoint
            .as_ref()
            .and_then(|endpoint| endpoint.host_str())
            .map(str::to_owned),
        dialect: profile.dialect.map(dialect),
        credential_source: match profile.credential {
            CredentialRef::SystemStore { .. } => "system-store",
            CredentialRef::Environment { .. } => "environment",
        },
        consented: profile.consented_at.is_some(),
        enabled: profile.enabled,
        is_default,
        limits: profile.limits.clone(),
    }
}

pub fn model_profiles_data(
    profiles: &[ModelProfile],
    default_id: Option<uuid::Uuid>,
) -> Vec<ModelProfileData> {
    profiles
        .iter()
        .map(|profile| model_profile_data(profile, default_id == Some(profile.id)))
        .collect()
}

pub fn model_profile(profile: &ModelProfileData) -> String {
    model_profiles(std::slice::from_ref(profile))
}

pub fn model_profiles(profiles: &[ModelProfileData]) -> String {
    if profiles.is_empty() {
        return "No model profiles configured".to_owned();
    }
    profiles
        .iter()
        .map(|profile| {
            let endpoint = profile
                .endpoint_host
                .as_deref()
                .map_or_else(String::new, |host| format!("\nEndpoint host: {host}"));
            let dialect = profile
                .dialect
                .map_or_else(String::new, |value| format!("\nDialect: {value}"));
            let daily_budget = profile
                .limits
                .max_daily_cost_microusd
                .map_or_else(|| "none".to_owned(), format_usd);
            let input_rate = profile
                .limits
                .input_cost_microusd_per_million
                .map_or_else(|| "none".to_owned(), format_usd);
            let output_rate = profile
                .limits
                .output_cost_microusd_per_million
                .map_or_else(|| "none".to_owned(), format_usd);
            format!(
                "{}\nProvider: {}\nModel: {}{}{}\nCredential source: {}\nConsented: {}\nDefault: {}\nEnabled: {}\nLimits: {} summaries/refresh, {} max output tokens, {}s timeout, {} retries\nRates: daily ${}, input ${}/million, output ${}/million",
                profile.name,
                profile.provider,
                profile.model,
                endpoint,
                dialect,
                profile.credential_source,
                yes_no(profile.consented),
                yes_no(profile.is_default),
                yes_no(profile.enabled),
                profile.limits.max_summaries_per_refresh,
                profile.limits.max_output_tokens,
                profile.limits.timeout_seconds,
                profile.limits.max_retries,
                daily_budget,
                input_rate,
                output_rate,
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn summarize_data(report: &SummarizeReport) -> SummarizeData {
    SummarizeData {
        story_id: report.story_id.clone(),
        status: manual_status(report.status),
        summary: report.summary.as_ref().map(summary_variant_data),
        attempt: report.attempt.as_ref().map(generation_attempt_data),
        generation: report.generation.clone(),
    }
}

pub fn summarize(report: &SummarizeData) -> String {
    let mut rendered = format!(
        "Summary mode: {}\nStatus: {}\n{}",
        if report.summary.is_some() {
            "AI"
        } else {
            "Smart fallback"
        },
        report.status,
        generation(&report.generation)
    );
    if let Some(summary) = &report.summary {
        rendered.push_str(&format!(
            "\nProvider: {}\nModel: {}\nWhat happened: {}\nWhy it matters: {}",
            summary.provider,
            summary.model,
            summary.fields.what_happened,
            summary.fields.why_it_matters
        ));
        if let Some(caveat) = &summary.fields.caveat {
            rendered.push_str(&format!("\nCaveat: {caveat}"));
        }
    }
    rendered
}

pub fn test_model_data(report: &TestModelReport) -> TestModelData {
    TestModelData {
        status: manual_status(report.status),
        cost_may_apply: report.cost_may_apply,
        attempt: report.attempt.as_ref().map(generation_attempt_data),
        generation: report.generation.clone(),
    }
}

pub fn test_model(report: &TestModelData) -> String {
    let mut rendered = format!(
        "Model test status: {}\nCost may apply: {}\n{}",
        report.status,
        yes_no(report.cost_may_apply),
        generation(&report.generation)
    );
    if let Some(attempt) = &report.attempt {
        rendered.push_str(&format!(
            "\nProvider: {}\nModel: {}",
            attempt.provider, attempt.model
        ));
    }
    rendered
}

pub fn remove_model(report: &RemoveModelReport) -> String {
    let mut rendered = "Model profile removed".to_owned();
    if report.warning.is_some() {
        rendered
            .push_str("\nWarning: the stored credential could not be deleted; remove it manually");
    }
    rendered
}

fn summary_variant_data(summary: &SummaryVariant) -> SummaryVariantData {
    SummaryVariantData {
        provider: provider(summary.provider),
        model: summary.model.clone(),
        endpoint_host: summary.endpoint.as_deref().and_then(endpoint_host),
        dialect: summary.dialect.map(dialect),
        fields: summary.fields.clone(),
        input_tokens: summary.input_tokens,
        output_tokens: summary.output_tokens,
        cost_microusd: summary.cost_microusd,
        generated_at: summary.generated_at.to_rfc3339(),
    }
}

fn generation_attempt_data(attempt: &GenerationAttempt) -> GenerationAttemptData {
    GenerationAttemptData {
        provider: provider(attempt.provider),
        model: attempt.model.clone(),
        endpoint_host: attempt.endpoint.as_deref().and_then(endpoint_host),
        dialect: attempt.dialect.map(dialect),
        input_tokens: attempt.input_tokens,
        output_tokens: attempt.output_tokens,
        cost_microusd: attempt.actual_cost_microusd,
        failure_kind: attempt.failure_kind,
    }
}

fn summary_variant(summary: &SummaryVariantData) -> String {
    let mut rendered = format!(
        "Summary mode: AI\nProvider: {}\nModel: {}\nWhat happened: {}\nWhy it matters: {}\n",
        summary.provider,
        summary.model,
        summary.fields.what_happened,
        summary.fields.why_it_matters
    );
    if let Some(caveat) = &summary.fields.caveat {
        rendered.push_str(&format!("Caveat: {caveat}\n"));
    }
    rendered
}

fn endpoint_host(value: &str) -> Option<String> {
    value
        .parse::<url::Url>()
        .ok()
        .and_then(|endpoint| endpoint.host_str().map(str::to_owned))
}

fn manual_status(status: ManualGenerationStatus) -> &'static str {
    match status {
        ManualGenerationStatus::Generated => "generated",
        ManualGenerationStatus::CacheHit => "cache_hit",
        ManualGenerationStatus::BudgetExhausted => "budget_exhausted",
        ManualGenerationStatus::CredentialUnavailable => "credential_unavailable",
        ManualGenerationStatus::ConsentRequired => "consent_required",
        ManualGenerationStatus::ProfileUnavailable => "profile_unavailable",
        ManualGenerationStatus::RefreshCapReached => "refresh_cap_reached",
        ManualGenerationStatus::ProviderFailure => "provider_failure",
        ManualGenerationStatus::MalformedOutput => "malformed_output",
    }
}

fn generation(report: &GenerationReport) -> String {
    format!(
        "AI generation: {} generated, {} cache hits, {} cap skips, {} budget skips, {} missing credentials, {} provider failures, {} malformed outputs, {} Smart fallbacks",
        report.generated,
        report.cache_hits,
        report.skipped_cap,
        report.skipped_budget,
        report.missing_credentials,
        report.provider_failures,
        report.malformed_outputs,
        report.smart_fallbacks
    )
}

fn provider(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::OpenAi => "open-ai",
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::Gemini => "gemini",
        ProviderKind::OpenAiCompatible => "open-ai-compatible",
    }
}

fn dialect(dialect: ApiDialect) -> &'static str {
    match dialect {
        ApiDialect::Responses => "responses",
        ApiDialect::ChatCompletions => "chat-completions",
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn format_usd(micros: u64) -> String {
    let whole = micros / 1_000_000;
    let fraction = micros % 1_000_000;
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction:06}")
            .trim_end_matches('0')
            .to_owned()
    }
}
