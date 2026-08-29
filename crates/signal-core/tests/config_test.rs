use signal_core::{AppPaths, ConfigRepository, SourceKind};

#[test]
fn first_load_writes_the_standard_source_pack() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(temp.path());
    let config = ConfigRepository::new(paths).load_or_create().unwrap();

    assert!(!config.sources.is_empty());
    assert!(config.sources.iter().all(|source| source.enabled));
    assert!(matches!(config.sources[0].kind, SourceKind::Feed { .. }));
}

#[test]
fn saved_source_overrides_survive_reload() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(temp.path());
    let repo = ConfigRepository::new(paths);
    let mut config = repo.load_or_create().unwrap();
    config.sources[0].enabled = false;
    repo.save(&config).unwrap();

    assert!(!repo.load_or_create().unwrap().sources[0].enabled);
}
