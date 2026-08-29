use assert_cmd::Command;
use predicates::prelude::*;
use signal_core::{AppPaths, ConfigRepository, Source, SourceKind, Store};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn assert_tree_does_not_contain(root: &std::path::Path, sentinel: &str) {
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            assert_tree_does_not_contain(&path, sentinel);
        } else {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(sentinel.len())
                    .any(|window| window == sentinel.as_bytes()),
                "credential sentinel leaked to {}",
                path.display()
            );
        }
    }
}

fn local_feed_once() -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let published = chrono::Utc::now().to_rfc2822();
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
<title>Fixture feed</title><link>https://example.com</link><description>Fixture</description>
<item><guid>fixture-1</guid><title>Local fixture signal</title>
<link>https://example.com/local-signal</link>
<description>A complete local fixture sentence.</description><pubDate>{published}</pubDate></item>
</channel></rss>"#
    );
    let server = std::thread::spawn(move || {
        use std::io::{Read, Write};

        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 2_048];
        let _ = stream.read(&mut request).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/rss+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    (format!("http://{address}/feed.xml"), server)
}

fn cached_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(home.path());
    let repository = ConfigRepository::new(paths.clone());
    let mut config = repository.load_or_create().unwrap();
    for source in &mut config.sources {
        let SourceKind::Feed { url } = &mut source.kind;
        *url = "http://127.0.0.1:9/unreachable.xml".to_owned();
    }
    repository.save(&config).unwrap();

    let mut briefing = signal_core::test_support::briefing_fixture();
    briefing.date = chrono::Utc::now().date_naive();
    briefing.generated_at = chrono::Utc::now();
    let stories = briefing
        .items
        .iter()
        .map(|item| item.story.clone())
        .collect::<Vec<_>>();
    Store::open(paths.data_dir.join("signal.sqlite3"))
        .unwrap()
        .commit_refresh(&stories, &briefing)
        .unwrap();
    home
}

fn local_feed_body() -> String {
    let published = chrono::Utc::now().to_rfc2822();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
<title>Fixture feed</title><link>https://example.com</link><description>Fixture</description>
<item><guid>fixture-ai-1</guid><title>Local AI fixture signal</title>
<link>https://example.com/local-ai-signal</link>
<description>A complete local AI fixture sentence.</description><pubDate>{published}</pubDate></item>
</channel></rss>"#
    )
}

async fn configure_wiremock_feed(home: &std::path::Path, server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/feed.xml"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/rss+xml")
                .set_body_string(local_feed_body()),
        )
        .mount(server)
        .await;
    let paths = AppPaths::for_root(home);
    let repository = ConfigRepository::new(paths);
    let mut config = repository.load_or_create().unwrap();
    config.sources = vec![Source {
        id: "wiremock-feed".to_owned(),
        name: "WireMock fixture".to_owned(),
        category: "research".to_owned(),
        enabled: true,
        weight: 1.0,
        kind: SourceKind::Feed {
            url: format!("{}/feed.xml", server.uri()),
        },
    }];
    repository.save(&config).unwrap();
}

fn add_custom_model(home: &std::path::Path, server: &MockServer, name: &str, variable: &str) {
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home)
        .args([
            "models",
            "add",
            "--name",
            name,
            "--provider",
            "open-ai-compatible",
            "--model",
            "vendor/opaque:model",
            "--endpoint",
            &format!("{}/api/v1/", server.uri()),
            "--dialect",
            "chat-completions",
            "--credential-env",
            variable,
            "--max-retries",
            "1",
            "--consent-provider-data-sharing",
        ])
        .assert()
        .success();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home)
        .args(["models", "use", name])
        .assert()
        .success();
}

fn successful_chat_response(label: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "chatcmpl_cli_fixture",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": serde_json::json!({
                    "what_happened": format!("{label} happened."),
                    "why_it_matters": format!("{label} matters."),
                    "caveat": null
                }).to_string()
            },
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 51, "completion_tokens": 19, "total_tokens": 70}
    })
}

#[test]
fn model_profiles_with_separate_environment_sources_persist_and_are_redacted() {
    let home = tempfile::tempdir().unwrap();
    let alpha_secret = "SENTINEL_ALPHA_CREDENTIAL_7eb06f";
    let beta_secret = "SENTINEL_BETA_CREDENTIAL_42b93a";
    for (name, provider, model, variable, secret) in [
        (
            "Alpha",
            "open-ai",
            "gpt-alpha",
            "ALPHA_MODEL_KEY",
            alpha_secret,
        ),
        (
            "Beta",
            "anthropic",
            "claude-beta",
            "BETA_MODEL_KEY",
            beta_secret,
        ),
    ] {
        let output = Command::cargo_bin("signal")
            .unwrap()
            .env("SIGNAL_HOME", home.path())
            .env(variable, secret)
            .args([
                "--json",
                "models",
                "add",
                "--name",
                name,
                "--provider",
                provider,
                "--model",
                model,
                "--credential-env",
                variable,
                "--consent-provider-data-sharing",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(
            !output
                .stdout
                .windows(secret.len())
                .any(|value| value == secret.as_bytes())
        );
        assert!(
            !output
                .stderr
                .windows(secret.len())
                .any(|value| value == secret.as_bytes())
        );
    }

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["models", "use", "bEtA"])
        .assert()
        .success();

    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .env("ALPHA_MODEL_KEY", alpha_secret)
        .env("BETA_MODEL_KEY", beta_secret)
        .args(["--json", "models", "list"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    let profiles = value["data"].as_array().unwrap();
    assert_eq!(profiles.len(), 2);
    let alpha = profiles
        .iter()
        .find(|profile| profile["name"] == "Alpha")
        .unwrap();
    assert_eq!(alpha["provider"], "open-ai");
    assert_eq!(alpha["model"], "gpt-alpha");
    assert_eq!(alpha["credential_source"], "environment");
    assert_eq!(alpha["is_default"], false);
    assert!(alpha.get("credential").is_none());
    let beta = profiles
        .iter()
        .find(|profile| profile["name"] == "Beta")
        .unwrap();
    assert_eq!(beta["provider"], "anthropic");
    assert_eq!(beta["model"], "claude-beta");
    assert_eq!(beta["credential_source"], "environment");
    assert_eq!(beta["is_default"], true);
    assert!(beta.get("credential").is_none());
    for secret in [alpha_secret, beta_secret] {
        assert!(
            !output
                .stdout
                .windows(secret.len())
                .any(|value| value == secret.as_bytes())
        );
        assert!(
            !output
                .stderr
                .windows(secret.len())
                .any(|value| value == secret.as_bytes())
        );
        assert_tree_does_not_contain(home.path(), secret);
    }

    let stored = Store::open(
        AppPaths::for_root(home.path())
            .data_dir
            .join("signal.sqlite3"),
    )
    .unwrap()
    .list_model_profiles()
    .unwrap();
    let sources = stored
        .iter()
        .map(|profile| match &profile.credential {
            signal_core::CredentialRef::Environment { variable } => variable.as_str(),
            signal_core::CredentialRef::SystemStore { .. } => "system-store",
        })
        .collect::<Vec<_>>();
    assert_eq!(sources, ["ALPHA_MODEL_KEY", "BETA_MODEL_KEY"]);
}

#[test]
fn noninteractive_model_add_requires_explicit_consent_and_credential_reference() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args([
            "models",
            "add",
            "--name",
            "No consent",
            "--provider",
            "open-ai",
            "--model",
            "opaque",
            "--credential-env",
            "MODEL_KEY",
        ])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("consent").and(predicate::str::contains("MODEL_KEY").not()),
        );

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args([
            "models",
            "add",
            "--name",
            "Visible secret",
            "--provider",
            "open-ai",
            "--model",
            "opaque",
            "--consent-provider-data-sharing",
        ])
        .write_stdin("VISIBLE_SENTINEL_MUST_NOT_BE_READ\n")
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("credential")
                .and(predicate::str::contains("VISIBLE_SENTINEL_MUST_NOT_BE_READ").not()),
        );

    let profiles = Store::open(
        AppPaths::for_root(home.path())
            .data_dir
            .join("signal.sqlite3"),
    )
    .unwrap()
    .list_model_profiles()
    .unwrap();
    assert!(profiles.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_selects_ai_fields_and_no_ai_makes_zero_provider_requests() {
    let home = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    configure_wiremock_feed(home.path(), &server).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(successful_chat_response("CLI AI")))
        .mount(&server)
        .await;
    add_custom_model(home.path(), &server, "Local custom", "LOCAL_CUSTOM_KEY");

    let generated = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .env("LOCAL_CUSTOM_KEY", "SENTINEL_LOCAL_CUSTOM_SECRET")
        .args(["--json", "refresh"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "{generated:?}");
    let generated_json: serde_json::Value = serde_json::from_slice(&generated.stdout).unwrap();
    assert_eq!(generated_json["schema_version"], 1);
    assert_eq!(generated_json["data"]["successful_sources"], 1);
    assert_eq!(generated_json["data"]["generation"]["generated"], 1);
    assert_eq!(generated_json["data"]["generation"]["smart_fallbacks"], 0);
    let selected = &generated_json["data"]["briefing"]["items"][0]["selected_summary"];
    assert_eq!(selected["fields"]["what_happened"], "CLI AI happened.");
    assert_eq!(selected["fields"]["why_it_matters"], "CLI AI matters.");
    assert_eq!(selected["provider"], "open_ai_compatible");
    assert_eq!(selected["model"], "vendor/opaque:model");

    let without_ai = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .env("LOCAL_CUSTOM_KEY", "SENTINEL_LOCAL_CUSTOM_SECRET")
        .args(["--json", "refresh", "--no-ai"])
        .output()
        .unwrap();
    assert!(without_ai.status.success(), "{without_ai:?}");
    let without_ai_json: serde_json::Value = serde_json::from_slice(&without_ai.stdout).unwrap();
    assert_eq!(without_ai_json["data"]["generation"]["eligible"], 0);
    assert_eq!(without_ai_json["data"]["generation"]["generated"], 0);
    assert!(without_ai_json["data"]["briefing"]["items"][0]["selected_summary"].is_null());

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/api/v1/chat/completions")
            .count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automatic_provider_failure_keeps_refresh_successful_with_smart_fallback() {
    const PROVIDER_BODY: &str = "SENTINEL_AUTOMATIC_PROVIDER_BODY";
    const CREDENTIAL: &str = "SENTINEL_AUTOMATIC_CREDENTIAL";
    let home = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    configure_wiremock_feed(home.path(), &server).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string(PROVIDER_BODY))
        .mount(&server)
        .await;
    add_custom_model(home.path(), &server, "Fallback", "FALLBACK_MODEL_KEY");

    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .env("FALLBACK_MODEL_KEY", CREDENTIAL)
        .args(["--json", "refresh"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(
        !output
            .stdout
            .windows(PROVIDER_BODY.len())
            .any(|value| value == PROVIDER_BODY.as_bytes())
    );
    assert!(
        !output
            .stderr
            .windows(PROVIDER_BODY.len())
            .any(|value| value == PROVIDER_BODY.as_bytes())
    );
    assert!(
        !output
            .stdout
            .windows(CREDENTIAL.len())
            .any(|value| value == CREDENTIAL.as_bytes())
    );
    assert!(
        !output
            .stderr
            .windows(CREDENTIAL.len())
            .any(|value| value == CREDENTIAL.as_bytes())
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["generation"]["provider_failures"], 1);
    assert_eq!(value["data"]["generation"]["smart_fallbacks"], 1);
    assert!(value["data"]["briefing"]["items"][0]["selected_summary"].is_null());
    assert_eq!(
        value["data"]["briefing"]["items"][0]["story"]["smart_summary"],
        "A complete local AI fixture sentence."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn summarize_uses_cache_force_regenerates_and_remove_retains_variants() {
    let home = cached_home();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(successful_chat_response("Manual")))
        .mount(&server)
        .await;
    add_custom_model(home.path(), &server, "Manual profile", "MANUAL_MODEL_KEY");

    for (extra, expected_status) in [
        (Vec::<&str>::new(), "generated"),
        (Vec::<&str>::new(), "cache_hit"),
        (vec!["--force"], "generated"),
    ] {
        let mut args = vec![
            "--json",
            "summarize",
            "story-1",
            "--model",
            "manual PROFILE",
        ];
        args.extend(extra);
        let output = Command::cargo_bin("signal")
            .unwrap()
            .env("SIGNAL_HOME", home.path())
            .env("MANUAL_MODEL_KEY", "SENTINEL_MANUAL_CREDENTIAL")
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["data"]["status"], expected_status);
        assert_eq!(
            value["data"]["summary"]["fields"]["what_happened"],
            "Manual happened."
        );
    }

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["models", "remove", "MANUAL profile", "--yes"])
        .assert()
        .success();
    let store = Store::open(
        AppPaths::for_root(home.path())
            .data_dir
            .join("signal.sqlite3"),
    )
    .unwrap();
    assert!(store.list_model_profiles().unwrap().is_empty());
    assert_eq!(store.list_summary_variants("story-1").unwrap().len(), 2);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/api/v1/chat/completions")
            .count(),
        2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_model_test_failure_uses_exit_six_with_redacted_report() {
    const PROVIDER_BODY: &str = "SENTINEL_EXPLICIT_PROVIDER_BODY";
    const CREDENTIAL: &str = "SENTINEL_EXPLICIT_CREDENTIAL";
    let home = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string(PROVIDER_BODY))
        .mount(&server)
        .await;
    add_custom_model(home.path(), &server, "Failing test", "FAILING_MODEL_KEY");

    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .env("FAILING_MODEL_KEY", CREDENTIAL)
        .args(["--json", "models", "test", "failing TEST"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(6), "{output:?}");
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["status"], "provider_failure");
    assert_eq!(value["data"]["generation"]["provider_failures"], 1);
    assert!(
        !output
            .stdout
            .windows(PROVIDER_BODY.len())
            .any(|value| value == PROVIDER_BODY.as_bytes())
    );
    assert!(
        !output
            .stderr
            .windows(PROVIDER_BODY.len())
            .any(|value| value == PROVIDER_BODY.as_bytes())
    );
    assert!(
        !output
            .stdout
            .windows(CREDENTIAL.len())
            .any(|value| value == CREDENTIAL.as_bytes())
    );
    assert!(
        !output
            .stderr
            .windows(CREDENTIAL.len())
            .any(|value| value == CREDENTIAL.as_bytes())
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("AI generation failed")
    );
}

#[test]
fn money_flags_are_exact_and_invalid_pairs_do_not_persist_profiles() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args([
            "models",
            "add",
            "--name",
            "Priced",
            "--provider",
            "open-ai",
            "--model",
            "opaque",
            "--credential-env",
            "PRICED_KEY",
            "--daily-budget-usd",
            "1.000001",
            "--input-usd-per-million",
            "0.123456",
            "--output-usd-per-million",
            "2",
            "--consent-provider-data-sharing",
        ])
        .assert()
        .success();
    let store = Store::open(
        AppPaths::for_root(home.path())
            .data_dir
            .join("signal.sqlite3"),
    )
    .unwrap();
    let priced = store.list_model_profiles().unwrap().remove(0);
    assert_eq!(priced.limits.max_daily_cost_microusd, Some(1_000_001));
    assert_eq!(priced.limits.input_cost_microusd_per_million, Some(123_456));
    assert_eq!(
        priced.limits.output_cost_microusd_per_million,
        Some(2_000_000)
    );

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args([
            "models",
            "add",
            "--name",
            "Invalid pair",
            "--provider",
            "open-ai",
            "--model",
            "opaque",
            "--credential-env",
            "INVALID_KEY",
            "--input-usd-per-million",
            "1.5",
            "--consent-provider-data-sharing",
        ])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("INVALID_KEY").not());
    assert_eq!(store.list_model_profiles().unwrap().len(), 1);
}

#[test]
fn noninteractive_remove_requires_yes_and_missing_profile_errors_are_redacted() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args([
            "models",
            "add",
            "--name",
            "Removable",
            "--provider",
            "open-ai",
            "--model",
            "opaque",
            "--credential-env",
            "REMOVABLE_KEY",
            "--consent-provider-data-sharing",
        ])
        .assert()
        .success();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["models", "remove", "Removable"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--yes"));

    let missing = "SENTINEL-MISSING-PROFILE-ID";
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["models", "remove", missing, "--yes"])
        .assert()
        .failure()
        .code(4)
        .stderr(predicate::str::contains("Model profile was not found"))
        .stderr(predicate::str::contains(missing).not());
    assert_eq!(
        Store::open(
            AppPaths::for_root(home.path())
                .data_dir
                .join("signal.sqlite3")
        )
        .unwrap()
        .list_model_profiles()
        .unwrap()
        .len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_credential_and_budget_failures_use_exit_six_without_requests() {
    let home = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    add_custom_model(
        home.path(),
        &server,
        "Missing credential",
        "ABSENT_MODEL_KEY",
    );
    let credential = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .env_remove("ABSENT_MODEL_KEY")
        .args(["--json", "models", "test", "Missing credential"])
        .output()
        .unwrap();
    assert_eq!(credential.status.code(), Some(6), "{credential:?}");
    let credential_json: serde_json::Value = serde_json::from_slice(&credential.stdout).unwrap();
    assert_eq!(credential_json["data"]["status"], "credential_unavailable");
    assert_eq!(
        credential_json["data"]["generation"]["missing_credentials"],
        1
    );

    let budget_home = tempfile::tempdir().unwrap();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", budget_home.path())
        .args([
            "models",
            "add",
            "--name",
            "Budget",
            "--provider",
            "open-ai-compatible",
            "--model",
            "opaque",
            "--endpoint",
            &format!("{}/api/v1/", server.uri()),
            "--dialect",
            "chat-completions",
            "--credential-env",
            "BUDGET_MODEL_KEY",
            "--daily-budget-usd",
            ".000001",
            "--input-usd-per-million",
            "1",
            "--output-usd-per-million",
            "1",
            "--max-retries",
            "1",
            "--consent-provider-data-sharing",
        ])
        .assert()
        .success();
    let budget = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", budget_home.path())
        .env("BUDGET_MODEL_KEY", "SENTINEL_BUDGET_CREDENTIAL")
        .args(["--json", "models", "test", "Budget"])
        .output()
        .unwrap();
    assert_eq!(budget.status.code(), Some(6), "{budget:?}");
    let budget_json: serde_json::Value = serde_json::from_slice(&budget.stdout).unwrap();
    assert_eq!(budget_json["data"]["status"], "budget_exhausted");
    assert_eq!(budget_json["data"]["generation"]["skipped_budget"], 1);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn show_keeps_story_fields_and_adds_the_selected_ai_summary() {
    let home = cached_home();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(successful_chat_response("Shown")))
        .mount(&server)
        .await;
    add_custom_model(home.path(), &server, "Show profile", "SHOW_MODEL_KEY");
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .env("SHOW_MODEL_KEY", "SENTINEL_SHOW_CREDENTIAL")
        .args(["summarize", "story-1"])
        .assert()
        .success();

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("show")
        .arg("story-1")
        .assert()
        .success()
        .stdout(predicate::str::contains("A deterministic signal"))
        .stdout(predicate::str::contains(
            "A stable summary for storage tests.",
        ))
        .stdout(predicate::str::contains("https://example.com/story-1"))
        .stdout(predicate::str::contains("Summary mode: AI"))
        .stdout(predicate::str::contains("Provider: open-ai-compatible"))
        .stdout(predicate::str::contains("Model: vendor/opaque:model"))
        .stdout(predicate::str::contains("What happened: Shown happened."));

    let json = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "show", "story-1"])
        .output()
        .unwrap();
    assert!(json.status.success(), "{json:?}");
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["id"], "story-1");
    assert_eq!(
        value["data"]["smart_summary"],
        "A stable summary for storage tests."
    );
    assert_eq!(
        value["data"]["selected_summary"]["fields"]["what_happened"],
        "Shown happened."
    );
    assert_eq!(
        value["data"]["selected_summary"]["provider"],
        "open-ai-compatible"
    );
}

#[test]
fn init_then_status_json_returns_schema_version_one() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("init")
        .assert()
        .success();

    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "status"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["story_count"], 0);
}

#[test]
fn today_does_not_refresh_without_the_flag() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("today")
        .assert()
        .failure()
        .code(4)
        .stderr(predicate::str::contains("No briefing is stored"));
}

#[test]
fn today_reads_the_cached_briefing_without_collecting() {
    let home = cached_home();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("today")
        .assert()
        .success()
        .stdout(predicate::str::contains("A deterministic signal"));
}

#[test]
fn plain_today_contains_no_ansi_escapes() {
    let home = cached_home();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--plain", "today"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\u{1b}[").not());
}

#[test]
fn json_today_uses_the_versioned_envelope() {
    let home = cached_home();
    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "today"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["items"][0]["story"]["id"], "story-1");
}

#[test]
fn save_changes_show_json_across_processes() {
    let home = cached_home();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["save", "story-1"])
        .assert()
        .success();

    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "show", "story-1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["is_saved"], true);
}

#[test]
fn disabling_a_source_survives_a_new_process() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("init")
        .assert()
        .success();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["sources", "disable", "openai-news"])
        .assert()
        .success();

    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "sources", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let source = value["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["id"] == "openai-news")
        .unwrap();
    assert_eq!(source["enabled"], false);
}

#[test]
fn a_total_refresh_failure_preserves_the_cached_briefing() {
    let home = cached_home();
    let paths = AppPaths::for_root(home.path());
    let store = Store::open(paths.data_dir.join("signal.sqlite3")).unwrap();
    let generation_before = store.status().unwrap().data_generation;
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("refresh")
        .assert()
        .failure()
        .code(3)
        .stderr(predicate::str::contains("Refresh failed"));

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("today")
        .assert()
        .success()
        .stdout(predicate::str::contains("A deterministic signal"));

    let status = store.status().unwrap();
    assert_eq!(status.data_generation, generation_before);
    let run = store.latest_refresh_run().unwrap().unwrap();
    assert_eq!(run.successful_sources, 0);
    assert_eq!(run.failed_sources, 7);
}

#[test]
fn latest_saved_and_remove_cover_the_remaining_story_commands() {
    let home = cached_home();
    let paths = AppPaths::for_root(home.path());
    let mut second = signal_core::test_support::story_fixture("story-2");
    second.title = "A second deterministic signal".to_owned();
    Store::open(paths.data_dir.join("signal.sqlite3"))
        .unwrap()
        .upsert_stories(&[second])
        .unwrap();

    let latest = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "latest", "--limit", "1"])
        .output()
        .unwrap();
    let latest_json: serde_json::Value = serde_json::from_slice(&latest.stdout).unwrap();
    assert_eq!(latest_json["data"].as_array().unwrap().len(), 1);

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["save", "story-1"])
        .assert()
        .success();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("saved")
        .assert()
        .success()
        .stdout(predicate::str::contains("A deterministic signal"));
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["save", "story-1", "--remove"])
        .assert()
        .success();
    let saved = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "saved"])
        .output()
        .unwrap();
    let saved_json: serde_json::Value = serde_json::from_slice(&saved.stdout).unwrap();
    assert_eq!(saved_json["data"].as_array().unwrap().len(), 0);
}

#[test]
fn human_briefing_contains_the_required_fields_and_saved_marker() {
    let home = cached_home();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["save", "story-1"])
        .assert()
        .success();

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("today")
        .assert()
        .success()
        .stdout(predicate::str::contains("Briefing for"))
        .stdout(predicate::str::contains("Generated:"))
        .stdout(predicate::str::contains(
            "1. A deterministic signal [saved]",
        ))
        .stdout(predicate::str::contains(
            "A stable summary for storage tests.",
        ))
        .stdout(predicate::str::contains("https://example.com/story-1"));
}

#[test]
fn source_can_be_reenabled_in_a_later_process() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["sources", "disable", "openai-news"])
        .assert()
        .success();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["sources", "enable", "openai-news"])
        .assert()
        .success();

    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "sources", "list"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let source = value["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["id"] == "openai-news")
        .unwrap();
    assert_eq!(source["enabled"], true);
}

#[test]
fn missing_story_uses_not_found_exit_code_without_echoing_the_id() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["show", "secret-looking-story-id"])
        .assert()
        .failure()
        .code(4)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Story was not found"))
        .stderr(predicate::str::contains("secret-looking-story-id").not());
}

#[test]
fn malformed_configuration_is_redacted_and_uses_exit_code_two() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(home.path());
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(
        paths.config_dir.join("config.toml"),
        "[briefing\nprivate-value-should-not-appear",
    )
    .unwrap();

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("status")
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "Configuration could not be read or written",
        ))
        .stderr(predicate::str::contains("private-value-should-not-appear").not());
}

#[test]
fn storage_failures_are_redacted_and_use_exit_code_five() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(home.path());
    ConfigRepository::new(paths.clone())
        .load_or_create()
        .unwrap();
    std::fs::write(&paths.data_dir, "not a directory").unwrap();

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("status")
        .assert()
        .failure()
        .code(5)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Storage operation failed"))
        .stderr(predicate::str::contains(home.path().to_string_lossy().as_ref()).not());
}

#[test]
fn clap_rejects_invalid_commands_with_exit_code_two() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("not-a-command")
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty());
}

#[test]
fn refresh_collects_a_local_feed_and_reports_source_counts() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(home.path());
    let repository = ConfigRepository::new(paths);
    let mut config = repository.load_or_create().unwrap();
    let (feed_url, server) = local_feed_once();
    config.sources = vec![
        Source {
            id: "local-fixture".to_owned(),
            name: "Local fixture".to_owned(),
            category: "research".to_owned(),
            enabled: true,
            weight: 1.0,
            kind: SourceKind::Feed { url: feed_url },
        },
        Source {
            id: "failed-fixture".to_owned(),
            name: "Failed fixture".to_owned(),
            category: "research".to_owned(),
            enabled: true,
            weight: 1.0,
            kind: SourceKind::Feed {
                url: "http://127.0.0.1:9/unreachable.xml".to_owned(),
            },
        },
    ];
    repository.save(&config).unwrap();

    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "refresh"])
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["successful_sources"], 1);
    assert_eq!(value["data"]["failures"].as_array().unwrap().len(), 1);
    assert_eq!(
        value["data"]["briefing"]["items"][0]["story"]["title"],
        "Local fixture signal"
    );
    let store = Store::open(
        AppPaths::for_root(home.path())
            .data_dir
            .join("signal.sqlite3"),
    )
    .unwrap();
    let run = store.latest_refresh_run().unwrap().unwrap();
    assert_eq!(run.successful_sources, 1);
    assert_eq!(run.failed_sources, 1);

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("today")
        .assert()
        .success()
        .stdout(predicate::str::contains("Local fixture signal"));
}

#[test]
fn failed_refresh_bookkeeping_is_a_redacted_storage_error() {
    let home = cached_home();
    let paths = AppPaths::for_root(home.path());
    let database_path = paths.data_dir.join("signal.sqlite3");
    let store = Store::open(&database_path).unwrap();
    let generation_before = store.status().unwrap().data_generation;
    rusqlite::Connection::open(&database_path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_failed_refresh_run
             BEFORE INSERT ON refresh_runs
             BEGIN
                 SELECT RAISE(ABORT, 'private bookkeeping detail');
             END;",
        )
        .unwrap();

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("refresh")
        .assert()
        .failure()
        .code(5)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Storage operation failed"))
        .stderr(predicate::str::contains("private bookkeeping detail").not());

    assert_eq!(store.status().unwrap().data_generation, generation_before);
    assert!(
        store
            .load_briefing(chrono::Utc::now().date_naive())
            .unwrap()
            .is_some()
    );
    assert!(store.latest_refresh_run().unwrap().is_none());
}

#[test]
fn today_loads_yesterdays_latest_briefing_and_reports_stale_status() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(home.path());
    let repository = ConfigRepository::new(paths.clone());
    let mut config = repository.load_or_create().unwrap();
    config.briefing.stale_after_minutes = 60;
    repository.save(&config).unwrap();
    let mut briefing = signal_core::test_support::briefing_fixture();
    briefing.date = (chrono::Utc::now() - chrono::Duration::days(1)).date_naive();
    briefing.generated_at = chrono::Utc::now() - chrono::Duration::days(1);
    let stories = briefing
        .items
        .iter()
        .map(|item| item.story.clone())
        .collect::<Vec<_>>();
    Store::open(paths.data_dir.join("signal.sqlite3"))
        .unwrap()
        .commit_refresh(&stories, &briefing)
        .unwrap();

    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "today"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["date"], briefing.date.to_string());
    assert_eq!(value["data"]["is_stale"], true);
    assert_eq!(value["data"]["items"][0]["story"]["id"], "story-1");
    assert_eq!(value["data"]["items"][0]["is_stale"], false);

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("today")
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: stale"));
}

#[test]
fn today_refresh_returns_a_fresh_versioned_view() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(home.path());
    let repository = ConfigRepository::new(paths);
    let mut config = repository.load_or_create().unwrap();
    let (feed_url, server) = local_feed_once();
    config.sources = vec![Source {
        id: "local-fixture".to_owned(),
        name: "Local fixture".to_owned(),
        category: "research".to_owned(),
        enabled: true,
        weight: 1.0,
        kind: SourceKind::Feed { url: feed_url },
    }];
    repository.save(&config).unwrap();

    let output = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "today", "--refresh"])
        .output()
        .unwrap();
    server.join().unwrap();

    assert!(output.status.success(), "{:?}", output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["is_stale"], false);
    assert_eq!(
        value["data"]["items"][0]["story"]["title"],
        "Local fixture signal"
    );
    assert_eq!(value["data"]["items"][0]["is_stale"], false);
}

#[test]
fn partial_refresh_carries_and_persists_failed_source_item_as_stale() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(home.path());
    let repository = ConfigRepository::new(paths.clone());
    let mut config = repository.load_or_create().unwrap();
    config.briefing.max_items = 2;
    let (feed_url, server) = local_feed_once();
    config.sources = vec![
        Source {
            id: "local-fixture".to_owned(),
            name: "Local fixture".to_owned(),
            category: "research".to_owned(),
            enabled: true,
            weight: 1.0,
            kind: SourceKind::Feed { url: feed_url },
        },
        Source {
            id: "failed-fixture".to_owned(),
            name: "Failed fixture".to_owned(),
            category: "research".to_owned(),
            enabled: true,
            weight: 1.0,
            kind: SourceKind::Feed {
                url: "http://127.0.0.1:9/unreachable.xml".to_owned(),
            },
        },
    ];
    repository.save(&config).unwrap();
    let mut previous = signal_core::test_support::briefing_fixture();
    previous.date = (chrono::Utc::now() - chrono::Duration::days(1)).date_naive();
    previous.generated_at = chrono::Utc::now() - chrono::Duration::days(1);
    previous.items[0].story.source_ids = vec!["failed-fixture".to_owned()];
    previous.items[0].story.title = "Carried source signal".to_owned();
    let previous_stories = previous
        .items
        .iter()
        .map(|item| item.story.clone())
        .collect::<Vec<_>>();
    Store::open(paths.data_dir.join("signal.sqlite3"))
        .unwrap()
        .commit_refresh(&previous_stories, &previous)
        .unwrap();

    let refresh = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "refresh"])
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(refresh.status.success(), "{:?}", refresh);
    let refresh_json: serde_json::Value = serde_json::from_slice(&refresh.stdout).unwrap();
    let items = refresh_json["data"]["briefing"]["items"]
        .as_array()
        .unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["position"], 1);
    assert_eq!(items[0]["story"]["title"], "Local fixture signal");
    assert_eq!(items[0]["is_stale"], false);
    assert_eq!(items[1]["position"], 2);
    assert_eq!(items[1]["story"]["title"], "Carried source signal");
    assert_eq!(items[1]["is_stale"], true);

    let cached = Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .args(["--json", "today"])
        .output()
        .unwrap();
    assert!(cached.status.success(), "{:?}", cached);
    let cached_json: serde_json::Value = serde_json::from_slice(&cached.stdout).unwrap();
    assert_eq!(cached_json["data"]["is_stale"], false);
    assert_eq!(cached_json["data"]["items"][1]["is_stale"], true);
    assert_eq!(
        cached_json["data"]["items"][1]["story"]["title"],
        "Carried source signal"
    );
    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("today")
        .assert()
        .success()
        .stdout(predicate::str::contains("Carried source signal [stale]"));
}
