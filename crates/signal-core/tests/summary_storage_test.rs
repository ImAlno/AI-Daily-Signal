use signal_core::{
    AiSummaryFields, AttemptOutcome, BudgetDecision, GenerationFailureKind, GenerationOutcomeKind,
    GenerationStatus, ProviderKind, SignalError, Store, SummarySettings,
};

#[test]
fn forced_variants_can_share_a_cache_key_and_newest_is_selected() {
    let store = signal_core::test_support::temporary_store();
    let older = signal_core::test_support::summary_variant(
        "variant-old",
        "same-cache-key",
        signal_core::test_support::fixed_now(),
    );
    let newer = signal_core::test_support::summary_variant(
        "variant-new",
        "same-cache-key",
        signal_core::test_support::fixed_now() + chrono::Duration::seconds(1),
    );
    store.insert_summary_variant(&older).unwrap();
    store.insert_summary_variant(&newer).unwrap();
    assert_eq!(
        store
            .find_cached_summary("same-cache-key")
            .unwrap()
            .unwrap()
            .id,
        newer.id
    );
}

#[test]
fn cache_identity_changes_for_every_specified_input() {
    let fixture = signal_core::test_support::cache_identity_fixture();
    let baseline = signal_core::summary_cache_key(
        &fixture.story,
        &fixture.profile,
        &fixture.prompt_version,
        &fixture.settings,
    )
    .unwrap();
    for changed in fixture.each_single_field_changed() {
        assert_ne!(
            baseline,
            signal_core::summary_cache_key(
                &changed.story,
                &changed.profile,
                &changed.prompt_version,
                &changed.settings,
            )
            .unwrap()
        );
    }
}

#[test]
fn two_connections_cannot_reserve_past_the_daily_budget() {
    let fixture = signal_core::test_support::shared_budget_store(1_000_000);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let handles = [uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2)].map(|attempt_id| {
        let store = fixture.store.clone();
        let profile = fixture.profile.clone();
        let barrier = barrier.clone();
        let now = fixture.now;
        let expires_at = fixture.expires_at;
        std::thread::spawn(move || {
            barrier.wait();
            store
                .reserve_generation(&profile, attempt_id, now, 750_000, expires_at)
                .unwrap()
        })
    });
    barrier.wait();
    let decisions = handles.map(|handle| handle.join().unwrap());
    assert_eq!(
        decisions
            .iter()
            .filter(|value| matches!(value, &BudgetDecision::Reserved(_)))
            .count(),
        1
    );
    assert_eq!(
        decisions
            .iter()
            .filter(|value| matches!(value, &BudgetDecision::Exhausted))
            .count(),
        1
    );
}

#[test]
fn milestone_one_database_migrates_without_losing_briefings_or_saved_state() {
    let fixture = signal_core::test_support::version_two_database();
    let source_config_bytes_before = std::fs::read(&fixture.source_config_path).unwrap();
    let source_config_before: signal_core::AppConfig =
        toml::from_str(std::str::from_utf8(&source_config_bytes_before).unwrap()).unwrap();
    assert_eq!(source_config_before, fixture.source_config);

    let store = Store::open(&fixture.path).unwrap();
    let briefing = store.load_latest_briefing().unwrap().unwrap();
    assert_eq!(
        briefing.date,
        signal_core::test_support::fixed_now().date_naive()
    );
    assert_eq!(
        briefing.generated_at,
        signal_core::test_support::fixed_now()
    );
    assert_eq!(briefing.items.len(), 1);
    assert_eq!(briefing.items[0].position, 1);
    assert_eq!(briefing.items[0].section, "top_signals");
    assert!(!briefing.items[0].is_stale);
    assert_eq!(briefing.items[0].story.id, "story-1");

    let selected_story = store.find_story("story-1").unwrap().unwrap();
    assert!(selected_story.is_read);
    assert!(selected_story.is_saved);
    let story_ids = store
        .list_latest()
        .unwrap()
        .into_iter()
        .map(|story| story.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        story_ids,
        ["story-1".to_owned(), "story-2".to_owned()].into()
    );

    let refresh = store.latest_refresh_run().unwrap().unwrap();
    assert_eq!(
        refresh.started_at,
        signal_core::test_support::fixed_now() - chrono::Duration::minutes(5)
    );
    assert_eq!(
        refresh.finished_at,
        Some(signal_core::test_support::fixed_now() - chrono::Duration::minutes(4))
    );
    assert_eq!(refresh.successful_sources, 3);
    assert_eq!(refresh.failed_sources, 1);
    assert!(store.list_model_profiles().unwrap().is_empty());

    let source_config_bytes_after = std::fs::read(&fixture.source_config_path).unwrap();
    assert_eq!(source_config_bytes_after, source_config_bytes_before);
    let source_config_after: signal_core::AppConfig =
        toml::from_str(std::str::from_utf8(&source_config_bytes_after).unwrap()).unwrap();
    assert_eq!(source_config_after, fixture.source_config);
}

#[test]
fn summary_fields_reject_blank_oversized_markup_links_and_unknown_json_fields() {
    let settings = SummarySettings::default();
    let valid = AiSummaryFields {
        what_happened: "A factual event happened.".to_owned(),
        why_it_matters: "It changes the documented outcome.".to_owned(),
        caveat: Some("The timing remains uncertain.".to_owned()),
    };
    assert!(valid.validate(&settings).is_ok());

    for invalid in [
        AiSummaryFields {
            what_happened: " \n ".to_owned(),
            ..valid.clone()
        },
        AiSummaryFields {
            why_it_matters: "x".repeat(601),
            ..valid.clone()
        },
        AiSummaryFields {
            caveat: Some("  ".to_owned()),
            ..valid.clone()
        },
        AiSummaryFields {
            what_happened: "An <em>event</em>.".to_owned(),
            ..valid.clone()
        },
        AiSummaryFields {
            why_it_matters: "See [the source](https://example.com).".to_owned(),
            ..valid.clone()
        },
        AiSummaryFields {
            why_it_matters: "See [the source][reference].".to_owned(),
            ..valid.clone()
        },
    ] {
        assert!(invalid.validate(&settings).is_err());
    }

    let unknown = r#"{
        "what_happened":"An event.",
        "why_it_matters":"A consequence.",
        "caveat":null,
        "extra":"not allowed"
    }"#;
    assert!(serde_json::from_str::<AiSummaryFields>(unknown).is_err());
}

#[test]
fn cache_normalization_excludes_credentials_and_has_a_deterministic_tie_break() {
    let fixture = signal_core::test_support::cache_identity_fixture();
    let baseline = signal_core::summary_cache_key(
        &fixture.story,
        &fixture.profile,
        &fixture.prompt_version,
        &fixture.settings,
    )
    .unwrap();
    let mut equivalent = fixture.clone();
    equivalent.story.title = " A---DETERMINISTIC   signal ".to_owned();
    equivalent.story.excerpt = "A stable excerpt\nfor storage tests.".to_owned();
    equivalent.story.canonical_url =
        "https://EXAMPLE.com:443/story-1?utm_source=secret#fragment".to_owned();
    equivalent.story.source_ids = vec!["example-feed".to_owned()];
    equivalent.profile.endpoint = Some("https://PROVIDER.example:443/v1".parse().unwrap());
    equivalent.profile.credential = signal_core::CredentialRef::Environment {
        variable: "A_DIFFERENT_REFERENCE".to_owned(),
    };
    assert_eq!(
        baseline,
        signal_core::summary_cache_key(
            &equivalent.story,
            &equivalent.profile,
            &equivalent.prompt_version,
            &equivalent.settings,
        )
        .unwrap()
    );

    let store = signal_core::test_support::temporary_store();
    let generated_at = signal_core::test_support::fixed_now();
    let mut higher = signal_core::test_support::summary_variant("higher", "tie", generated_at);
    higher.id = uuid::Uuid::from_u128(2);
    let mut lower = signal_core::test_support::summary_variant("lower", "tie", generated_at);
    lower.id = uuid::Uuid::from_u128(1);
    store.insert_summary_variant(&higher).unwrap();
    store.insert_summary_variant(&lower).unwrap();
    assert_eq!(
        store.find_cached_summary("tie").unwrap().unwrap().id,
        lower.id
    );
}

#[test]
fn finalized_costs_replace_reservations_and_finalization_is_strictly_idempotent() {
    let fixture = signal_core::test_support::shared_budget_store(1_000);
    let first_id = uuid::Uuid::from_u128(10);
    assert!(matches!(
        fixture
            .store
            .reserve_generation(
                &fixture.profile,
                first_id,
                fixture.now,
                800,
                fixture.expires_at
            )
            .unwrap(),
        BudgetDecision::Reserved(_)
    ));
    let completed = AttemptOutcome::Completed {
        input_tokens: Some(12),
        output_tokens: Some(6),
        cost_microusd: 300,
    };
    let finalized_at = fixture.now + chrono::Duration::seconds(2);
    let first = fixture
        .store
        .finalize_generation(first_id, finalized_at, completed.clone())
        .unwrap();
    assert_eq!(first.status, GenerationStatus::Completed);
    assert_eq!(first.actual_cost_microusd, Some(300));
    assert_eq!(
        fixture
            .store
            .finalize_generation(first_id, finalized_at, completed)
            .unwrap(),
        first
    );
    assert!(matches!(
        fixture.store.finalize_generation(
            first_id,
            finalized_at,
            AttemptOutcome::FailedUncharged {
                category: GenerationFailureKind::Timeout,
            },
        ),
        Err(SignalError::Storage(_))
    ));

    let second_id = uuid::Uuid::from_u128(11);
    assert!(matches!(
        fixture
            .store
            .reserve_generation(
                &fixture.profile,
                second_id,
                fixture.now,
                700,
                fixture.expires_at
            )
            .unwrap(),
        BudgetDecision::Reserved(_)
    ));
    assert!(matches!(
        fixture
            .store
            .reserve_generation(
                &fixture.profile,
                uuid::Uuid::from_u128(12),
                fixture.now,
                1,
                fixture.expires_at
            )
            .unwrap(),
        BudgetDecision::Exhausted
    ));
}

#[test]
fn charged_zero_and_uncharged_failures_are_conflicting_outcomes() {
    let fixture = signal_core::test_support::shared_budget_store(1_000);
    let attempt_id = uuid::Uuid::from_u128(13);
    fixture
        .store
        .reserve_generation(
            &fixture.profile,
            attempt_id,
            fixture.now,
            100,
            fixture.expires_at,
        )
        .unwrap();
    let finalized_at = fixture.now + chrono::Duration::seconds(2);
    let charged_zero = AttemptOutcome::FailedCharged {
        category: GenerationFailureKind::Timeout,
        cost_microusd: 0,
    };
    let original = fixture
        .store
        .finalize_generation(attempt_id, finalized_at, charged_zero.clone())
        .unwrap();
    assert_eq!(
        original.final_outcome,
        Some(GenerationOutcomeKind::FailedCharged)
    );

    assert!(matches!(
        fixture.store.finalize_generation(
            attempt_id,
            finalized_at,
            AttemptOutcome::FailedUncharged {
                category: GenerationFailureKind::Timeout,
            },
        ),
        Err(SignalError::Storage(_))
    ));
    assert_eq!(
        fixture
            .store
            .finalize_generation(attempt_id, finalized_at, charged_zero)
            .unwrap(),
        original
    );
}

#[test]
fn daily_limit_outside_sqlites_integer_range_is_a_checked_error() {
    let store = signal_core::test_support::temporary_store();
    let persisted = signal_core::test_support::model_profile("wide-cap", ProviderKind::OpenAi);
    store.create_model_profile(&persisted).unwrap();
    let mut supplied = persisted.clone();
    supplied.limits.max_daily_cost_microusd = Some(u64::MAX);
    supplied.limits.input_cost_microusd_per_million = Some(1);
    supplied.limits.output_cost_microusd_per_million = Some(1);
    let attempt_id = uuid::Uuid::from_u128(14);
    let now = signal_core::test_support::fixed_now();

    assert!(matches!(
        store.reserve_generation(
            &supplied,
            attempt_id,
            now,
            1,
            now + chrono::Duration::minutes(1),
        ),
        Err(SignalError::Serialization(_))
    ));
    assert!(matches!(
        store
            .reserve_generation(
                &persisted,
                attempt_id,
                now,
                1,
                now + chrono::Duration::minutes(1),
            )
            .unwrap(),
        BudgetDecision::Reserved(_)
    ));
}

#[test]
fn finalized_attempt_rows_require_an_explicit_outcome_disposition() {
    let fixture = signal_core::test_support::version_two_database();
    let store = Store::open(&fixture.path).unwrap();
    drop(store);
    let connection = rusqlite::Connection::open(&fixture.path).unwrap();
    let now = signal_core::test_support::fixed_now().to_rfc3339();

    assert!(
        connection
            .execute(
                "INSERT INTO generation_attempts (
                     id, profile_id, provider, model, endpoint, dialect, usage_date, status,
                     final_outcome, estimated_cost_microusd, actual_cost_microusd, input_tokens,
                     output_tokens, failure_kind, reserved_at, expires_at, finalized_at
                 ) VALUES (
                     ?1, NULL, 'open_ai', 'fixture-model', NULL, NULL, '2026-08-29', 'failed',
                     NULL, 1, 0, NULL, NULL, 'timeout', ?2, ?2, ?2
                 )",
                rusqlite::params![uuid::Uuid::from_u128(15).hyphenated().to_string(), now],
            )
            .is_err()
    );
}

#[test]
fn expired_reservations_are_ignored_and_uncapped_profiles_are_still_accounted() {
    let fixture = signal_core::test_support::shared_budget_store(1_000);
    fixture
        .store
        .reserve_generation(
            &fixture.profile,
            uuid::Uuid::from_u128(20),
            fixture.now,
            900,
            fixture.expires_at,
        )
        .unwrap();
    let later = fixture.expires_at + chrono::Duration::seconds(1);
    let reservation = fixture
        .store
        .reserve_generation(
            &fixture.profile,
            uuid::Uuid::from_u128(21),
            later,
            1_000,
            later + chrono::Duration::minutes(1),
        )
        .unwrap();
    assert!(matches!(reservation, BudgetDecision::Reserved(_)));

    let store = signal_core::test_support::temporary_store();
    let profile = signal_core::test_support::model_profile("uncapped", ProviderKind::OpenAi);
    store.create_model_profile(&profile).unwrap();
    let attempt_id = uuid::Uuid::from_u128(22);
    let decision = store
        .reserve_generation(&profile, attempt_id, fixture.now, 55, fixture.expires_at)
        .unwrap();
    let BudgetDecision::Reserved(reservation) = decision else {
        panic!("an uncapped profile must still create an accounting attempt");
    };
    assert_eq!(reservation.usage_date, fixture.now.date_naive());
    let failed = store
        .finalize_generation(
            attempt_id,
            fixture.now + chrono::Duration::seconds(1),
            AttemptOutcome::FailedCharged {
                category: GenerationFailureKind::ProviderUnavailable,
                cost_microusd: 55,
            },
        )
        .unwrap();
    assert_eq!(failed.status, GenerationStatus::Failed);
    assert_eq!(failed.actual_cost_microusd, Some(55));
}

#[test]
fn deleting_a_profile_retains_variants_and_attempts_with_null_profile_references() {
    let store = signal_core::test_support::temporary_store();
    let profile = signal_core::test_support::model_profile("history", ProviderKind::OpenAi);
    store.create_model_profile(&profile).unwrap();
    let mut variant = signal_core::test_support::summary_variant(
        "history-variant",
        "history-cache",
        signal_core::test_support::fixed_now(),
    );
    variant.profile_id = Some(profile.id);
    variant.provider = profile.provider;
    variant.model.clone_from(&profile.model);
    store.insert_summary_variant(&variant).unwrap();
    let attempt_id = uuid::Uuid::from_u128(30);
    let now = signal_core::test_support::fixed_now();
    store
        .reserve_generation(
            &profile,
            attempt_id,
            now,
            5,
            now + chrono::Duration::minutes(1),
        )
        .unwrap();

    store.remove_model_profile(profile.id).unwrap();

    assert_eq!(
        store.list_summary_variants("story-1").unwrap()[0].profile_id,
        None
    );
    let attempt = store
        .finalize_generation(
            attempt_id,
            now + chrono::Duration::seconds(1),
            AttemptOutcome::FailedUncharged {
                category: GenerationFailureKind::Transport,
            },
        )
        .unwrap();
    assert_eq!(attempt.profile_id, None);
}

#[test]
fn selected_summary_round_trips_and_manual_selection_updates_only_the_newest_briefing() {
    let store = signal_core::test_support::temporary_store();
    let variant = signal_core::test_support::summary_variant(
        "selected-variant",
        "selected-cache",
        signal_core::test_support::fixed_now(),
    );
    store.insert_summary_variant(&variant).unwrap();
    let mut older = signal_core::test_support::briefing_fixture();
    older.date -= chrono::Duration::days(1);
    older.generated_at -= chrono::Duration::days(1);
    let older_stories = [older.items[0].story.clone()];
    store.commit_refresh(&older_stories, &older).unwrap();
    let newer = signal_core::test_support::briefing_fixture();
    let newer_stories = [newer.items[0].story.clone()];
    store.commit_refresh(&newer_stories, &newer).unwrap();

    store.select_story_summary("story-1", variant.id).unwrap();

    assert_eq!(
        store.load_briefing(newer.date).unwrap().unwrap().items[0].selected_summary,
        Some(variant.clone())
    );
    assert_eq!(
        store.load_briefing(older.date).unwrap().unwrap().items[0].selected_summary,
        None
    );

    let mut selected_on_write = newer;
    selected_on_write.items[0].selected_summary = Some(variant);
    store
        .commit_refresh(&newer_stories, &selected_on_write)
        .unwrap();
    assert_eq!(
        store.load_briefing(selected_on_write.date).unwrap(),
        Some(selected_on_write)
    );
}

#[test]
fn old_briefing_json_defaults_selected_summary_without_changing_existing_fields() {
    let briefing = signal_core::test_support::briefing_fixture();
    let mut json = serde_json::to_value(&briefing).unwrap();
    let item = json["items"][0].as_object_mut().unwrap();
    item.remove("selected_summary");
    let restored: signal_core::Briefing = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(restored, briefing);
    assert_eq!(json["items"][0]["position"], 1);
    assert_eq!(json["items"][0]["section"], "top_signals");
    assert_eq!(json["items"][0]["is_stale"], false);
    assert_eq!(json["items"][0]["story"]["id"], "story-1");
}

#[test]
fn failure_categories_have_stable_snake_case_values() {
    let values = [
        (
            GenerationFailureKind::CredentialMissing,
            "credential_missing",
        ),
        (GenerationFailureKind::Authentication, "authentication"),
        (GenerationFailureKind::RateLimited, "rate_limited"),
        (GenerationFailureKind::Timeout, "timeout"),
        (GenerationFailureKind::Transport, "transport"),
        (GenerationFailureKind::ProviderRejected, "provider_rejected"),
        (
            GenerationFailureKind::ProviderUnavailable,
            "provider_unavailable",
        ),
        (GenerationFailureKind::MalformedOutput, "malformed_output"),
    ];
    for (kind, expected) in values {
        assert_eq!(
            serde_json::to_string(&kind).unwrap(),
            format!("\"{expected}\"")
        );
        assert_eq!(
            serde_json::from_str::<GenerationFailureKind>(&format!("\"{expected}\"")).unwrap(),
            kind
        );
    }

    for (outcome, expected) in [
        (GenerationOutcomeKind::Completed, "completed"),
        (GenerationOutcomeKind::FailedCharged, "failed_charged"),
        (GenerationOutcomeKind::FailedUncharged, "failed_uncharged"),
    ] {
        assert_eq!(
            serde_json::to_string(&outcome).unwrap(),
            format!("\"{expected}\"")
        );
    }
}

#[test]
fn monetary_values_outside_sqlites_nonnegative_integer_range_are_rejected() {
    let store = signal_core::test_support::temporary_store();
    let mut variant = signal_core::test_support::summary_variant(
        "overflow",
        "overflow-cache",
        signal_core::test_support::fixed_now(),
    );
    variant.cost_microusd = u64::MAX;
    assert!(store.insert_summary_variant(&variant).is_err());

    let profile = signal_core::test_support::model_profile("overflow", ProviderKind::OpenAi);
    store.create_model_profile(&profile).unwrap();
    assert!(
        store
            .reserve_generation(
                &profile,
                uuid::Uuid::from_u128(40),
                signal_core::test_support::fixed_now(),
                u64::MAX,
                signal_core::test_support::fixed_now() + chrono::Duration::minutes(1),
            )
            .is_err()
    );
}

#[test]
fn migrations_store_no_secret_or_free_form_error_body_columns() {
    const SENTINEL_CREDENTIAL_VALUE: &str = "SENTINEL-CREDENTIAL-VALUE";
    let fixture = signal_core::test_support::version_two_database();
    let store = Store::open(&fixture.path).unwrap();
    let profile = signal_core::test_support::model_profile("schema", ProviderKind::OpenAi);
    store.create_model_profile(&profile).unwrap();
    let mut variant = signal_core::test_support::summary_variant(
        "schema-variant",
        "schema-cache",
        signal_core::test_support::fixed_now(),
    );
    variant.profile_id = Some(profile.id);
    store.insert_summary_variant(&variant).unwrap();
    drop(store);

    let connection = rusqlite::Connection::open(&fixture.path).unwrap();
    let schema = connection
        .query_row(
            "SELECT COALESCE(group_concat(sql, '\n'), '') FROM sqlite_schema",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    assert!(!schema.contains(SENTINEL_CREDENTIAL_VALUE));

    for table in [
        "model_profiles",
        "app_settings",
        "summary_variants",
        "generation_attempts",
        "briefing_items",
    ] {
        let mut columns = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        let text_columns = columns
            .query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .filter(|(_, kind)| kind.eq_ignore_ascii_case("TEXT"))
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        drop(columns);
        for column in text_columns {
            let contains_sentinel = connection
                .query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM {table} WHERE {column} LIKE '%' || ?1 || '%')"
                    ),
                    [SENTINEL_CREDENTIAL_VALUE],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap();
            assert!(!contains_sentinel, "sentinel found in {table}.{column}");
        }
    }

    let attempt_columns = connection
        .prepare("PRAGMA table_info(generation_attempts)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert!(!attempt_columns.iter().any(|column| {
        column.contains("body") || column.contains("message") || column.contains("details")
    }));
}

#[test]
fn summary_content_is_immutable_cost_checks_are_defensive_and_story_deletion_cascades() {
    let fixture = signal_core::test_support::version_two_database();
    let store = Store::open(&fixture.path).unwrap();
    let variant = signal_core::test_support::summary_variant(
        "immutable",
        "immutable-cache",
        signal_core::test_support::fixed_now(),
    );
    store.insert_summary_variant(&variant).unwrap();
    let story = signal_core::test_support::story_fixture("story-2");
    store.upsert_stories(std::slice::from_ref(&story)).unwrap();
    let mut cascading = signal_core::test_support::summary_variant(
        "cascading",
        "cascading-cache",
        signal_core::test_support::fixed_now(),
    );
    cascading.story_id.clone_from(&story.id);
    store.insert_summary_variant(&cascading).unwrap();
    drop(store);

    let connection = rusqlite::Connection::open(&fixture.path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", true)
        .unwrap();
    assert!(
        connection
            .execute(
                "UPDATE summary_variants SET what_happened = 'mutated' WHERE id = ?1",
                [variant.id.hyphenated().to_string()],
            )
            .is_err()
    );
    assert!(
        connection
            .execute(
                "INSERT INTO summary_variants (
                     id, story_id, profile_id, provider, model, endpoint, dialect, prompt_version,
                     cache_key, what_happened, why_it_matters, caveat, input_tokens,
                     output_tokens, cost_microusd, generated_at
                 ) SELECT
                     ?1, story_id, profile_id, provider, model, endpoint, dialect, prompt_version,
                     'negative-cost', what_happened, why_it_matters, caveat, input_tokens,
                     output_tokens, -1, generated_at
                 FROM summary_variants WHERE id = ?2",
                rusqlite::params![
                    uuid::Uuid::from_u128(50).hyphenated().to_string(),
                    variant.id.hyphenated().to_string(),
                ],
            )
            .is_err()
    );
    connection
        .execute("DELETE FROM stories WHERE id = ?1", [&story.id])
        .unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM summary_variants WHERE id = ?1",
                [cascading.id.hyphenated().to_string()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}
