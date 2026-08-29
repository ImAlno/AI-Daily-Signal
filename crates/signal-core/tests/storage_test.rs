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
