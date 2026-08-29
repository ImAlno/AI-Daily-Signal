use serde::{Deserialize, Serialize};

use crate::{AiSummaryFields, Story, SummarySettings, normalize_title, normalize_url};

use super::{ProviderFailure, ProviderFailureKind, RequestChargeStatus};

pub const AI_SUMMARY_PROMPT_VERSION: &str = "ai-summary-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiSummaryPrompt {
    pub system_text: String,
    pub user_text: String,
}

#[derive(Serialize)]
struct PromptStory {
    normalized_title: String,
    excerpt: String,
    canonical_url: String,
    published_at: Option<String>,
    category: String,
    source_ids: Vec<String>,
}

pub fn build_ai_summary_prompt(
    story: &Story,
    settings: &SummarySettings,
) -> Result<AiSummaryPrompt, ProviderFailure> {
    let mut source_ids = story
        .source_ids
        .iter()
        .map(|source_id| collapse_whitespace(source_id))
        .collect::<Vec<_>>();
    source_ids.sort();

    let prompt_story = PromptStory {
        normalized_title: normalize_title(&story.title),
        excerpt: clean_excerpt(&story.excerpt),
        canonical_url: canonical_prompt_url(&story.canonical_url),
        published_at: story.published_at.map(|value| value.to_rfc3339()),
        category: collapse_whitespace(&story.category),
        source_ids,
    };
    let user_text = serde_json::to_string(&prompt_story).map_err(|_| {
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

fn clean_excerpt(value: &str) -> String {
    let mut plain_text = String::new();
    let mut inside_tag = false;
    for character in value.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => plain_text.push(character),
            _ => {}
        }
    }
    collapse_whitespace(&html_escape::decode_html_entities(&plain_text))
}

fn canonical_prompt_url(value: &str) -> String {
    let normalized = normalize_url(value);
    let Ok(mut url) = url::Url::parse(&normalized) else {
        return normalized;
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.into()
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
