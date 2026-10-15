use std::{fs, process::Command};

fn legacy(home: &std::path::Path) {
    fs::create_dir_all(home.join(".novel")).unwrap();
    fs::write(
        home.join(".novel/history.json"),
        r#"{"histories":[],"local_path":null}"#,
    )
    .unwrap();
}

fn trn(home: &std::path::Path, argument: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_trn"))
        .arg(argument)
        .env("HOME", home)
        .output()
        .unwrap()
}

#[test]
fn help_and_version_do_not_migrate_or_create_new_home() {
    let home = tempfile::tempdir().unwrap();
    legacy(home.path());
    for argument in ["--help", "--version"] {
        let output = trn(home.path(), argument);
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
fn clear_does_not_migrate_and_stays_authoritative_for_later_explicit_migrate() {
    let home = tempfile::tempdir().unwrap();
    legacy(home.path());
    for _ in 0..2 {
        let output = trn(home.path(), "clear");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // clear 不再触发自动迁移:只写清理标记,不导入阅读数据。
        assert!(!home.path().join(".trnovel/data/history.json").exists());
        assert!(!home.path().join(".trnovel/.legacy-migration-v1").exists());
        assert!(
            home.path()
                .join(".trnovel/.legacy-reading-cleared-v1")
                .is_file()
        );
    }
    // 之后显式无头迁移:清理标记抑制阅读数据重导入,但迁移仍能完成。
    let output = Command::new(env!("CARGO_BIN_EXE_trn"))
        .args(["migrate", "--yes"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("迁移完成"), "{stdout}");
    assert!(!home.path().join(".trnovel/data/history.json").exists());
    assert!(home.path().join(".trnovel/.legacy-migration-v1").is_file());
    assert!(home.path().join(".novel/history.json").is_file());
}

#[test]
fn headless_migrate_imports_once_and_reports_needs_nothing_afterwards() {
    let home = tempfile::tempdir().unwrap();
    legacy(home.path());
    let output = Command::new(env!("CARGO_BIN_EXE_trn"))
        .args(["migrate", "--yes"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("迁移完成"), "{stdout}");
    assert!(home.path().join(".trnovel/data/history.json").is_file());
    assert!(home.path().join(".trnovel/.legacy-migration-v1").is_file());
    assert!(home.path().join(".novel/history.json").is_file());

    let output = Command::new(env!("CARGO_BIN_EXE_trn"))
        .args(["migrate", "--yes"])
        .env("HOME", home.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("无需迁移"), "{stdout}");
}
