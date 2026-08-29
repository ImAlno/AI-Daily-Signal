use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::{Candidate, Result, SignalError, Source, SourceKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFailure {
    pub source_id: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CollectionReport {
    pub candidates: Vec<Candidate>,
    pub successful_source_ids: Vec<String>,
    pub failures: Vec<SourceFailure>,
}

pub struct FeedCollector {
    client: reqwest::Client,
}

impl FeedCollector {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("AI-Daily-Signal/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;

        Ok(Self { client })
    }

    pub fn parse(
        source: &Source,
        bytes: &[u8],
        collected_at: DateTime<Utc>,
    ) -> Result<Vec<Candidate>> {
        let feed =
            feed_rs::parser::parse(bytes).map_err(|error| SignalError::Feed(error.to_string()))?;

        Ok(feed
            .entries
            .iter()
            .filter_map(|entry| Self::candidate_from_entry(source, entry, collected_at))
            .collect())
    }

    pub async fn fetch(&self, source: &Source) -> Result<Vec<Candidate>> {
        self.fetch_at(source, Utc::now()).await
    }

    pub async fn collect_all(
        &self,
        sources: &[Source],
        collected_at: DateTime<Utc>,
    ) -> CollectionReport {
        let mut report = CollectionReport {
            candidates: Vec::new(),
            successful_source_ids: Vec::new(),
            failures: Vec::new(),
        };

        for source in sources.iter().filter(|source| source.enabled) {
            match self.fetch_at(source, collected_at).await {
                Ok(candidates) => {
                    report.successful_source_ids.push(source.id.clone());
                    report.candidates.extend(candidates);
                }
                Err(_) => report.failures.push(SourceFailure {
                    source_id: source.id.clone(),
                    message: "source could not be collected".to_owned(),
                }),
            }
        }

        report
    }

    async fn fetch_at(
        &self,
        source: &Source,
        collected_at: DateTime<Utc>,
    ) -> Result<Vec<Candidate>> {
        let response = self
            .client
            .get(feed_url(source))
            .send()
            .await?
            .error_for_status()?;
        let bytes = response.bytes().await?;
        Self::parse(source, bytes.as_ref(), collected_at)
    }

    fn candidate_from_entry(
        source: &Source,
        entry: &feed_rs::model::Entry,
        collected_at: DateTime<Utc>,
    ) -> Option<Candidate> {
        let title = normalized_text(entry.title.as_ref()?.content.as_str());
        if title.is_empty() {
            return None;
        }

        let canonical_url = entry
            .links
            .iter()
            .find(|link| is_alternate_link(link))
            .and_then(|link| usable_url(link.href.as_str()))
            .or_else(|| usable_url(entry.id.as_str()))?;

        let external_id = if entry.id.trim().is_empty() {
            canonical_url.clone()
        } else {
            entry.id.trim().to_owned()
        };
        let excerpt = entry
            .summary
            .as_ref()
            .map(|summary| summary.content.as_str())
            .or_else(|| {
                entry
                    .content
                    .as_ref()
                    .and_then(|content| content.body.as_deref())
            })
            .map(normalized_text)
            .unwrap_or_default();

        Some(Candidate {
            source_id: source.id.clone(),
            external_id,
            canonical_url,
            title,
            excerpt,
            published_at: entry.published,
            collected_at,
        })
    }
}

fn feed_url(source: &Source) -> &str {
    match &source.kind {
        SourceKind::Feed { url } => url,
    }
}

fn is_alternate_link(link: &feed_rs::model::Link) -> bool {
    link.rel
        .as_deref()
        .is_none_or(|relation| relation.eq_ignore_ascii_case("alternate"))
}

fn usable_url(value: &str) -> Option<String> {
    let url = url::Url::parse(value.trim()).ok()?;
    matches!(url.scheme(), "http" | "https")
        .then_some(url)
        .filter(|url| url.host_str().is_some())
        .map(|url| url.to_string())
}

fn normalized_text(value: &str) -> String {
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

    plain_text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::normalized_text;

    #[test]
    fn normalized_text_removes_markup_and_collapses_whitespace() {
        assert_eq!(
            normalized_text(" <p> A  <strong>signal</strong> </p> "),
            "A signal"
        );
    }
}
