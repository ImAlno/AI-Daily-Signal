use signal_core::{
    ApiDialect, CredentialRef, MoneyMicros, NewModelProfile, ProfileLimits, ProviderKind,
    SignalError,
};

#[test]
fn multiple_profiles_round_trip_and_default_selection_is_persisted() {
    let store = signal_core::test_support::temporary_store();
    let openai = signal_core::test_support::model_profile("personal", ProviderKind::OpenAi);
    let anthropic = signal_core::test_support::model_profile("research", ProviderKind::Anthropic);

    store.create_model_profile(&openai).unwrap();
    store.create_model_profile(&anthropic).unwrap();
    store.set_default_model_profile(Some(anthropic.id)).unwrap();

    assert_eq!(
        store.list_model_profiles().unwrap(),
        vec![openai.clone(), anthropic.clone()]
    );
    assert_eq!(store.find_model_profile(openai.id).unwrap(), Some(openai));
    assert_eq!(
        store.find_model_profile_by_name("RESEARCH").unwrap(),
        Some(anthropic.clone())
    );
    assert_eq!(store.default_model_profile().unwrap(), Some(anthropic));
}

#[test]
fn profile_names_are_unique_without_regard_to_case_and_model_identifiers_are_opaque() {
    let store = signal_core::test_support::temporary_store();
    let mut first = signal_core::test_support::model_profile("Personal", ProviderKind::OpenAi);
    first.model = "vendor/model:2026-08?tier=beta".to_owned();
    let second = signal_core::test_support::model_profile("personal", ProviderKind::Gemini);

    store.create_model_profile(&first).unwrap();
    assert!(store.create_model_profile(&second).is_err());
    assert_eq!(
        store.find_model_profile(first.id).unwrap().unwrap().model,
        first.model
    );
}

#[test]
fn custom_http_endpoint_is_allowed_only_for_loopback() {
    assert!(
        new_profile_with_endpoint("http://127.0.0.1:8080/v1")
            .validate()
            .is_ok()
    );
    assert!(
        new_profile_with_endpoint("http://[::1]:8080/v1")
            .validate()
            .is_ok()
    );
    assert!(
        new_profile_with_endpoint("http://127.0.0.2:8080/v1")
            .validate()
            .is_err()
    );
    assert!(
        new_profile_with_endpoint("http://provider.example/v1")
            .validate()
            .is_err()
    );
    assert!(
        new_profile_with_endpoint("https://user:password@provider.example/v1")
            .validate()
            .is_err()
    );
}

#[test]
fn provider_endpoint_and_dialect_combinations_are_validated() {
    let mut official = new_profile_with_endpoint("https://provider.example/v1");
    official.provider = ProviderKind::OpenAi;
    official.endpoint = None;
    assert_eq!(official.dialect, Some(ApiDialect::ChatCompletions));
    assert!(official.validate().is_err());

    official.dialect = None;
    assert!(official.validate().is_ok());

    let mut custom = new_profile_with_endpoint("https://provider.example/v1");
    custom.dialect = None;
    assert!(custom.validate().is_err());
}

#[test]
fn invalid_profile_inputs_are_rejected_before_persistence() {
    let mut profile = new_profile_with_endpoint("https://provider.example/v1");
    profile.name = " \t ".to_owned();
    assert!(profile.validate().is_err());

    let mut profile = new_profile_with_endpoint("https://provider.example/v1");
    profile.model = "\n".to_owned();
    assert!(profile.validate().is_err());

    let mut profile = new_profile_with_endpoint("https://provider.example/v1");
    profile.credential = CredentialRef::Environment {
        variable: "not-valid".to_owned(),
    };
    assert!(profile.validate().is_err());

    let mut profile = new_profile_with_endpoint("https://provider.example/v1");
    profile.credential = CredentialRef::SystemStore {
        service: "other.service".to_owned(),
        account: "model-profile/00000000-0000-0000-0000-000000000000".to_owned(),
    };
    assert!(profile.validate().is_err());
}

#[test]
fn limits_require_nonzero_values_complete_rates_and_budget_rates() {
    let mut profile = new_profile_with_endpoint("https://provider.example/v1");
    profile.limits.max_summaries_per_refresh = 0;
    assert!(profile.validate().is_err());

    let mut profile = new_profile_with_endpoint("https://provider.example/v1");
    profile.limits.input_cost_microusd_per_million = Some(1);
    assert!(profile.validate().is_err());

    let mut profile = new_profile_with_endpoint("https://provider.example/v1");
    profile.limits.max_daily_cost_microusd = Some(1);
    assert!(profile.validate().is_err());
}

#[test]
fn zero_daily_cost_cap_is_rejected_even_when_rates_are_configured() {
    let mut profile = new_profile_with_endpoint("https://provider.example/v1");
    profile.limits.max_daily_cost_microusd = Some(0);
    profile.limits.input_cost_microusd_per_million = Some(1);
    profile.limits.output_cost_microusd_per_million = Some(1);

    assert!(profile.validate().is_err());
}

#[test]
fn usd_values_parse_without_floating_point() {
    assert_eq!(
        MoneyMicros::parse_usd("1.234567").unwrap().as_micros(),
        1_234_567
    );
    assert_eq!(MoneyMicros::parse_usd(".1").unwrap().as_micros(), 100_000);
    assert!(MoneyMicros::parse_usd("0.0000001").is_err());
    assert!(MoneyMicros::parse_usd("-0.01").is_err());
}

#[test]
fn removing_the_default_profile_clears_only_that_default_reference() {
    let store = signal_core::test_support::temporary_store();
    let first = signal_core::test_support::model_profile("first", ProviderKind::OpenAi);
    let second = signal_core::test_support::model_profile("second", ProviderKind::Anthropic);
    store.create_model_profile(&first).unwrap();
    store.create_model_profile(&second).unwrap();
    store.set_default_model_profile(Some(first.id)).unwrap();

    store.remove_model_profile(first.id).unwrap();

    assert_eq!(store.default_model_profile().unwrap(), None);
    assert_eq!(store.find_model_profile(second.id).unwrap(), Some(second));
}

#[test]
fn missing_default_profile_is_not_accepted() {
    let store = signal_core::test_support::temporary_store();
    let profile = signal_core::test_support::model_profile("unpersisted", ProviderKind::OpenAi);

    let error = store
        .set_default_model_profile(Some(profile.id))
        .unwrap_err();

    assert!(matches!(error, SignalError::NotFound(_)));
}

fn new_profile_with_endpoint(endpoint: &str) -> NewModelProfile {
    NewModelProfile {
        name: "custom".to_owned(),
        provider: ProviderKind::OpenAiCompatible,
        model: "local/model".to_owned(),
        endpoint: Some(endpoint.parse().unwrap()),
        dialect: Some(ApiDialect::ChatCompletions),
        credential: CredentialRef::Environment {
            variable: "LOCAL_API_KEY".to_owned(),
        },
        consented_at: None,
        enabled: true,
        limits: ProfileLimits::default(),
    }
}
