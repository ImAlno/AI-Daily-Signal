use chrono::Duration;

#[test]
fn canonical_url_duplicates_become_one_story_with_two_sources() {
    let candidates = signal_core::test_support::duplicate_candidates();
    let output = signal_core::Pipeline::build(
        candidates,
        &signal_core::test_support::config_fixture(),
        signal_core::test_support::fixed_now(),
    );

    assert_eq!(output.stories.len(), 1);
    assert_eq!(output.briefing.items.len(), 1);
    assert_eq!(output.briefing.items[0].story.source_ids.len(), 2);
    assert_eq!(
        output.stories[0].canonical_url,
        "https://example.com/releases/1?topic=ai"
    );
}

#[test]
fn newer_authoritative_story_ranks_above_old_low_weight_story() {
    let output = signal_core::Pipeline::build(
        signal_core::test_support::ranked_candidates(),
        &signal_core::test_support::config_fixture(),
        signal_core::test_support::fixed_now(),
    );

    assert_eq!(output.briefing.items[0].story.title, "New official release");
}

#[test]
fn briefing_never_pads_past_available_quality_items() {
    let config = signal_core::test_support::config_with_max_items(7);
    let output = signal_core::Pipeline::build(
        vec![signal_core::test_support::candidate_fixture("only")],
        &config,
        signal_core::test_support::fixed_now(),
    );

    assert_eq!(output.briefing.items.len(), 1);
}

#[test]
fn title_duplicates_within_two_days_merge_and_keep_a_sorted_source_list() {
    let now = signal_core::test_support::fixed_now();
    let mut first = signal_core::test_support::candidate_fixture("first");
    first.source_id = "syndicated".to_owned();
    first.title = "A Major AI Release!".to_owned();
    first.canonical_url = "https://example.com/first".to_owned();

    let mut second = signal_core::test_support::candidate_fixture("second");
    second.source_id = "primary".to_owned();
    second.title = "a major ai release".to_owned();
    second.canonical_url = "https://another.example/second".to_owned();
    second.published_at = Some(now - Duration::hours(47));

    let output = signal_core::Pipeline::build(
        vec![first, second],
        &signal_core::test_support::config_fixture(),
        now,
    );

    assert_eq!(output.stories.len(), 1);
    assert_eq!(
        output.stories[0].source_ids,
        vec!["primary".to_owned(), "syndicated".to_owned()]
    );
}

#[test]
fn stale_stories_remain_in_latest_but_not_the_daily_briefing() {
    let now = signal_core::test_support::fixed_now();
    let mut candidate = signal_core::test_support::candidate_fixture("archive");
    candidate.published_at = Some(now - Duration::days(8));

    let output = signal_core::Pipeline::build(
        vec![candidate],
        &signal_core::test_support::config_fixture(),
        now,
    );

    assert_eq!(output.stories.len(), 1);
    assert!(output.briefing.items.is_empty());
}

#[test]
fn smart_summary_decodes_entities_and_stops_at_a_complete_sentence_boundary() {
    let excerpt = format!(
        "First &amp; verified sentence. Second sentence. {}",
        "x".repeat(360)
    );

    assert_eq!(
        signal_core::smart_summary(&excerpt, "Fallback title"),
        "First & verified sentence. Second sentence."
    );
}

#[test]
fn smart_summary_uses_the_title_when_the_excerpt_is_empty() {
    assert_eq!(
        signal_core::smart_summary("  \n\t", "Fallback title"),
        "Fallback title"
    );
}
