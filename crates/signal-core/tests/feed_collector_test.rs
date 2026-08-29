use chrono::{DateTime, Utc};

fn collected_at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-29T08:00:00Z")
        .unwrap()
        .to_utc()
}

#[test]
fn parses_rss_into_normalized_candidates() {
    let source = signal_core::test_support::feed_source("fixture");
    let bytes = include_bytes!("../../../tests/fixtures/sample-rss.xml");
    let collected_at = collected_at();

    let items = signal_core::FeedCollector::parse(&source, bytes, collected_at).unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].source_id, "fixture");
    assert!(items[0].excerpt.chars().all(|character| character != '<'));
    assert_eq!(items[0].excerpt, "A useful signal report.");
    assert_eq!(items[0].collected_at, collected_at);
    assert_eq!(
        items[0].canonical_url,
        "https://example.com/stories/signal-report?utm_source=rss"
    );
    assert_eq!(
        items[0].published_at,
        Some(
            DateTime::parse_from_rfc3339("2026-08-29T07:30:00Z")
                .unwrap()
                .to_utc()
        )
    );
}

#[test]
fn parses_atom_entries_using_alternate_links() {
    let source = signal_core::test_support::feed_source("fixture");
    let bytes = include_bytes!("../../../tests/fixtures/sample-atom.xml");

    let items = signal_core::FeedCollector::parse(&source, bytes, collected_at()).unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].canonical_url,
        "https://example.com/stories/signal-report?utm_source=atom"
    );
    assert_eq!(items[0].external_id, "atom-shared-id");
    assert_eq!(items[0].excerpt, "A useful signal report.");
}

#[test]
fn parses_json_feed_entries() {
    let source = signal_core::test_support::feed_source("json-fixture");
    let bytes = br#"{
        "version": "https://jsonfeed.org/version/1.1",
        "title": "Fixture JSON Feed",
        "items": [{
            "id": "json-entry-id",
            "url": "https://example.com/stories/json",
            "title": "JSON Feed Story",
            "content_html": "<p>A <strong>sanitized</strong> JSON entry.</p>",
            "date_published": "2026-08-29T07:00:00Z"
        }]
    }"#;

    let items = signal_core::FeedCollector::parse(&source, bytes, collected_at()).unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].external_id, "json-entry-id");
    assert_eq!(items[0].canonical_url, "https://example.com/stories/json");
    assert_eq!(items[0].excerpt, "A sanitized JSON entry.");
}

#[test]
fn skips_entries_without_a_title_or_usable_url() {
    let source = signal_core::test_support::feed_source("incomplete");
    let bytes = br#"
        <rss version="2.0"><channel><title>Fixture</title>
          <item><guid>missing-title</guid><link>https://example.com/no-title</link></item>
          <item><guid>missing-url</guid><title>No URL</title></item>
          <item><guid>not-a-url</guid><title>Not a URL</title></item>
        </channel></rss>
    "#;

    let items = signal_core::FeedCollector::parse(&source, bytes, collected_at()).unwrap();

    assert!(items.is_empty());
}

#[test]
fn malformed_feed_returns_a_typed_error() {
    let source = signal_core::test_support::feed_source("broken");
    let bytes = include_bytes!("../../../tests/fixtures/malformed-feed.xml");

    let error = signal_core::FeedCollector::parse(&source, bytes, Utc::now()).unwrap_err();

    assert!(matches!(error, signal_core::SignalError::Feed(_)));
}

#[tokio::test]
async fn collection_skips_disabled_sources_without_requesting_them() {
    let mut source = signal_core::test_support::feed_source("disabled");
    source.enabled = false;
    let collector = signal_core::FeedCollector::new().unwrap();

    let report = collector.collect_all(&[source], collected_at()).await;

    assert!(report.candidates.is_empty());
    assert!(report.successful_source_ids.is_empty());
    assert!(report.failures.is_empty());
}
