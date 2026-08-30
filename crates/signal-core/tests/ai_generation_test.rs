use chrono::Duration;
use secrecy::SecretString;
use signal_core::{
    AddModelCredential, AddModelInput, CredentialStore, CredentialWarningKind,
    GenerationFailureKind, GenerationOutcomeKind, ManualGenerationStatus, ProfileLimits,
    ProviderFailureKind, ProviderKind, RefreshOptions, RequestChargeStatus, SummarizeOptions,
    TestModelOptions,
};

#[tokio::test]
async fn ranking_finishes_before_only_selected_items_are_generated() {
    let fixture = signal_core::test_support::ai_app_fixture().with_max_items(1);

    let report = fixture
        .app
        .refresh(fixture.now)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "refresh failed: {error}; fixture feed stats: {:?}",
                fixture.feed_server_stats()
            )
        });

    assert_eq!(report.briefing.items.len(), 1);
    assert_eq!(
        fixture.provider.requested_story_ids(),
        vec![report.briefing.items[0].story.id.clone()]
    );
}

#[tokio::test]
async fn refresh_revision_counts_generated_variant_and_refresh_commit() {
    let fixture = signal_core::test_support::ai_app_fixture().with_max_items(1);
    let before = fixture.app.status().unwrap().data_generation;

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert_eq!(report.generation.generated, 1);
    assert_eq!(fixture.app.status().unwrap().data_generation, before + 2,);
}

#[tokio::test]
async fn cache_hits_make_no_request_and_do_not_consume_refresh_cap() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(2)
        .with_refresh_cap(1)
        .with_cached_story_at(0);

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert_eq!(fixture.provider.request_count(), 1);
    assert_eq!(report.generation.cache_hits, 1);
    assert_eq!(report.generation.generated, 1);
    assert_eq!(report.generation.skipped_cap, 0);
    assert!(
        report
            .briefing
            .items
            .iter()
            .all(|item| item.selected_summary.is_some())
    );
}

#[tokio::test]
async fn provider_failure_keeps_smart_and_refresh_succeeds() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_provider_failure(ProviderFailureKind::RateLimited);

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert!(report.briefing.items[0].selected_summary.is_none());
    assert_eq!(report.generation.provider_failures, 1);
    assert_eq!(report.generation.smart_fallbacks, 1);
    let attempt = fixture
        .store()
        .list_generation_attempts()
        .unwrap()
        .remove(0);
    assert_eq!(
        attempt.final_outcome,
        Some(GenerationOutcomeKind::FailedCharged)
    );
    assert_eq!(
        attempt.actual_cost_microusd,
        Some(attempt.estimated_cost_microusd)
    );
}

#[tokio::test]
async fn not_sent_provider_failure_is_finalized_uncharged() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_provider_failure_status(ProviderFailureKind::Transport, RequestChargeStatus::NotSent);

    fixture.app.refresh(fixture.now).await.unwrap();

    let attempt = fixture
        .store()
        .list_generation_attempts()
        .unwrap()
        .remove(0);
    assert_eq!(
        attempt.final_outcome,
        Some(GenerationOutcomeKind::FailedUncharged)
    );
    assert_eq!(attempt.actual_cost_microusd, Some(0));
}

#[tokio::test]
async fn not_sent_failure_does_not_consume_the_outbound_refresh_cap() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(2)
        .with_refresh_cap(1)
        .with_provider_failure_then_success(
            ProviderFailureKind::Transport,
            RequestChargeStatus::NotSent,
        );

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert_eq!(fixture.provider.request_count(), 2);
    assert_eq!(report.generation.provider_failures, 1);
    assert_eq!(report.generation.generated, 1);
    assert_eq!(report.generation.skipped_cap, 0);
    assert!(report.briefing.items[0].selected_summary.is_none());
    assert!(report.briefing.items[1].selected_summary.is_some());
}

#[tokio::test]
async fn possibly_sent_failure_consumes_the_outbound_refresh_cap() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(2)
        .with_refresh_cap(1)
        .with_provider_failure_then_success(
            ProviderFailureKind::Timeout,
            RequestChargeStatus::PossiblySent,
        );

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert_eq!(fixture.provider.request_count(), 1);
    assert_eq!(report.generation.provider_failures, 1);
    assert_eq!(report.generation.generated, 0);
    assert_eq!(report.generation.skipped_cap, 1);
    assert!(
        report
            .briefing
            .items
            .iter()
            .all(|item| item.selected_summary.is_none())
    );
}

#[tokio::test]
async fn budget_exhaustion_stops_calls_in_briefing_order() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(3)
        .with_budget_for_one_request();

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert_eq!(fixture.provider.request_count(), 1);
    assert_eq!(report.generation.generated, 1);
    assert_eq!(
        report.generation.skipped_budget,
        report.briefing.items.len() - 1
    );
    assert_eq!(
        fixture.provider.requested_story_ids(),
        vec![report.briefing.items[0].story.id.clone()]
    );
}

#[tokio::test]
async fn no_ai_option_and_missing_default_make_no_provider_request() {
    let disabled = signal_core::test_support::ai_app_fixture();
    let disabled_report = disabled
        .app
        .refresh_with_options(disabled.now, RefreshOptions { ai: false })
        .await
        .unwrap_or_else(|error| {
            panic!(
                "refresh failed: {error}; fixture feed stats: {:?}",
                disabled.feed_server_stats()
            )
        });
    assert_eq!(disabled.provider.request_count(), 0);
    assert!(disabled_report.briefing.items[0].selected_summary.is_none());

    let no_default = signal_core::test_support::ai_app_fixture().without_default_profile();
    no_default
        .app
        .refresh(no_default.now)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "refresh failed: {error}; fixture feed stats: {:?}",
                no_default.feed_server_stats()
            )
        });
    assert_eq!(no_default.provider.request_count(), 0);
}

#[tokio::test]
async fn carried_selected_summary_is_cleared_when_ai_is_disabled() {
    let fixture = signal_core::test_support::ai_app_fixture().with_carried_selected_summary();

    let report = fixture
        .app
        .refresh_with_options(fixture.now, RefreshOptions { ai: false })
        .await
        .unwrap();

    let carried = report
        .briefing
        .items
        .iter()
        .find(|item| item.story.id == "carried-failed-story")
        .expect("stale story should be carried");
    assert!(carried.is_stale);
    assert!(carried.selected_summary.is_none());
}

#[tokio::test]
async fn carried_selected_summary_is_cleared_when_credentials_are_unavailable() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_carried_selected_summary()
        .without_credential();

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    let carried = report
        .briefing
        .items
        .iter()
        .find(|item| item.story.id == "carried-failed-story")
        .expect("stale story should be carried");
    assert!(carried.is_stale);
    assert!(carried.selected_summary.is_none());
    assert_eq!(fixture.provider.request_count(), 0);
}

#[tokio::test]
async fn carried_selected_summary_is_reselected_only_from_a_usable_cache() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_carried_selected_cache()
        .without_credential();

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    let carried = report
        .briefing
        .items
        .iter()
        .find(|item| item.story.id == "carried-failed-story")
        .expect("stale story should be carried");
    assert!(carried.is_stale);
    assert!(carried.selected_summary.is_some());
    assert_eq!(report.generation.cache_hits, 1);
    assert_eq!(fixture.provider.request_count(), 0);
}

#[tokio::test]
async fn missing_consent_and_missing_or_empty_credentials_keep_smart() {
    let no_consent = signal_core::test_support::ai_app_fixture().without_consent();
    let report = no_consent.app.refresh(no_consent.now).await.unwrap();
    assert_eq!(no_consent.provider.request_count(), 0);
    assert_eq!(
        report.generation.smart_fallbacks,
        report.briefing.items.len()
    );

    let missing = signal_core::test_support::ai_app_fixture().without_credential();
    let report = missing.app.refresh(missing.now).await.unwrap();
    assert_eq!(missing.provider.request_count(), 0);
    assert_eq!(
        report.generation.missing_credentials,
        report.briefing.items.len()
    );

    let empty = signal_core::test_support::ai_app_fixture().with_empty_environment_credential();
    let report = empty.app.refresh(empty.now).await.unwrap();
    assert_eq!(empty.provider.request_count(), 0);
    assert_eq!(
        report.generation.missing_credentials,
        report.briefing.items.len()
    );
}

#[tokio::test]
async fn empty_and_whitespace_system_credentials_make_no_provider_request() {
    for value in ["", " \t\n"] {
        let fixture = signal_core::test_support::ai_app_fixture().with_max_items(1);
        let profile = fixture.profile();
        fixture
            .credential_store
            .set(&profile.credential, SecretString::from(value.to_owned()))
            .unwrap();

        let report = fixture.app.refresh(fixture.now).await.unwrap();

        assert_eq!(fixture.provider.request_count(), 0);
        assert_eq!(report.generation.missing_credentials, 1);
        assert!(report.briefing.items[0].selected_summary.is_none());
    }
}

#[tokio::test]
async fn malformed_success_is_charged_and_counted_without_storing_a_variant() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_malformed_output();

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert_eq!(report.generation.malformed_outputs, 1);
    assert_eq!(report.generation.provider_failures, 0);
    assert!(report.briefing.items[0].selected_summary.is_none());
    assert!(
        fixture
            .store()
            .list_summary_variants(&report.briefing.items[0].story.id)
            .unwrap()
            .is_empty()
    );
    let attempt = fixture
        .store()
        .list_generation_attempts()
        .unwrap()
        .remove(0);
    assert_eq!(
        attempt.final_outcome,
        Some(GenerationOutcomeKind::FailedCharged)
    );
    assert_eq!(
        attempt.failure_kind,
        Some(GenerationFailureKind::MalformedOutput)
    );
    assert_eq!(
        attempt.actual_cost_microusd,
        Some(attempt.estimated_cost_microusd)
    );
}

#[tokio::test]
async fn provider_invalid_profile_keeps_smart_without_starting_an_attempt() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_provider_invalid_gemini_model();

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert_eq!(report.generation.provider_failures, 1);
    assert_eq!(report.generation.smart_fallbacks, 1);
    assert!(report.briefing.items[0].selected_summary.is_none());
    assert_eq!(fixture.provider.request_count(), 0);
    assert!(
        fixture
            .store()
            .list_generation_attempts()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn conservative_cost_overflow_skips_budget_without_failing_refresh() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_cost_overflow();

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert_eq!(report.generation.skipped_budget, 1);
    assert_eq!(report.generation.smart_fallbacks, 1);
    assert_eq!(fixture.provider.request_count(), 0);
    assert!(
        fixture
            .store()
            .list_generation_attempts()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn sqlite_cost_range_overflow_skips_budget_without_failing_refresh() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_sqlite_cost_range_overflow();

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert_eq!(report.generation.skipped_budget, 1);
    assert_eq!(fixture.provider.request_count(), 0);
    assert!(
        fixture
            .store()
            .list_generation_attempts()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn unpersistable_reported_usage_is_malformed_and_conservatively_charged() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_unpersistable_reported_usage();

    let report = fixture.app.refresh(fixture.now).await.unwrap();

    assert_eq!(report.generation.malformed_outputs, 1);
    let attempt = fixture
        .store()
        .list_generation_attempts()
        .unwrap()
        .remove(0);
    assert_eq!(
        attempt.final_outcome,
        Some(GenerationOutcomeKind::FailedCharged)
    );
    assert_eq!(
        attempt.actual_cost_microusd,
        Some(attempt.estimated_cost_microusd)
    );
}

#[tokio::test]
async fn reported_usage_replaces_estimate_and_unreported_usage_keeps_it() {
    let reported = signal_core::test_support::ai_app_fixture().with_max_items(1);
    reported
        .app
        .refresh(reported.now)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "reported refresh failed: {error}; fixture feed stats: {:?}",
                reported.feed_server_stats()
            )
        });
    let attempt = reported
        .store()
        .list_generation_attempts()
        .unwrap()
        .remove(0);
    assert_eq!(attempt.input_tokens, Some(120));
    assert_eq!(attempt.output_tokens, Some(60));
    assert!(attempt.actual_cost_microusd.unwrap() < attempt.estimated_cost_microusd);

    let unreported = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_unreported_usage();
    unreported
        .app
        .refresh(unreported.now)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "unreported refresh failed: {error}; fixture feed stats: {:?}",
                unreported.feed_server_stats()
            )
        });
    let attempt = unreported
        .store()
        .list_generation_attempts()
        .unwrap()
        .remove(0);
    assert_eq!(attempt.input_tokens, None);
    assert_eq!(attempt.output_tokens, None);
    assert_eq!(
        attempt.actual_cost_microusd,
        Some(attempt.estimated_cost_microusd)
    );
}

#[tokio::test]
async fn multilingual_framing_heavy_prompt_reserves_bytes_schema_and_maximum_output() {
    let fixture = signal_core::test_support::ai_app_fixture();
    let mut story = signal_core::test_support::story_fixture("multilingual-framing-story");
    story.title = "研究🙂 lancement café".to_owned();
    story.excerpt = "中文、العربية、emoji 🚀 plus \\\"quoted\\\" structured facts.".to_owned();
    story.category = "研究/announcements".to_owned();
    story.source_ids = vec![
        "source-\\\"quoted\\\"".to_owned(),
        "源-一".to_owned(),
        "emoji-🙂".to_owned(),
    ];
    story.smart_summary = "A multilingual framing fixture.".to_owned();

    let settings = signal_core::SummarySettings::default();
    let prompt = signal_core::build_ai_summary_prompt(&story, &settings).unwrap();
    let conservative_input = u64::try_from(
        prompt
            .system_text
            .len()
            .checked_add(prompt.user_text.len())
            .unwrap(),
    )
    .unwrap()
    .checked_add(1_024)
    .unwrap();

    let store = fixture.store();
    store.upsert_stories(std::slice::from_ref(&story)).unwrap();
    let mut profile = fixture.profile();
    let reservation = conservative_input + u64::from(profile.limits.max_output_tokens);
    profile.limits.max_daily_cost_microusd = Some(reservation - 1);
    store.remove_model_profile(profile.id).unwrap();
    store.create_model_profile(&profile).unwrap();
    store.set_default_model_profile(Some(profile.id)).unwrap();

    let report = fixture
        .app
        .summarize_story(&story.id, SummarizeOptions::default(), fixture.now)
        .await
        .unwrap();

    assert_eq!(report.status, ManualGenerationStatus::BudgetExhausted);
    assert_eq!(report.generation.skipped_budget, 1);
    assert_eq!(fixture.provider.request_count(), 0);
    assert!(store.list_generation_attempts().unwrap().is_empty());
}

#[tokio::test]
async fn summarize_cache_hit_and_force_share_identity_and_select_forced_variant() {
    let fixture = signal_core::test_support::ai_app_fixture().with_max_items(1);
    let refresh = fixture
        .app
        .refresh(fixture.now)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "refresh failed: {error}; fixture feed stats: {:?}",
                fixture.feed_server_stats()
            )
        });
    let story_id = refresh.briefing.items[0].story.id.clone();
    let first = refresh.briefing.items[0].selected_summary.clone().unwrap();
    assert_eq!(fixture.provider.request_count(), 1);

    let cached = fixture
        .app
        .summarize_story(
            &story_id,
            SummarizeOptions::default(),
            fixture.now + Duration::seconds(1),
        )
        .await
        .unwrap();
    assert_eq!(cached.status, ManualGenerationStatus::CacheHit);
    assert_eq!(cached.summary.as_ref().unwrap().id, first.id);
    assert_eq!(fixture.provider.request_count(), 1);

    let forced = fixture
        .app
        .summarize_story(
            &story_id,
            SummarizeOptions {
                profile: None,
                force: true,
            },
            fixture.now + Duration::seconds(2),
        )
        .await
        .unwrap();
    assert_eq!(forced.status, ManualGenerationStatus::Generated);
    assert_ne!(forced.summary.as_ref().unwrap().id, first.id);
    assert_eq!(forced.summary.as_ref().unwrap().cache_key, first.cache_key);
    assert_eq!(fixture.provider.request_count(), 2);
    let latest = fixture
        .app
        .today(fixture.now + Duration::seconds(2))
        .unwrap();
    assert_eq!(
        latest.briefing.items[0]
            .selected_summary
            .as_ref()
            .unwrap()
            .id,
        forced.summary.unwrap().id
    );
}

#[tokio::test]
async fn model_test_records_attempt_but_no_story_variant_or_selection() {
    let fixture = signal_core::test_support::ai_app_fixture().with_max_items(1);
    let profile = fixture.profile();

    let report = fixture
        .app
        .test_model(
            TestModelOptions {
                profile: profile.name,
            },
            fixture.now,
        )
        .await
        .unwrap();

    assert!(report.cost_may_apply);
    assert_eq!(report.status, ManualGenerationStatus::Generated);
    assert_eq!(
        fixture.provider.requested_story_ids(),
        vec!["model-test".to_owned()]
    );
    assert!(report.attempt.is_some());
    assert!(
        fixture
            .store()
            .list_summary_variants("model-test")
            .unwrap()
            .is_empty()
    );
    assert!(fixture.store().load_latest_briefing().unwrap().is_none());
}

#[tokio::test]
async fn model_test_payload_is_fixed_while_attempts_use_invocation_clocks() {
    let fixture = signal_core::test_support::ai_app_fixture();
    let profile = fixture.profile();
    let first_now = fixture.now;
    let second_now = fixture.now + Duration::days(2);

    let first = fixture
        .app
        .test_model(
            TestModelOptions {
                profile: profile.name.clone(),
            },
            first_now,
        )
        .await
        .unwrap();
    let second = fixture
        .app
        .test_model(
            TestModelOptions {
                profile: profile.name,
            },
            second_now,
        )
        .await
        .unwrap();

    let prompts = fixture.provider.requested_prompts();
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[0], prompts[1]);
    assert_eq!(first.attempt.unwrap().reserved_at, first_now);
    assert_eq!(second.attempt.unwrap().reserved_at, second_now);
}

#[tokio::test]
async fn model_test_uses_the_same_daily_budget_path() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_budget_for_one_request();
    fixture.app.refresh(fixture.now).await.unwrap();
    let profile = fixture.profile();

    let report = fixture
        .app
        .test_model(
            TestModelOptions {
                profile: profile.name,
            },
            fixture.now,
        )
        .await
        .unwrap();

    assert_eq!(report.status, ManualGenerationStatus::BudgetExhausted);
    assert_eq!(report.generation.skipped_budget, 1);
    assert_eq!(fixture.provider.request_count(), 1);
}

#[tokio::test]
async fn profile_override_is_invocation_local() {
    let fixture = signal_core::test_support::ai_app_fixture().with_max_items(1);
    let refresh = fixture
        .app
        .refresh(fixture.now)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "refresh failed: {error}; fixture feed stats: {:?}",
                fixture.feed_server_stats()
            )
        });
    let story_id = refresh.briefing.items[0].story.id.clone();
    let original_default = fixture.profile();
    fixture
        .environment_reader
        .set("ALTERNATE_KEY", Some("alternate-secret".to_owned()));
    let alternate = fixture
        .app
        .add_model(
            AddModelInput {
                name: "alternate".to_owned(),
                provider: ProviderKind::OpenAi,
                model: "alternate-model".to_owned(),
                endpoint: None,
                dialect: None,
                credential: AddModelCredential::Environment {
                    variable: "ALTERNATE_KEY".to_owned(),
                },
                consented_at: Some(fixture.now),
                enabled: true,
                limits: ProfileLimits::default(),
            },
            fixture.now,
        )
        .unwrap()
        .profile;

    let report = fixture
        .app
        .summarize_story(
            &story_id,
            SummarizeOptions {
                profile: Some("alternate".to_owned()),
                force: true,
            },
            fixture.now + Duration::seconds(1),
        )
        .await
        .unwrap();

    assert_eq!(report.summary.unwrap().profile_id, Some(alternate.id));
    assert_eq!(
        fixture.store().default_model_profile().unwrap().unwrap().id,
        original_default.id
    );
}

#[tokio::test]
async fn unpriced_profiles_still_record_zero_cost_attempts() {
    let fixture = signal_core::test_support::ai_app_fixture()
        .with_max_items(1)
        .with_unpriced_profile();

    fixture.app.refresh(fixture.now).await.unwrap();

    let attempt = fixture
        .store()
        .list_generation_attempts()
        .unwrap()
        .remove(0);
    assert_eq!(attempt.estimated_cost_microusd, 0);
    assert_eq!(attempt.actual_cost_microusd, Some(0));
}

#[tokio::test]
async fn reservation_expiry_includes_attempts_delays_and_safety_margin() {
    let fixture = signal_core::test_support::ai_app_fixture();
    let profile = fixture.profile();

    let report = fixture
        .app
        .test_model(
            TestModelOptions {
                profile: profile.name,
            },
            fixture.now,
        )
        .await
        .unwrap();

    let attempt = report.attempt.unwrap();
    assert_eq!(
        attempt
            .expires_at
            .signed_duration_since(attempt.reserved_at),
        Duration::seconds(240)
    );
}

#[test]
fn add_use_and_remove_model_enforce_consent_and_redact_delete_warning() {
    let fixture = signal_core::test_support::ai_app_fixture().without_default_profile();
    let removed = fixture.app.remove_model(&fixture.profile().name).unwrap();
    assert!(removed.credential_deleted);

    let refused = fixture.app.add_model(
        AddModelInput {
            name: "refused".to_owned(),
            provider: ProviderKind::OpenAi,
            model: "refused-model".to_owned(),
            endpoint: None,
            dialect: None,
            credential: AddModelCredential::Environment {
                variable: "REFUSED_KEY".to_owned(),
            },
            consented_at: None,
            enabled: true,
            limits: ProfileLimits::default(),
        },
        fixture.now,
    );
    assert!(refused.is_err());

    let added = fixture
        .app
        .add_model(
            AddModelInput {
                name: "replacement".to_owned(),
                provider: ProviderKind::OpenAi,
                model: "replacement-model".to_owned(),
                endpoint: None,
                dialect: None,
                credential: AddModelCredential::SystemStore {
                    secret: SecretString::from("replacement-secret".to_owned()),
                },
                consented_at: Some(fixture.now),
                enabled: true,
                limits: ProfileLimits::default(),
            },
            fixture.now,
        )
        .unwrap();
    assert_eq!(
        fixture.app.use_model("replacement").unwrap().id,
        added.profile.id
    );

    fixture.credential_store.fail_deletes_for_test(true);
    let report = fixture.app.remove_model("replacement").unwrap();
    assert!(!report.credential_deleted);
    assert_eq!(report.warning, Some(CredentialWarningKind::DeleteFailed));
    assert!(fixture.app.list_models().unwrap().is_empty());
    assert!(!format!("{report:?}").contains("replacement-secret"));
}

#[test]
fn add_model_compensates_system_credential_when_profile_persistence_fails() {
    let fixture = signal_core::test_support::ai_app_fixture();
    let existing = fixture.profile();
    let result = fixture.app.add_model(
        AddModelInput {
            name: existing.name,
            provider: ProviderKind::OpenAi,
            model: "duplicate-name-model".to_owned(),
            endpoint: None,
            dialect: None,
            credential: AddModelCredential::SystemStore {
                secret: SecretString::from("temporary-secret".to_owned()),
            },
            consented_at: Some(fixture.now),
            enabled: true,
            limits: ProfileLimits::default(),
        },
        fixture.now,
    );

    assert!(result.is_err());
    assert_eq!(fixture.credential_store.credential_count_for_test(), 1);
}

#[test]
fn add_model_rejects_empty_and_whitespace_system_secrets_without_side_effects() {
    for (index, value) in ["", " \t\n"].into_iter().enumerate() {
        let fixture = signal_core::test_support::ai_app_fixture();
        let profile_count = fixture.app.list_models().unwrap().len();
        let credential_count = fixture.credential_store.credential_count_for_test();

        let result = fixture.app.add_model(
            AddModelInput {
                name: format!("invalid-secret-{index}"),
                provider: ProviderKind::OpenAi,
                model: "invalid-secret-model".to_owned(),
                endpoint: None,
                dialect: None,
                credential: AddModelCredential::SystemStore {
                    secret: SecretString::from(value.to_owned()),
                },
                consented_at: Some(fixture.now),
                enabled: true,
                limits: ProfileLimits::default(),
            },
            fixture.now,
        );

        let error = result.expect_err("empty secret should be rejected");
        assert_eq!(error.to_string(), "credential is empty");
        assert_eq!(fixture.app.list_models().unwrap().len(), profile_count);
        assert_eq!(
            fixture.credential_store.credential_count_for_test(),
            credential_count
        );
        assert_eq!(fixture.provider.request_count(), 0);
    }
}
