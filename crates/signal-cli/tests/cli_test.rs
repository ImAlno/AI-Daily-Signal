use assert_cmd::Command;
use predicates::prelude::*;
use signal_core::{AppPaths, ConfigRepository, Source, SourceKind, Store};

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
        .args(["--json", "refresh"])
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["successful_sources"], 1);
    assert_eq!(value["data"]["failures"].as_array().unwrap().len(), 0);
    assert_eq!(
        value["data"]["briefing"]["items"][0]["story"]["title"],
        "Local fixture signal"
    );

    Command::cargo_bin("signal")
        .unwrap()
        .env("SIGNAL_HOME", home.path())
        .arg("today")
        .assert()
        .success()
        .stdout(predicate::str::contains("Local fixture signal"));
}
