use super::*;
use crate::config::ConfigStore;

const HISTORY: &str = r#"{"histories":[],"local_path":null}"#;
const LOCAL: &str = r#"{"chapters":[],"encoding":"UTF-8","current_chapter":1,"line_percent":0.0,"path":"book.txt"}"#;

fn write(home: &Path, relative: &str, bytes: &str) {
    let path = home.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn startup_imports_legacy_fields_and_preserves_old_files() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_home(home.path());
    write(
        home.path(),
        ".novel/appearance.json",
        r#"{"themeSlug":"dracula","background":"theme","showHeader":false}"#,
    );
    write(
        home.path(),
        ".novel/reader-display.json",
        r#"{"followTts":false,"showTitle":false,"pageOverlap":99,"paragraphSpacing":true}"#,
    );
    write(home.path(), ".novel/browser_assist.on", "on");
    write(home.path(), ".novel/history.json", HISTORY);
    write(home.path(), ".novel/local/a.json", LOCAL);
    write(
        home.path(),
        ".novel/browser-profile/Cookies",
        "binary profile",
    );
    assert!(migrate(&paths).unwrap().is_empty());
    let config = ConfigStore::new(paths.config()).load().unwrap();
    assert_eq!(config.appearance.theme_slug, "dracula");
    assert!(!config.appearance.show_header);
    assert!(!config.reader.follow_tts);
    assert!(!config.reader.show_title);
    assert_eq!(config.reader.page_overlap, 10);
    assert!(config.reader.paragraph_spacing);
    assert!(config.browser.always_allow);
    assert_eq!(
        fs::read(paths.local().join("a.json")).unwrap(),
        fs::read(home.path().join(".novel/local/a.json")).unwrap()
    );
    assert_eq!(
        fs::read(paths.browser_profile().join("Cookies")).unwrap(),
        b"binary profile"
    );
    assert!(paths.root().join(MARKER).is_file());
    assert!(home.path().join(".novel/appearance.json").is_file());
    paths.clear().unwrap();
    assert!(migrate(&paths).unwrap().is_empty());
    assert!(!paths.history().exists());
    assert!(!paths.local().exists());
    assert!(paths.root().join(MARKER).is_file());
}

#[test]
fn existing_new_configuration_and_files_win_including_invalid_toml() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_home(home.path());
    write(
        home.path(),
        ".novel/appearance.json",
        "corrupt legacy preference",
    );
    write(home.path(), ".novel/history.json", "corrupt legacy history");
    write(home.path(), ".novel/local/only-old.json", LOCAL);
    write(home.path(), ".trnovel/config.toml", "[do not rewrite");
    write(
        home.path(),
        ".trnovel/data/history.json",
        "new history wins",
    );
    assert!(migrate(&paths).unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(paths.config()).unwrap(),
        "[do not rewrite"
    );
    assert_eq!(
        fs::read_to_string(paths.history()).unwrap(),
        "new history wins"
    );
    assert!(paths.local().join("only-old.json").exists());
}

#[test]
fn corrupt_preferences_and_history_are_reported_without_publication_or_source_deletion() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_home(home.path());
    write(home.path(), ".novel/appearance.json", "invalid JSON");
    write(home.path(), ".novel/history.json", "invalid JSON");
    write(home.path(), ".novel/keybindings.toml", "valid copied bytes");
    let messages = migrate(&paths).unwrap();
    assert_eq!(messages.len(), 2);
    assert!(!paths.config().exists());
    assert!(!paths.history().exists());
    assert!(!paths.root().join(MARKER).exists());
    assert_eq!(
        fs::read_to_string(home.path().join(".novel/history.json")).unwrap(),
        "invalid JSON"
    );
    assert!(paths.keybindings().exists());
    write(
        home.path(),
        ".novel/appearance.json",
        r#"{"themeSlug":"dracula","background":"terminal"}"#,
    );
    write(home.path(), ".novel/history.json", HISTORY);
    assert!(migrate(&paths).unwrap().is_empty());
    assert!(paths.root().join(MARKER).exists());
}

#[test]
fn clear_after_partial_migration_prevents_reimport_but_keeps_preferences_retryable() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_home(home.path());
    write(home.path(), ".novel/appearance.json", "invalid JSON");
    write(home.path(), ".novel/history.json", HISTORY);
    write(home.path(), ".novel/local/book.json", LOCAL);
    assert_eq!(migrate(&paths).unwrap().len(), 1);
    clear(&paths).unwrap();
    assert!(paths.root().join(CLEARED).is_file());
    assert_eq!(migrate(&paths).unwrap().len(), 1);
    assert!(!paths.history().exists());
    assert!(!paths.local().exists());
    write(
        home.path(),
        ".novel/appearance.json",
        r#"{"themeSlug":"dracula","background":"terminal"}"#,
    );
    assert!(migrate(&paths).unwrap().is_empty());
    assert!(paths.config().exists());
    assert!(!paths.history().exists());
}

#[test]
fn malformed_cache_does_not_prevent_other_files_in_the_same_directory() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_home(home.path());
    write(home.path(), ".novel/local/bad.json", "{}");
    write(home.path(), ".novel/local/good.json", LOCAL);
    assert_eq!(migrate(&paths).unwrap().len(), 1);
    assert!(!paths.local().join("bad.json").exists());
    assert!(paths.local().join("good.json").is_file());
}

#[test]
fn missing_legacy_home_creates_nothing_and_concurrent_migrations_do_not_clobber() {
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_home(home.path());
    assert!(migrate(&paths).unwrap().is_empty());
    assert!(!paths.root().exists());
    write(home.path(), ".novel/history.json", HISTORY);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let paths = &paths;
            scope.spawn(move || assert!(migrate(paths).unwrap().is_empty()));
        }
    });
    assert_eq!(fs::read_to_string(paths.history()).unwrap(), HISTORY);
}

#[cfg(unix)]
#[test]
fn source_links_are_skipped_and_destination_directory_links_are_not_followed() {
    use std::os::unix::fs::symlink;
    let home = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_home(home.path());
    write(home.path(), ".novel/local/book.json", LOCAL);
    symlink(external.path(), home.path().join(".novel/browser-profile")).unwrap();
    fs::create_dir_all(paths.root()).unwrap();
    symlink(external.path(), paths.data()).unwrap();
    assert!(!migrate(&paths).unwrap().is_empty());
    assert!(!external.path().join("local").exists());
    fs::remove_file(paths.data()).unwrap();
    assert!(migrate(&paths).unwrap().is_empty());
    assert!(!paths.browser_profile().exists());
    assert!(paths.local().join("book.json").exists());
}

#[cfg(unix)]
#[test]
fn legacy_home_link_is_not_followed() {
    let home = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(external.path(), home.path().join(".novel")).unwrap();
    let paths = AppPaths::from_home(home.path());
    assert!(migrate(&paths).is_err());
    assert!(!paths.root().exists());
}

#[cfg(unix)]
#[test]
fn unreadable_legacy_data_is_preserved_and_reported_for_retry() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_home(home.path());
    write(home.path(), ".novel/history.json", HISTORY);
    let source = home.path().join(".novel/history.json");
    let permissions = fs::metadata(&source).unwrap().permissions();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o000)).unwrap();
    let unreadable = File::open(&source).is_err();
    let result = migrate(&paths);
    fs::set_permissions(&source, permissions).unwrap();
    if unreadable {
        assert_eq!(result.unwrap().len(), 1);
        assert!(!paths.history().exists());
        assert!(!paths.root().join(MARKER).exists());
        assert!(migrate(&paths).unwrap().is_empty());
    }
    assert_eq!(fs::read_to_string(&source).unwrap(), HISTORY);
}
