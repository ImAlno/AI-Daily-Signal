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

#[test]
fn title_similarity_chain_never_merges_nonmatching_endpoints() {
    let now = signal_core::test_support::fixed_now();
    let mut first = signal_core::test_support::candidate_fixture("chain-a");
    first.source_id = "primary".to_owned();
    first.title = "a b c d e f g h i j".to_owned();
    first.canonical_url = "https://example.com/chain-a".to_owned();

    let mut middle = signal_core::test_support::candidate_fixture("chain-b");
    middle.source_id = "syndicated".to_owned();
    middle.title = "a b c d e f g h i j k".to_owned();
    middle.canonical_url = "https://example.com/chain-b".to_owned();

    let mut last = signal_core::test_support::candidate_fixture("chain-c");
    last.source_id = "official".to_owned();
    last.title = "a b c d e f g h i j k l".to_owned();
    last.canonical_url = "https://example.com/chain-c".to_owned();

    let output = signal_core::Pipeline::build(
        vec![first, middle, last],
        &signal_core::test_support::config_fixture(),
        now,
    );

    assert_eq!(output.stories.len(), 2);
    assert!(
        output
            .stories
            .iter()
            .all(|story| story.source_ids.len() < 3)
    );
    assert!(output.stories.iter().all(|story| {
        !(story.source_ids.contains(&"primary".to_owned())
            && story.source_ids.contains(&"official".to_owned()))
    }));
}

#[test]
fn canonical_url_duplicates_merge_even_when_titles_and_dates_differ() {
    let now = signal_core::test_support::fixed_now();
    let mut first = signal_core::test_support::candidate_fixture("canonical-a");
    first.title = "Original announcement".to_owned();
    first.canonical_url = "https://example.com/same".to_owned();

    let mut duplicate = signal_core::test_support::candidate_fixture("canonical-b");
    duplicate.source_id = "official".to_owned();
    duplicate.title = "Completely unrelated syndication title".to_owned();
    duplicate.canonical_url = "https://example.com/same#fragment".to_owned();
    duplicate.published_at = Some(now - Duration::days(10));

    let output = signal_core::Pipeline::build(
        vec![first, duplicate],
        &signal_core::test_support::config_fixture(),
        now,
    );

    assert_eq!(output.stories.len(), 1);
    assert_eq!(output.stories[0].source_ids, vec!["official", "primary"]);
}

#[test]
fn smart_summary_falls_back_to_title_when_a_short_excerpt_has_no_complete_sentence() {
    assert_eq!(
        signal_core::smart_summary("Unfinished source text", "Fallback title"),
        "Fallback title"
    );
}

#[test]
fn smart_summary_falls_back_to_title_when_an_over_limit_excerpt_has_no_complete_sentence() {
    assert_eq!(
        signal_core::smart_summary(&"x".repeat(361), "Fallback title"),
        "Fallback title"
    );
}

#[test]
fn future_dated_stories_cap_the_recency_component_at_sixty() {
    let now = signal_core::test_support::fixed_now();
    let mut story = signal_core::test_support::story_fixture("future");
    story.source_ids = vec!["primary".to_owned()];
    story.published_at = Some(now + Duration::hours(2));

    let score = signal_core::score_story(&story, &signal_core::test_support::config_fixture(), now);

    assert_eq!(score.recency, 60.0);
    assert!((0.0..=100.0).contains(&score.total));
}

#[test]
fn output_is_identical_for_reversed_candidates_and_uses_the_supplied_timestamp() {
    let config = signal_core::test_support::config_fixture();
    let now = signal_core::test_support::fixed_now();
    let candidates = signal_core::test_support::duplicate_candidates();
    let normal = signal_core::Pipeline::build(candidates.clone(), &config, now);
    let reversed =
        signal_core::Pipeline::build(candidates.into_iter().rev().collect(), &config, now);

    assert_eq!(normal, reversed);
    assert_eq!(normal.briefing.date, now.date_naive());
    assert_eq!(normal.briefing.generated_at, now);
    assert_eq!(
        normal.stories[0].id,
        "04fbec20771ca513f59ad0c30b5e5650b7784a9e3d6668c32a14d638f1d562ad"
    );
}

#[test]
fn equal_score_and_timestamp_items_are_ordered_by_story_id() {
    let now = signal_core::test_support::fixed_now();
    let first = signal_core::test_support::candidate_fixture("first-tie");
    let second = signal_core::test_support::candidate_fixture("second-tie");

    let output = signal_core::Pipeline::build(
        vec![second, first],
        &signal_core::test_support::config_fixture(),
        now,
    );

    assert_eq!(output.briefing.items.len(), 2);
    assert!(output.briefing.items[0].story.id < output.briefing.items[1].story.id);
    assert_eq!(output.briefing.items[0].position, 1);
    assert_eq!(output.briefing.items[1].position, 2);
}

#[test]
fn score_components_clamp_source_weight_and_cap_corroboration() {
    let now = signal_core::test_support::fixed_now();
    let mut config = signal_core::test_support::config_fixture();
    let mut story = signal_core::test_support::story_fixture("bounded-score");
    story.published_at = None;
    story.source_ids = vec!["primary".to_owned()];

    config.sources[0].weight = -0.5;
    assert_eq!(
        signal_core::score_story(&story, &config, now).source_weight,
        0.0
    );

    config.sources[0].weight = 1.5;
    assert_eq!(
        signal_core::score_story(&story, &config, now).source_weight,
        30.0
    );

    for source in &mut config.sources {
        source.weight = 0.0;
    }
    for (source_ids, expected_corroboration) in [
        (vec!["primary"], 0.0),
        (vec!["primary", "syndicated"], 5.0),
        (vec!["primary", "syndicated", "official"], 10.0),
        (vec!["primary", "syndicated", "official", "low"], 10.0),
    ] {
        story.source_ids = source_ids.into_iter().map(str::to_owned).collect();
        assert_eq!(
            signal_core::score_story(&story, &config, now).corroboration,
            expected_corroboration
        );
    }
}
