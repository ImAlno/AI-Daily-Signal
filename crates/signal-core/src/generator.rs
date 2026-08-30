use std::time::Duration as StdDuration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AI_SUMMARY_PROMPT_VERSION, AttemptOutcome, Briefing, BudgetDecision, CancellationToken,
    CredentialResolver, CredentialStore, EnvironmentReader, GenerationAttempt,
    GenerationFailureKind, GenerationReport, ModelProfile, ProviderFailure, ProviderFailureKind,
    ProviderRegistry, ProviderRequest, ProviderUsage, RequestChargeStatus, Result, RetryPolicy,
    SignalError, Store, Story, SummarySettings, SummaryVariant, build_ai_summary_prompt,
    summary_cache_key,
};

const RESERVATION_SAFETY_MARGIN_SECONDS: u64 = 60;
const COST_DENOMINATOR: u128 = 1_000_000;
const PROVIDER_FRAMING_TOKEN_ALLOWANCE: u64 = 1_024;
const MODEL_TEST_PUBLISHED_AT: DateTime<Utc> = DateTime::<Utc>::UNIX_EPOCH;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SummarizeOptions {
    pub profile: Option<String>,
    pub force: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManualGenerationStatus {
    Generated,
    CacheHit,
    BudgetExhausted,
    CredentialUnavailable,
    ConsentRequired,
    ProfileUnavailable,
    RefreshCapReached,
    ProviderFailure,
    MalformedOutput,
}

impl ManualGenerationStatus {
    pub fn is_success(self) -> bool {
        matches!(self, Self::Generated | Self::CacheHit)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SummarizeReport {
    pub story_id: String,
    pub status: ManualGenerationStatus,
    pub summary: Option<SummaryVariant>,
    pub attempt: Option<GenerationAttempt>,
    pub generation: GenerationReport,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestModelOptions {
    pub profile: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestModelReport {
    pub profile_id: Uuid,
    pub cost_may_apply: bool,
    pub status: ManualGenerationStatus,
    pub attempt: Option<GenerationAttempt>,
    pub generation: GenerationReport,
}

pub struct AiGenerationCoordinator<'a> {
    store: &'a Store,
    credentials: &'a dyn CredentialStore,
    environment: &'a dyn EnvironmentReader,
    providers: &'a ProviderRegistry,
    settings: SummarySettings,
}

impl<'a> AiGenerationCoordinator<'a> {
    pub fn new(
        store: &'a Store,
        credentials: &'a dyn CredentialStore,
        environment: &'a dyn EnvironmentReader,
        providers: &'a ProviderRegistry,
    ) -> Self {
        Self {
            store,
            credentials,
            environment,
            providers,
            settings: SummarySettings::default(),
        }
    }

    pub async fn generate_briefing(
        &self,
        briefing: &mut Briefing,
        profile: Option<&ModelProfile>,
        now: DateTime<Utc>,
    ) -> Result<GenerationReport> {
        let cancellation = CancellationToken::new();
        self.generate_briefing_with_cancel(briefing, profile, now, &cancellation)
            .await
    }

    pub async fn generate_briefing_with_cancel(
        &self,
        briefing: &mut Briefing,
        profile: Option<&ModelProfile>,
        now: DateTime<Utc>,
        cancellation: &CancellationToken,
    ) -> Result<GenerationReport> {
        cancellation.check()?;
        let mut report = GenerationReport {
            eligible: briefing.items.len(),
            ..GenerationReport::default()
        };
        let Some(profile) = profile.filter(|profile| automatic_profile_is_usable(profile)) else {
            report.smart_fallbacks = briefing.items.len();
            return Ok(report);
        };

        let mut outbound_requests = 0_u32;
        let maximum_requests = profile.limits.max_summaries_per_refresh;
        for item in &mut briefing.items {
            cancellation.check()?;
            let generated = self
                .generate_story(
                    &item.story,
                    profile,
                    now,
                    GenerationControl {
                        force: false,
                        persist_variant: true,
                        refresh_cap: Some(RefreshCap {
                            used: &mut outbound_requests,
                            maximum: maximum_requests,
                        }),
                        cancellation: Some(cancellation),
                        produce_variant: true,
                    },
                )
                .await?;
            update_report(&mut report, &generated);
            if let Some(variant) = generated.variant {
                item.selected_summary = Some(variant);
            }
        }
        Ok(report)
    }

    pub(crate) async fn generate_briefing_staged_with_cancel(
        &self,
        briefing: &mut Briefing,
        profile: Option<&ModelProfile>,
        now: DateTime<Utc>,
        cancellation: &CancellationToken,
    ) -> Result<StagedGeneration> {
        cancellation.check()?;
        let mut report = GenerationReport {
            eligible: briefing.items.len(),
            ..GenerationReport::default()
        };
        let Some(profile) = profile.filter(|profile| automatic_profile_is_usable(profile)) else {
            report.smart_fallbacks = briefing.items.len();
            return Ok(StagedGeneration {
                report,
                variants: Vec::new(),
            });
        };

        let mut variants = Vec::new();
        let mut outbound_requests = 0_u32;
        let maximum_requests = profile.limits.max_summaries_per_refresh;
        for item in &mut briefing.items {
            cancellation.check()?;
            let generated = self
                .generate_story(
                    &item.story,
                    profile,
                    now,
                    GenerationControl {
                        force: false,
                        persist_variant: false,
                        produce_variant: true,
                        refresh_cap: Some(RefreshCap {
                            used: &mut outbound_requests,
                            maximum: maximum_requests,
                        }),
                        cancellation: Some(cancellation),
                    },
                )
                .await?;
            update_report(&mut report, &generated);
            if let Some(variant) = generated.variant {
                item.selected_summary = Some(variant.clone());
                if generated.status == ManualGenerationStatus::Generated {
                    variants.push(variant);
                }
            }
        }
        Ok(StagedGeneration { report, variants })
    }

    pub async fn summarize(
        &self,
        story: &Story,
        profile: &ModelProfile,
        force: bool,
        now: DateTime<Utc>,
    ) -> Result<SummarizeReport> {
        let generated = self
            .generate_story(
                story,
                profile,
                now,
                GenerationControl {
                    force,
                    persist_variant: true,
                    refresh_cap: None,
                    cancellation: None,
                    produce_variant: true,
                },
            )
            .await?;
        let mut generation = GenerationReport {
            eligible: 1,
            ..GenerationReport::default()
        };
        update_report(&mut generation, &generated);
        Ok(SummarizeReport {
            story_id: story.id.clone(),
            status: generated.status,
            summary: generated.variant,
            attempt: generated.attempt,
            generation,
        })
    }

    pub async fn test_model(
        &self,
        profile: &ModelProfile,
        now: DateTime<Utc>,
    ) -> Result<TestModelReport> {
        let story = synthetic_test_story();
        let generated = self
            .generate_story(
                &story,
                profile,
                now,
                GenerationControl {
                    force: true,
                    persist_variant: false,
                    refresh_cap: None,
                    cancellation: None,
                    produce_variant: false,
                },
            )
            .await?;
        let mut generation = GenerationReport {
            eligible: 1,
            ..GenerationReport::default()
        };
        update_report(&mut generation, &generated);
        Ok(TestModelReport {
            profile_id: profile.id,
            cost_may_apply: true,
            status: generated.status,
            attempt: generated.attempt,
            generation,
        })
    }

    async fn generate_story(
        &self,
        story: &Story,
        profile: &ModelProfile,
        now: DateTime<Utc>,
        mut control: GenerationControl<'_>,
    ) -> Result<SingleGeneration> {
        if let Some(cancellation) = control.cancellation {
            cancellation.check()?;
        }
        if !profile.enabled {
            return Ok(SingleGeneration::skipped(
                ManualGenerationStatus::ProfileUnavailable,
            ));
        }
        if profile.consented_at.is_none() {
            return Ok(SingleGeneration::skipped(
                ManualGenerationStatus::ConsentRequired,
            ));
        }

        let cache_key =
            summary_cache_key(story, profile, AI_SUMMARY_PROMPT_VERSION, &self.settings)?;
        let prompt = match build_ai_summary_prompt(story, &self.settings) {
            Ok(prompt) => prompt,
            Err(_) => {
                return Ok(SingleGeneration::skipped(
                    ManualGenerationStatus::MalformedOutput,
                ));
            }
        };
        if !control.force
            && let Some(variant) = self.store.find_cached_summary(&cache_key)?
        {
            return Ok(SingleGeneration {
                status: ManualGenerationStatus::CacheHit,
                variant: Some(variant),
                attempt: None,
            });
        }

        let request = match ProviderRequest::from_profile(story.id.clone(), profile, prompt.clone())
        {
            Ok(request) => request,
            Err(_) => {
                return Ok(SingleGeneration::skipped(
                    ManualGenerationStatus::ProviderFailure,
                ));
            }
        };
        let credential = match CredentialResolver::new(self.credentials, self.environment)
            .resolve(&profile.credential)
        {
            Ok(credential) => credential,
            Err(_) => {
                return Ok(SingleGeneration::skipped(
                    ManualGenerationStatus::CredentialUnavailable,
                ));
            }
        };

        if let Some(cap) = control.refresh_cap.as_mut()
            && *cap.used >= cap.maximum
        {
            return Ok(SingleGeneration::skipped_cap());
        }

        let estimated_input_tokens = match estimate_prompt_tokens(&prompt) {
            Ok(tokens) => tokens,
            Err(_) => {
                return Ok(SingleGeneration::skipped(
                    ManualGenerationStatus::BudgetExhausted,
                ));
            }
        };
        let estimated_output_tokens = u64::from(profile.limits.max_output_tokens);
        let estimated_cost =
            match request_cost(estimated_input_tokens, estimated_output_tokens, profile) {
                Ok(cost) => cost,
                Err(_) => {
                    return Ok(SingleGeneration::skipped(
                        ManualGenerationStatus::BudgetExhausted,
                    ));
                }
            };
        if i64::try_from(estimated_cost).is_err() {
            return Ok(SingleGeneration::skipped(
                ManualGenerationStatus::BudgetExhausted,
            ));
        }
        let expires_at = match reservation_expiry(now, profile) {
            Ok(expires_at) => expires_at,
            Err(_) => {
                return Ok(SingleGeneration::skipped(
                    ManualGenerationStatus::BudgetExhausted,
                ));
            }
        };
        let attempt_id = Uuid::new_v4();
        let reservation =
            self.store
                .reserve_generation(profile, attempt_id, now, estimated_cost, expires_at)?;
        if matches!(reservation, BudgetDecision::Exhausted) {
            return Ok(SingleGeneration::skipped(
                ManualGenerationStatus::BudgetExhausted,
            ));
        }

        let Some(provider) = self.providers.provider(profile.provider) else {
            let failure = ProviderFailure::new(
                ProviderFailureKind::ProviderRejected,
                RequestChargeStatus::NotSent,
            );
            let attempt = self.finalize_failure(attempt_id, now, estimated_cost, failure)?;
            return Ok(SingleGeneration {
                status: ManualGenerationStatus::ProviderFailure,
                variant: None,
                attempt: Some(attempt),
            });
        };

        if let Some(cancellation) = control.cancellation {
            if cancellation.is_cancelled() {
                self.store.finalize_generation(
                    attempt_id,
                    now,
                    AttemptOutcome::FailedUncharged {
                        category: GenerationFailureKind::Cancelled,
                    },
                )?;
                cancellation.check()?;
            }
        }
        let provider_response = match control.cancellation {
            Some(cancellation) => {
                provider
                    .generate_with_cancel(&request, &credential, cancellation)
                    .await
            }
            None => provider.generate(&request, &credential).await,
        };
        let response = match provider_response {
            Ok(response) => {
                consume_refresh_cap(&mut control)?;
                response
            }
            Err(failure) => {
                if failure.charge_status() == RequestChargeStatus::PossiblySent {
                    consume_refresh_cap(&mut control)?;
                }
                let status = if failure.kind() == ProviderFailureKind::MalformedOutput {
                    ManualGenerationStatus::MalformedOutput
                } else {
                    ManualGenerationStatus::ProviderFailure
                };
                let attempt = self.finalize_failure(attempt_id, now, estimated_cost, failure)?;
                if let Some(cancellation) = control.cancellation {
                    cancellation.check()?;
                }
                return Ok(SingleGeneration {
                    status,
                    variant: None,
                    attempt: Some(attempt),
                });
            }
        };

        if response.fields.validate(&self.settings).is_err() {
            let attempt = self.store.finalize_generation(
                attempt_id,
                now,
                AttemptOutcome::FailedCharged {
                    category: GenerationFailureKind::MalformedOutput,
                    cost_microusd: estimated_cost,
                },
            )?;
            if let Some(cancellation) = control.cancellation {
                cancellation.check()?;
            }
            return Ok(SingleGeneration {
                status: ManualGenerationStatus::MalformedOutput,
                variant: None,
                attempt: Some(attempt),
            });
        }

        let (input_tokens, output_tokens, actual_cost) = match response.usage {
            Some(usage) => match usage_cost(usage, profile) {
                Ok(cost)
                    if i64::try_from(usage.input_tokens).is_ok()
                        && i64::try_from(usage.output_tokens).is_ok()
                        && i64::try_from(cost).is_ok() =>
                {
                    (Some(usage.input_tokens), Some(usage.output_tokens), cost)
                }
                Err(_) => {
                    let attempt = self.store.finalize_generation(
                        attempt_id,
                        now,
                        AttemptOutcome::FailedCharged {
                            category: GenerationFailureKind::MalformedOutput,
                            cost_microusd: estimated_cost,
                        },
                    )?;
                    if let Some(cancellation) = control.cancellation {
                        cancellation.check()?;
                    }
                    return Ok(SingleGeneration {
                        status: ManualGenerationStatus::MalformedOutput,
                        variant: None,
                        attempt: Some(attempt),
                    });
                }
                Ok(_) => {
                    let attempt = self.store.finalize_generation(
                        attempt_id,
                        now,
                        AttemptOutcome::FailedCharged {
                            category: GenerationFailureKind::MalformedOutput,
                            cost_microusd: estimated_cost,
                        },
                    )?;
                    if let Some(cancellation) = control.cancellation {
                        cancellation.check()?;
                    }
                    return Ok(SingleGeneration {
                        status: ManualGenerationStatus::MalformedOutput,
                        variant: None,
                        attempt: Some(attempt),
                    });
                }
            },
            None => (None, None, estimated_cost),
        };
        let attempt = self.store.finalize_generation(
            attempt_id,
            now,
            AttemptOutcome::Completed {
                input_tokens,
                output_tokens,
                cost_microusd: actual_cost,
            },
        )?;

        if let Some(cancellation) = control.cancellation {
            cancellation.check()?;
        }

        let variant = control.produce_variant.then(|| SummaryVariant {
            id: Uuid::new_v4(),
            story_id: story.id.clone(),
            profile_id: Some(profile.id),
            provider: profile.provider,
            model: profile.model.clone(),
            endpoint: profile.endpoint.as_ref().map(ToString::to_string),
            dialect: profile.dialect,
            prompt_version: AI_SUMMARY_PROMPT_VERSION.to_owned(),
            cache_key,
            fields: response.fields,
            input_tokens,
            output_tokens,
            cost_microusd: actual_cost,
            generated_at: now,
        });
        if control.persist_variant
            && let Some(variant) = &variant
        {
            self.store.insert_summary_variant(variant)?;
        }
        Ok(SingleGeneration {
            status: ManualGenerationStatus::Generated,
            variant,
            attempt: Some(attempt),
        })
    }

    fn finalize_failure(
        &self,
        attempt_id: Uuid,
        now: DateTime<Utc>,
        estimated_cost: u64,
        failure: ProviderFailure,
    ) -> Result<GenerationAttempt> {
        let category = GenerationFailureKind::from(failure.kind());
        let outcome = match failure.charge_status() {
            RequestChargeStatus::NotSent => AttemptOutcome::FailedUncharged { category },
            RequestChargeStatus::PossiblySent => AttemptOutcome::FailedCharged {
                category,
                cost_microusd: estimated_cost,
            },
        };
        self.store.finalize_generation(attempt_id, now, outcome)
    }
}

struct RefreshCap<'a> {
    used: &'a mut u32,
    maximum: u32,
}

struct GenerationControl<'a> {
    force: bool,
    persist_variant: bool,
    produce_variant: bool,
    refresh_cap: Option<RefreshCap<'a>>,
    cancellation: Option<&'a CancellationToken>,
}

pub(crate) struct StagedGeneration {
    pub report: GenerationReport,
    pub variants: Vec<SummaryVariant>,
}

struct SingleGeneration {
    status: ManualGenerationStatus,
    variant: Option<SummaryVariant>,
    attempt: Option<GenerationAttempt>,
}

fn consume_refresh_cap(control: &mut GenerationControl<'_>) -> Result<()> {
    if let Some(cap) = control.refresh_cap.as_mut() {
        *cap.used = (*cap.used).checked_add(1).ok_or_else(|| {
            SignalError::InvalidConfiguration(
                "refresh generation cap arithmetic overflow".to_owned(),
            )
        })?;
    }
    Ok(())
}

impl SingleGeneration {
    fn skipped(status: ManualGenerationStatus) -> Self {
        Self {
            status,
            variant: None,
            attempt: None,
        }
    }

    fn skipped_cap() -> Self {
        Self::skipped(ManualGenerationStatus::RefreshCapReached)
    }
}

fn automatic_profile_is_usable(profile: &ModelProfile) -> bool {
    profile.enabled && profile.consented_at.is_some()
}

fn update_report(report: &mut GenerationReport, generated: &SingleGeneration) {
    match generated.status {
        ManualGenerationStatus::Generated => report.generated += 1,
        ManualGenerationStatus::CacheHit => report.cache_hits += 1,
        ManualGenerationStatus::BudgetExhausted => report.skipped_budget += 1,
        ManualGenerationStatus::CredentialUnavailable => report.missing_credentials += 1,
        ManualGenerationStatus::MalformedOutput => report.malformed_outputs += 1,
        ManualGenerationStatus::ProviderFailure => report.provider_failures += 1,
        ManualGenerationStatus::RefreshCapReached => report.skipped_cap += 1,
        ManualGenerationStatus::ConsentRequired | ManualGenerationStatus::ProfileUnavailable => {}
    }
    if !generated.status.is_success() {
        report.smart_fallbacks += 1;
    }
}

fn estimate_prompt_tokens(prompt: &crate::AiSummaryPrompt) -> Result<u64> {
    // Provider tokenizers differ, but their byte-fallback vocabularies cannot require more
    // content tokens than the canonical prompt's UTF-8 byte length. Reserve one token per byte
    // plus a fixed allowance for provider message framing and the structured-output schema.
    let prompt_bytes = prompt
        .system_text
        .len()
        .checked_add(prompt.user_text.len())
        .ok_or_else(|| arithmetic_error("prompt length"))?;
    let prompt_bytes =
        u64::try_from(prompt_bytes).map_err(|_| arithmetic_error("prompt length"))?;
    prompt_bytes
        .checked_add(PROVIDER_FRAMING_TOKEN_ALLOWANCE)
        .ok_or_else(|| arithmetic_error("prompt token estimate"))
}

fn request_cost(input_tokens: u64, output_tokens: u64, profile: &ModelProfile) -> Result<u64> {
    let (Some(input_rate), Some(output_rate)) = (
        profile.limits.input_cost_microusd_per_million,
        profile.limits.output_cost_microusd_per_million,
    ) else {
        return Ok(0);
    };
    let input = component_cost(input_tokens, input_rate)?;
    let output = component_cost(output_tokens, output_rate)?;
    input
        .checked_add(output)
        .ok_or_else(|| arithmetic_error("generation cost"))
}

fn usage_cost(usage: ProviderUsage, profile: &ModelProfile) -> Result<u64> {
    request_cost(usage.input_tokens, usage.output_tokens, profile)
}

fn component_cost(tokens: u64, rate: u64) -> Result<u64> {
    let product = u128::from(tokens)
        .checked_mul(u128::from(rate))
        .ok_or_else(|| arithmetic_error("generation cost"))?;
    let rounded = product
        .checked_add(COST_DENOMINATOR - 1)
        .ok_or_else(|| arithmetic_error("generation cost"))?
        / COST_DENOMINATOR;
    u64::try_from(rounded).map_err(|_| arithmetic_error("generation cost"))
}

fn reservation_expiry(now: DateTime<Utc>, profile: &ModelProfile) -> Result<DateTime<Utc>> {
    let attempts = profile
        .limits
        .max_retries
        .checked_add(1)
        .ok_or_else(|| arithmetic_error("retry count"))?;
    let attempt_time = StdDuration::from_secs(profile.limits.timeout_seconds)
        .checked_mul(attempts)
        .ok_or_else(|| arithmetic_error("reservation expiry"))?;
    let retry_policy = RetryPolicy::new(
        StdDuration::from_secs(profile.limits.timeout_seconds),
        profile.limits.max_retries,
    );
    let horizon = attempt_time
        .checked_add(retry_policy.maximum_total_delay())
        .and_then(|value| {
            value.checked_add(StdDuration::from_secs(RESERVATION_SAFETY_MARGIN_SECONDS))
        })
        .ok_or_else(|| arithmetic_error("reservation expiry"))?;
    let horizon =
        chrono::Duration::from_std(horizon).map_err(|_| arithmetic_error("reservation expiry"))?;
    now.checked_add_signed(horizon)
        .ok_or_else(|| arithmetic_error("reservation expiry"))
}

fn arithmetic_error(field: &str) -> SignalError {
    SignalError::InvalidConfiguration(format!("{field} arithmetic overflow"))
}

fn synthetic_test_story() -> Story {
    Story {
        id: "model-test".to_owned(),
        title: "Synthetic public AI model connectivity test".to_owned(),
        canonical_url: "https://example.com/ai-daily-signal/model-test".to_owned(),
        excerpt: "This fixed synthetic public story tests structured summary generation without transmitting private user data.".to_owned(),
        category: "test".to_owned(),
        published_at: Some(MODEL_TEST_PUBLISHED_AT),
        source_ids: vec!["synthetic-public-test".to_owned()],
        score: crate::ScoreBreakdown {
            recency: 0.0,
            source_weight: 0.0,
            corroboration: 0.0,
            total: 0.0,
        },
        smart_summary: "Synthetic public AI model connectivity test".to_owned(),
        is_read: false,
        is_saved: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AiSummaryPrompt, ProviderKind, test_support};

    #[test]
    fn prompt_estimate_uses_utf8_bytes_plus_provider_framing_allowance() {
        let prompt = AiSummaryPrompt {
            system_text: "é🙂".to_owned(),
            user_text: "中文".to_owned(),
        };

        assert_eq!(estimate_prompt_tokens(&prompt).unwrap(), 1_036);
    }

    #[test]
    fn input_and_output_costs_are_rounded_up_separately() {
        let mut profile = test_support::model_profile("cost", ProviderKind::OpenAi);
        profile.limits.input_cost_microusd_per_million = Some(500_001);
        profile.limits.output_cost_microusd_per_million = Some(333_334);

        assert_eq!(request_cost(2, 3, &profile).unwrap(), 4);
    }
}
