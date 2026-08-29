use serde::Deserialize;

use crate::summaries::canonical_summary_story;
use crate::{AiSummaryFields, Story, SummarySettings};

use super::{ProviderFailure, ProviderFailureKind, RequestChargeStatus};

pub const AI_SUMMARY_PROMPT_VERSION: &str = "ai-summary-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiSummaryPrompt {
    pub system_text: String,
    pub user_text: String,
}

pub fn build_ai_summary_prompt(
    story: &Story,
    settings: &SummarySettings,
) -> Result<AiSummaryPrompt, ProviderFailure> {
    let user_text = serde_json::to_string(&canonical_summary_story(story)).map_err(|_| {
        ProviderFailure::new(
            ProviderFailureKind::MalformedOutput,
            RequestChargeStatus::NotSent,
        )
    })?;
    let system_text = format!(
        "Prompt version {AI_SUMMARY_PROMPT_VERSION}. Use only facts in the supplied story. Return exactly one JSON object with no code fence, commentary, trailing content, HTML, or Markdown links. The object must have required nonblank string fields what_happened (at most {} characters) and why_it_matters (at most {} characters), plus caveat as a string of at most {} characters or null. Include no unknown fields. Put uncertainty in caveat. Do not give investment, medical, or legal advice.",
        settings.what_happened_max_chars,
        settings.why_it_matters_max_chars,
        settings.caveat_max_chars,
    );

    Ok(AiSummaryPrompt {
        system_text,
        user_text,
    })
}

pub fn parse_ai_summary(
    value: &str,
    settings: &SummarySettings,
) -> Result<AiSummaryFields, ProviderFailure> {
    let trimmed = value.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err(malformed_output());
    }

    let mut deserializer = serde_json::Deserializer::from_str(trimmed);
    let fields = AiSummaryFields::deserialize(&mut deserializer).map_err(|_| malformed_output())?;
    deserializer.end().map_err(|_| malformed_output())?;
    fields.validate(settings).map_err(|_| malformed_output())?;
    Ok(fields)
}

fn malformed_output() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureKind::MalformedOutput,
        RequestChargeStatus::PossiblySent,
    )
}
