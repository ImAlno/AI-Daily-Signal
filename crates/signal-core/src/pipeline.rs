use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use html_escape::decode_html_entities;
use sha2::{Digest, Sha256};
use url::Url;

use crate::{AppConfig, Briefing, BriefingItem, Candidate, ScoreBreakdown, Source, Story};

const BRIEFING_MAX_AGE: Duration = Duration::days(7);
const TITLE_DUPLICATE_WINDOW: Duration = Duration::hours(48);
const SMART_SUMMARY_LIMIT: usize = 360;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PipelineOutput {
    pub stories: Vec<Story>,
    pub briefing: Briefing,
}

pub struct Pipeline;

impl Pipeline {
    pub fn build(
        candidates: Vec<Candidate>,
        config: &AppConfig,
        now: DateTime<Utc>,
    ) -> PipelineOutput {
        let mut stories = deduplicate(candidates, config);
        for story in &mut stories {
            story.score = score_story(story, config, now);
            story.smart_summary = smart_summary(&story.excerpt, &story.title);
        }
        stories.sort_by(|left, right| left.id.cmp(&right.id));

        let briefing = assemble_briefing(&stories, config, now);
        PipelineOutput { stories, briefing }
    }
}

pub fn normalize_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value.trim()) else {
        return value.trim().to_owned();
    };

    url.set_fragment(None);
    if let Some(host) = url.host_str().map(str::to_ascii_lowercase) {
        let _ = url.set_host(Some(&host));
    }
    if matches!(
        (url.scheme(), url.port()),
        ("http", Some(80)) | ("https", Some(443))
    ) {
        let _ = url.set_port(None);
    }

    let mut query_pairs = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_parameter(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    query_pairs.sort_unstable();
    url.set_query(None);
    if !query_pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(query_pairs);
    }

    url.into()
}

pub fn normalize_title(value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_separator = true;

    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            normalized.push(character);
            previous_was_separator = false;
        } else if !previous_was_separator {
            normalized.push(' ');
            previous_was_separator = true;
        }
    }

    normalized.trim().to_owned()
}

pub fn deduplicate(candidates: Vec<Candidate>, config: &AppConfig) -> Vec<Story> {
    let mut candidates = candidates
        .into_iter()
        .map(|candidate| NormalizedCandidate {
            canonical_url: normalize_url(&candidate.canonical_url),
            normalized_title: normalize_title(&candidate.title),
            candidate,
        })
        .collect::<Vec<_>>();
    candidates.sort_by(candidate_order);

    let mut url_groups = BTreeMap::<String, Vec<NormalizedCandidate>>::new();
    for candidate in candidates {
        url_groups
            .entry(candidate.canonical_url.clone())
            .or_default()
            .push(candidate);
    }

    let mut groups = Vec::<Vec<NormalizedCandidate>>::new();
    for url_group in url_groups.into_values() {
        if let Some(index) = groups
            .iter()
            .position(|group| groups_can_merge(group, &url_group))
        {
            groups[index].extend(url_group);
        } else {
            groups.push(url_group);
        }
    }

    groups
        .into_iter()
        .map(|group| merge_group(group, config))
        .collect()
}

pub fn score_story(story: &Story, config: &AppConfig, now: DateTime<Utc>) -> ScoreBreakdown {
    let recency = story.published_at.map_or(10.0, |published_at| {
        let age_hours = now.signed_duration_since(published_at).num_seconds() as f64 / 3_600.0;
        (60.0 - (age_hours * 1.25)).clamp(0.0, 60.0)
    });
    let source_weight = story
        .source_ids
        .iter()
        .filter_map(|source_id| source_by_id(config, source_id))
        .map(|source| normalized_weight(source.weight))
        .max_by(f64::total_cmp)
        .unwrap_or(0.0)
        * 30.0;
    let corroboration = (((story.source_ids.len().saturating_sub(1)) * 5).min(10)) as f64;
    let total = (recency + source_weight + corroboration).clamp(0.0, 100.0);

    ScoreBreakdown {
        recency,
        source_weight,
        corroboration,
        total,
    }
}

pub fn smart_summary(excerpt: &str, title: &str) -> String {
    let plain_text = collapse_whitespace(&decode_html_entities(excerpt));
    if plain_text.is_empty() {
        return collapse_whitespace(title);
    }

    let mut last_complete_sentence = None;
    for (index, character) in plain_text.char_indices() {
        let after_character = index + character.len_utf8();
        if matches!(character, '.' | '!' | '?')
            && plain_text[after_character..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
            && plain_text[..after_character].chars().count() <= SMART_SUMMARY_LIMIT
        {
            last_complete_sentence = Some(after_character);
        }
    }
    if let Some(end) = last_complete_sentence {
        return plain_text[..end].to_owned();
    }
    collapse_whitespace(title)
}

pub fn assemble_briefing(stories: &[Story], config: &AppConfig, now: DateTime<Utc>) -> Briefing {
    let mut eligible = stories
        .iter()
        .filter(|story| story_is_fresh(story, now))
        .collect::<Vec<_>>();
    eligible.sort_by(|left, right| {
        right
            .score
            .total
            .total_cmp(&left.score.total)
            .then_with(|| right.published_at.cmp(&left.published_at))
            .then_with(|| left.id.cmp(&right.id))
    });

    let items = eligible
        .into_iter()
        .take(config.briefing.max_items)
        .enumerate()
        .map(|(index, story)| BriefingItem {
            position: u32::try_from(index + 1).unwrap_or(u32::MAX),
            section: "top_signals".to_owned(),
            is_stale: false,
            story: story.clone(),
        })
        .collect();

    Briefing {
        date: now.date_naive(),
        generated_at: now,
        items,
    }
}

struct NormalizedCandidate {
    candidate: Candidate,
    canonical_url: String,
    normalized_title: String,
}

fn candidate_order(left: &NormalizedCandidate, right: &NormalizedCandidate) -> std::cmp::Ordering {
    left.canonical_url
        .cmp(&right.canonical_url)
        .then_with(|| left.normalized_title.cmp(&right.normalized_title))
        .then_with(|| {
            left.candidate
                .published_at
                .cmp(&right.candidate.published_at)
        })
        .then_with(|| left.candidate.source_id.cmp(&right.candidate.source_id))
        .then_with(|| left.candidate.external_id.cmp(&right.candidate.external_id))
}

fn groups_can_merge(left: &[NormalizedCandidate], right: &[NormalizedCandidate]) -> bool {
    left.iter().all(|left_candidate| {
        right.iter().all(|right_candidate| {
            title_similarity(
                &left_candidate.normalized_title,
                &right_candidate.normalized_title,
            ) >= 0.9
                && published_within_window(
                    left_candidate.candidate.published_at,
                    right_candidate.candidate.published_at,
                    TITLE_DUPLICATE_WINDOW,
                )
        })
    })
}

fn title_similarity(left: &str, right: &str) -> f64 {
    let left_tokens = left.split_whitespace().collect::<BTreeSet<_>>();
    let right_tokens = right.split_whitespace().collect::<BTreeSet<_>>();
    let union_count = left_tokens.union(&right_tokens).count();
    if union_count == 0 {
        return 0.0;
    }
    left_tokens.intersection(&right_tokens).count() as f64 / union_count as f64
}

fn published_within_window(
    left: Option<DateTime<Utc>>,
    right: Option<DateTime<Utc>>,
    window: Duration,
) -> bool {
    matches!(
        (left, right),
        (Some(left), Some(right)) if left.signed_duration_since(right).abs() <= window
    )
}

fn merge_group(mut group: Vec<NormalizedCandidate>, config: &AppConfig) -> Story {
    group.sort_by(candidate_order);
    let preferred = group
        .iter()
        .min_by(|left, right| {
            normalized_weight(
                source_by_id(config, &right.candidate.source_id)
                    .map_or(0.0, |source| source.weight),
            )
            .total_cmp(&normalized_weight(
                source_by_id(config, &left.candidate.source_id).map_or(0.0, |source| source.weight),
            ))
            .then_with(|| candidate_order(left, right))
        })
        .expect("a deduplication group is never empty");
    let source_ids = group
        .iter()
        .map(|candidate| candidate.candidate.source_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let excerpt = group
        .iter()
        .map(|candidate| collapse_whitespace(&decode_html_entities(&candidate.candidate.excerpt)))
        .max_by(|left, right| {
            left.chars()
                .count()
                .cmp(&right.chars().count())
                .then_with(|| right.cmp(left))
        })
        .unwrap_or_default();
    let published_at = group
        .iter()
        .filter_map(|candidate| candidate.candidate.published_at)
        .max();
    let category = source_by_id(config, &preferred.candidate.source_id)
        .map(|source| source.category.clone())
        .unwrap_or_else(|| "uncategorized".to_owned());
    let id = format!("{:x}", Sha256::digest(preferred.canonical_url.as_bytes()));

    Story {
        id,
        title: preferred.candidate.title.clone(),
        canonical_url: preferred.canonical_url.clone(),
        excerpt,
        category,
        published_at,
        source_ids,
        score: ScoreBreakdown {
            recency: 0.0,
            source_weight: 0.0,
            corroboration: 0.0,
            total: 0.0,
        },
        smart_summary: String::new(),
        is_read: false,
        is_saved: false,
    }
}

fn story_is_fresh(story: &Story, now: DateTime<Utc>) -> bool {
    story
        .published_at
        .is_none_or(|published_at| now.signed_duration_since(published_at) <= BRIEFING_MAX_AGE)
}

fn source_by_id<'a>(config: &'a AppConfig, source_id: &str) -> Option<&'a Source> {
    config.sources.iter().find(|source| source.id == source_id)
}

fn normalized_weight(weight: f64) -> f64 {
    if weight.is_finite() {
        weight.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn is_tracking_parameter(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_") || matches!(key.as_str(), "fbclid" | "gclid")
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
