use std::{fs, process::Command};

fn legacy(home: &std::path::Path) {
    fs::create_dir_all(home.join(".novel")).unwrap();
    fs::write(
        home.join(".novel/history.json"),
        r#"{"histories":[],"local_path":null}"#,
    )
    .unwrap();
}

#[test]
fn help_and_version_do_not_migrate_or_create_new_home() {
    let home = tempfile::tempdir().unwrap();
    legacy(home.path());
    for argument in ["--help", "--version"] {
        let output = Command::new(env!("CARGO_BIN_EXE_trn"))
            .arg(argument)
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!home.path().join(".trnovel").exists());
    }
    assert!(home.path().join(".novel/history.json").exists());
}

#[test]
fn first_clear_migrates_then_clears_without_reimport_on_the_next_start() {
    let home = tempfile::tempdir().unwrap();
    legacy(home.path());
    for _ in 0..2 {
        let output = Command::new(env!("CARGO_BIN_EXE_trn"))
            .arg("clear")
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!home.path().join(".trnovel/data/history.json").exists());
        assert!(home.path().join(".trnovel/.legacy-migration-v1").is_file());
        assert!(
            home.path()
                .join(".trnovel/.legacy-reading-cleared-v1")
                .is_file()
        );
    }
    assert!(home.path().join(".novel/history.json").is_file());
}
