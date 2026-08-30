use std::sync::{Mutex, MutexGuard, OnceLock};

use signal_core::{
    AppPaths, ConfigRepository, NewFeedSource, SignalApp, SignalError, Source, SourceKind,
    SourceOrigin,
};
use uuid::Uuid;

static SIGNAL_HOME_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct SourceFixture {
    _signal_home_lock: MutexGuard<'static, ()>,
    _root: tempfile::TempDir,
    paths: AppPaths,
}

impl SourceFixture {
    fn new() -> Self {
        let signal_home_lock = SIGNAL_HOME_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = tempfile::tempdir().unwrap();
        let paths = AppPaths::for_root(root.path());
        // SAFETY: this fixture holds the process-wide test lock until it is dropped, and this
        // integration-test process does not spawn threads that read its environment.
        unsafe { std::env::set_var("SIGNAL_HOME", root.path()) };
        ConfigRepository::new(paths.clone())
            .load_or_create()
            .unwrap();
        Self {
            _signal_home_lock: signal_home_lock,
            _root: root,
            paths,
        }
    }

    fn open_app(&self) -> SignalApp {
        SignalApp::open().unwrap()
    }

    fn config_bytes(&self) -> Vec<u8> {
        std::fs::read(self.paths.config_dir.join("config.toml")).unwrap()
    }

    fn prevent_config_writes(&self) {
        std::fs::remove_dir_all(&self.paths.config_dir).unwrap();
        std::fs::write(&self.paths.config_dir, "not a directory").unwrap();
    }
}

fn personal_feed(name: &str) -> NewFeedSource {
    NewFeedSource {
        name: name.to_owned(),
        category: "research".to_owned(),
        url: "https://example.com/personal.xml".to_owned(),
        weight: 0.75,
        enabled: true,
    }
}

#[test]
fn personal_feed_round_trips_and_is_visible_to_another_app() {
    // Break caught: accepting a feed without persisting it for a newly opened app.
    let fixture = SourceFixture::new();
    let mut first = fixture.open_app();
    let added = first
        .add_feed_source(personal_feed("Personal research"))
        .unwrap();
    assert_eq!(added.origin, SourceOrigin::Personal);
    assert!(added.source.id.starts_with("personal-"));
    Uuid::parse_str(added.source.id.strip_prefix("personal-").unwrap()).unwrap();

    let mut second = fixture.open_app();
    assert!(second.list_source_records().unwrap().contains(&added));
}

#[test]
fn invalid_personal_feed_is_rejected_before_writing_config() {
    // Break caught: accepting malformed source input or modifying config before validation.
    let invalid_inputs = [
        NewFeedSource {
            name: " ".to_owned(),
            ..personal_feed("Personal")
        },
        NewFeedSource {
            category: "\t".to_owned(),
            ..personal_feed("Personal")
        },
        NewFeedSource {
            weight: f64::NAN,
            ..personal_feed("Personal")
        },
        NewFeedSource {
            weight: -0.01,
            ..personal_feed("Personal")
        },
        NewFeedSource {
            weight: 1.01,
            ..personal_feed("Personal")
        },
        NewFeedSource {
            url: "ftp://example.com/feed.xml".to_owned(),
            ..personal_feed("Personal")
        },
        NewFeedSource {
            url: "https://".to_owned(),
            ..personal_feed("Personal")
        },
        NewFeedSource {
            url: "https://user@example.com/feed.xml".to_owned(),
            ..personal_feed("Personal")
        },
    ];

    for input in invalid_inputs {
        let fixture = SourceFixture::new();
        let before = fixture.config_bytes();

        let error = fixture.open_app().add_feed_source(input).unwrap_err();

        assert!(matches!(error, SignalError::InvalidConfiguration(_)));
        assert_eq!(fixture.config_bytes(), before);
    }
}

#[test]
fn empty_url_user_info_is_rejected_before_writing_config() {
    // Break caught: accepting an authority with an explicit but empty user-info component.
    let fixture = SourceFixture::new();
    let before = fixture.config_bytes();
    let input = NewFeedSource {
        url: "https://@example.com/feed.xml".to_owned(),
        ..personal_feed("Personal")
    };

    let error = fixture.open_app().add_feed_source(input).unwrap_err();

    assert!(matches!(error, SignalError::InvalidConfiguration(_)));
    assert_eq!(fixture.config_bytes(), before);
}

#[test]
fn personal_feed_names_are_case_insensitively_unique() {
    // Break caught: allowing duplicate feeds that differ only by name casing.
    let fixture = SourceFixture::new();
    let mut app = fixture.open_app();
    app.add_feed_source(personal_feed("Personal research"))
        .unwrap();
    let before = fixture.config_bytes();

    let error = app
        .add_feed_source(personal_feed("PERSONAL RESEARCH"))
        .unwrap_err();

    assert!(matches!(error, SignalError::InvalidConfiguration(_)));
    assert_eq!(fixture.config_bytes(), before);
}

#[test]
fn only_bundled_source_ids_are_standard() {
    // Break caught: classifying every non-personal-prefix source as bundled standard.
    let fixture = SourceFixture::new();
    let repository = ConfigRepository::new(fixture.paths.clone());
    let mut config = repository.load().unwrap();
    config.sources.push(Source {
        id: "legacy-custom-feed".to_owned(),
        name: "Legacy custom feed".to_owned(),
        category: "research".to_owned(),
        enabled: true,
        weight: 0.5,
        kind: SourceKind::Feed {
            url: "https://example.com/legacy.xml".to_owned(),
        },
    });
    repository.save(&config).unwrap();
    let mut app = fixture.open_app();

    let legacy = app
        .list_source_records()
        .unwrap()
        .into_iter()
        .find(|record| record.source.id == "legacy-custom-feed")
        .unwrap();

    assert_eq!(legacy.origin, SourceOrigin::Personal);
    assert_eq!(
        app.remove_personal_source("legacy-custom-feed").unwrap(),
        legacy
    );
}

#[test]
fn standard_sources_cannot_be_removed_and_missing_sources_are_not_found() {
    // Break caught: removing a bundled source or misreporting a missing source.
    let fixture = SourceFixture::new();
    let mut app = fixture.open_app();
    let standard_id = app
        .list_source_records()
        .unwrap()
        .into_iter()
        .find(|record| record.origin == SourceOrigin::Standard)
        .unwrap()
        .source
        .id;

    assert!(matches!(
        app.remove_personal_source(&standard_id),
        Err(SignalError::InvalidConfiguration(_))
    ));
    assert!(matches!(
        app.remove_personal_source("missing-source"),
        Err(SignalError::NotFound(_))
    ));
}

#[test]
fn failed_atomic_write_leaves_added_source_out_of_memory_config() {
    // Break caught: replacing the in-memory config before its atomic write succeeds.
    let fixture = SourceFixture::new();
    let mut app = fixture.open_app();
    let before = app.list_sources();
    fixture.prevent_config_writes();

    assert!(
        app.add_feed_source(personal_feed("Personal research"))
            .is_err()
    );

    assert_eq!(app.list_sources(), before);
}

#[test]
fn failed_atomic_write_leaves_source_enabled_state_unchanged() {
    // Break caught: changing source state in memory before its atomic write succeeds.
    let fixture = SourceFixture::new();
    let mut app = fixture.open_app();
    let before = app.list_sources();
    let source = before.first().unwrap();
    fixture.prevent_config_writes();

    assert!(app.set_source_enabled(&source.id, !source.enabled).is_err());

    assert_eq!(app.list_sources(), before);
}

#[test]
fn failed_atomic_write_keeps_personal_source_in_memory_list() {
    // Break caught: removing a personal source from memory before its config write succeeds.
    let fixture = SourceFixture::new();
    let mut app = fixture.open_app();
    let added = app
        .add_feed_source(personal_feed("Personal research"))
        .unwrap();
    let before = app.list_sources();
    fixture.prevent_config_writes();

    assert!(app.remove_personal_source(&added.source.id).is_err());

    assert_eq!(app.list_sources(), before);
}
