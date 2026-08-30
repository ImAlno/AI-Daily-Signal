#[test]
fn opens_in_wal_mode_and_applies_the_initial_migration() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let status = store.status().unwrap();
    assert_eq!(status.story_count, 0);
    assert_eq!(store.journal_mode().unwrap(), "wal");
}

#[test]
fn committing_a_refresh_is_atomic_and_round_trips() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let briefing = signal_core::test_support::briefing_fixture();
    let stories = briefing
        .items
        .iter()
        .map(|item| item.story.clone())
        .collect::<Vec<_>>();
    store.commit_refresh(&stories, &briefing).unwrap();
    assert_eq!(store.load_briefing(briefing.date).unwrap(), Some(briefing));
}

#[test]
fn saved_state_survives_story_upsert() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let story = signal_core::test_support::story_fixture("story-1");
    store.upsert_stories(std::slice::from_ref(&story)).unwrap();
    store.set_saved("story-1", true).unwrap();
    store.upsert_stories(&[story]).unwrap();
    assert!(store.find_story("story-1").unwrap().unwrap().is_saved);
}

#[test]
fn a_failed_refresh_rolls_back_stories_and_generation() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let briefing = signal_core::test_support::briefing_fixture();
    let unselected_story = signal_core::test_support::story_fixture("story-2");

    assert!(
        store
            .commit_refresh(&[unselected_story], &briefing)
            .is_err()
    );

    let status = store.status().unwrap();
    assert_eq!(status.story_count, 0);
    assert_eq!(status.data_generation, 0);
    assert!(store.find_story("story-2").unwrap().is_none());
}

#[test]
fn latest_includes_stories_not_selected_for_the_briefing() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let briefing = signal_core::test_support::briefing_fixture();
    let mut unselected_story = signal_core::test_support::story_fixture("story-2");
    unselected_story.title = "An unselected signal".to_owned();
    let stories = [briefing.items[0].story.clone(), unselected_story.clone()];

    store.commit_refresh(&stories, &briefing).unwrap();

    let latest = store.list_latest().unwrap();
    assert_eq!(latest.len(), 2);
    assert!(latest.contains(&unselected_story));
    let status = store.status().unwrap();
    assert_eq!(status.story_count, 2);
    assert_eq!(status.last_refresh_at, Some(briefing.generated_at));
    assert_eq!(status.data_generation, 1);
}

#[test]
fn saved_stories_are_listed_and_can_be_removed() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let story = signal_core::test_support::story_fixture("story-1");
    store.upsert_stories(std::slice::from_ref(&story)).unwrap();

    store.set_saved(&story.id, true).unwrap();
    assert_eq!(store.list_saved().unwrap().len(), 1);

    store.set_saved(&story.id, false).unwrap();
    assert!(store.list_saved().unwrap().is_empty());
}

#[test]
fn user_visible_database_mutations_increment_data_generation() {
    let store = signal_core::test_support::temporary_store();
    let briefing = signal_core::test_support::briefing_fixture();
    store
        .commit_refresh(
            &briefing
                .items
                .iter()
                .map(|item| item.story.clone())
                .collect::<Vec<_>>(),
            &briefing,
        )
        .unwrap();
    let variant = signal_core::test_support::summary_variant(
        "generation-test-variant",
        "generation-test-cache-key",
        signal_core::test_support::fixed_now(),
    );
    store.insert_summary_variant(&variant).unwrap();
    let profile = signal_core::test_support::model_profile(
        "generation-test-profile",
        signal_core::ProviderKind::OpenAi,
    );

    let before = store.status().unwrap().data_generation;
    store.set_saved("story-1", true).unwrap();
    assert_eq!(store.status().unwrap().data_generation, before + 1);

    store.set_read("story-1", true).unwrap();
    assert_eq!(store.status().unwrap().data_generation, before + 2);

    store.select_story_summary("story-1", variant.id).unwrap();
    assert_eq!(store.status().unwrap().data_generation, before + 3);

    store.create_model_profile(&profile).unwrap();
    assert_eq!(store.status().unwrap().data_generation, before + 4);

    store.set_default_model_profile(Some(profile.id)).unwrap();
    assert_eq!(store.status().unwrap().data_generation, before + 5);

    store.remove_model_profile(profile.id).unwrap();
    assert_eq!(store.status().unwrap().data_generation, before + 6);
}

#[test]
fn failed_story_mutations_do_not_increment_data_generation() {
    let store = signal_core::test_support::temporary_store();
    let before = store.status().unwrap().data_generation;

    assert!(store.set_saved("missing-story", true).is_err());
    assert_eq!(store.status().unwrap().data_generation, before);

    assert!(store.set_read("missing-story", true).is_err());
    assert_eq!(store.status().unwrap().data_generation, before);
}

#[test]
fn inserting_summary_variant_increments_data_generation_once() {
    let store = signal_core::test_support::temporary_store();
    let variant = signal_core::test_support::summary_variant(
        "visible-variant",
        "visible-variant-cache-key",
        signal_core::test_support::fixed_now(),
    );
    let before = store.status().unwrap().data_generation;

    store.insert_summary_variant(&variant).unwrap();

    assert_eq!(store.status().unwrap().data_generation, before + 1);
}

#[test]
fn counted_refresh_commit_persists_source_counts() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let briefing = signal_core::test_support::briefing_fixture();
    let stories = briefing
        .items
        .iter()
        .map(|item| item.story.clone())
        .collect::<Vec<_>>();

    store
        .commit_refresh_with_counts(&stories, &briefing, 2, 1)
        .unwrap();

    let run = store.latest_refresh_run().unwrap().unwrap();
    assert_eq!(run.started_at, briefing.generated_at);
    assert_eq!(run.finished_at, Some(briefing.generated_at));
    assert_eq!(run.successful_sources, 2);
    assert_eq!(run.failed_sources, 1);
}

#[test]
fn failed_refresh_run_does_not_change_the_cached_generation() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let briefing = signal_core::test_support::briefing_fixture();
    let stories = briefing
        .items
        .iter()
        .map(|item| item.story.clone())
        .collect::<Vec<_>>();
    store.commit_refresh(&stories, &briefing).unwrap();
    let before = store.status().unwrap();

    store
        .record_refresh_failure(briefing.generated_at + chrono::Duration::minutes(5), 3)
        .unwrap();

    let after = store.status().unwrap();
    assert_eq!(after.data_generation, before.data_generation);
    assert_eq!(after.last_refresh_at, before.last_refresh_at);
    assert_eq!(store.load_briefing(briefing.date).unwrap(), Some(briefing));
    let run = store.latest_refresh_run().unwrap().unwrap();
    assert_eq!(run.successful_sources, 0);
    assert_eq!(run.failed_sources, 3);
}

#[test]
fn refresh_bookkeeping_failure_rolls_back_the_briefing_commit() {
    let temp = tempfile::tempdir().unwrap();
    let database_path = temp.path().join("signal.sqlite3");
    let store = signal_core::Store::open(&database_path).unwrap();
    rusqlite::Connection::open(&database_path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_refresh_run
             BEFORE INSERT ON refresh_runs
             BEGIN
                 SELECT RAISE(ABORT, 'fixture bookkeeping failure');
             END;",
        )
        .unwrap();
    let briefing = signal_core::test_support::briefing_fixture();
    let stories = briefing
        .items
        .iter()
        .map(|item| item.story.clone())
        .collect::<Vec<_>>();

    assert!(
        store
            .commit_refresh_with_counts(&stories, &briefing, 1, 0)
            .is_err()
    );

    assert_eq!(store.status().unwrap().data_generation, 0);
    assert!(store.load_briefing(briefing.date).unwrap().is_none());
    assert!(store.find_story("story-1").unwrap().is_none());
    assert!(store.latest_refresh_run().unwrap().is_none());
}

#[test]
fn latest_briefing_uses_newest_date_and_round_trips_item_staleness() {
    let temp = tempfile::tempdir().unwrap();
    let store = signal_core::Store::open(temp.path().join("signal.sqlite3")).unwrap();
    let mut older = signal_core::test_support::briefing_fixture();
    older.date -= chrono::Duration::days(1);
    older.generated_at -= chrono::Duration::days(1);
    let mut latest = signal_core::test_support::briefing_fixture();
    latest.items[0].is_stale = true;
    let latest_stories = latest
        .items
        .iter()
        .map(|item| item.story.clone())
        .collect::<Vec<_>>();
    let older_stories = older
        .items
        .iter()
        .map(|item| item.story.clone())
        .collect::<Vec<_>>();
    store.commit_refresh(&latest_stories, &latest).unwrap();
    store.commit_refresh(&older_stories, &older).unwrap();

    let loaded = store.load_latest_briefing().unwrap().unwrap();

    assert_eq!(loaded.date, latest.date);
    assert!(loaded.items[0].is_stale);
}

#[test]
fn opening_a_version_one_database_adds_persisted_item_staleness() {
    let temp = tempfile::tempdir().unwrap();
    let database_path = temp.path().join("signal.sqlite3");
    let connection = rusqlite::Connection::open(&database_path).unwrap();
    connection
        .execute_batch(include_str!("../migrations/001_initial.sql"))
        .unwrap();
    connection
        .execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (1, '2026-08-29T00:00:00Z')",
            [],
        )
        .unwrap();
    drop(connection);

    let store = signal_core::Store::open(&database_path).unwrap();
    let mut briefing = signal_core::test_support::briefing_fixture();
    briefing.items[0].is_stale = true;
    let stories = briefing
        .items
        .iter()
        .map(|item| item.story.clone())
        .collect::<Vec<_>>();
    store.commit_refresh(&stories, &briefing).unwrap();

    assert!(store.load_latest_briefing().unwrap().unwrap().items[0].is_stale);
}
