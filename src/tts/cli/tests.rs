use super::*;
use clap::Parser;

#[test]
fn clap_routes_nested_commands_and_checks_flags() {
    use crate::TRNovel;
    for args in [
        vec!["trn", "tts", "show"],
        vec!["trn", "tts", "list", "--backend", "moss", "--model", "nano"],
        vec![
            "trn",
            "tts",
            "setup",
            "--program",
            "worker",
            "--voice",
            "A",
            "--device",
            "cpu",
        ],
        vec!["trn", "--tts-program", "worker", "tts", "setup"],
    ] {
        assert!(TRNovel::try_parse_from(args).is_ok());
    }
    for args in [
        vec!["trn", "tts"],
        vec!["trn", "tts", "list", "--model", "nano"],
        vec!["trn", "tts", "setup", "--device", "typo"],
        vec![
            "trn",
            "tts",
            "setup",
            "--program",
            "b",
            "--tts-program",
            "a",
        ],
    ] {
        assert!(TRNovel::try_parse_from(&args).is_err(), "{args:?}");
    }
}

#[tokio::test]
async fn setup_persists_reader_only_and_rejects_invalid_values() {
    let (fixture, program) = crate::tts::client::tests::fixture("cli");
    let store = ConfigStore::new(fixture.path().join("config.toml"));
    store
        .update(|config| {
            config.reader.follow_tts = true;
            config.browser.always_allow = true;
        })
        .unwrap();
    let args = SetupArgs {
        program: Some(program.clone()),
        backend: Some("moss".into()),
        model: Some("nano".into()),
        voice: Some("B".into()),
        device: Some(SetupDevice::Cpu),
        ..Default::default()
    };
    execute(&TtsCommand::Setup(args.clone()), None, &store)
        .await
        .unwrap();
    let config = store.load().unwrap();
    assert!(config.reader.follow_tts && config.browser.always_allow);
    assert_eq!(config.tts.program, Some(program.canonicalize().unwrap()));
    assert_eq!(config.tts.sound.as_ref().unwrap().voice, "B");
    let original = std::fs::read(fixture.path().join("config.toml")).unwrap();
    for invalid in [
        SetupArgs {
            voice: Some("missing".into()),
            ..Default::default()
        },
        SetupArgs {
            device: Some(SetupDevice::Metal),
            ..Default::default()
        },
        SetupArgs {
            model: Some("missing".into()),
            ..Default::default()
        },
        SetupArgs {
            style: Some("不支持".into()),
            ..Default::default()
        },
    ] {
        assert!(
            execute(&TtsCommand::Setup(invalid), None, &store)
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read(fixture.path().join("config.toml")).unwrap(),
            original
        );
    }
    let log = std::fs::read_to_string(program.with_extension("requests.jsonl")).unwrap();
    let commands: Vec<tts_protocol::Request> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(commands.iter().all(|request| matches!(
        request.command,
        Command::Hello | Command::GetConfig | Command::Shutdown
    )));
    let (client, ready) = Client::connect(&program).await.unwrap();
    let Event::Ready(catalog) = ready.event else {
        panic!()
    };
    client.close().await;
    let switched = SetupArgs {
        backend: Some("qwen".into()),
        model: Some("custom".into()),
        ..Default::default()
    }
    .sound(config.tts.sound.as_ref().unwrap(), &catalog)
    .unwrap();
    assert_eq!(switched.voice, "C");
    assert_eq!(switched.device, Device::Auto);
    assert_eq!(switched.style, None);
}

#[tokio::test]
async fn read_only_commands_and_failed_workers_do_not_write_config() {
    let (fixture, program) = crate::tts::client::tests::fixture("cli-read-only");
    let path = fixture.path().join("config.toml");
    let store = ConfigStore::new(path.clone());
    execute(&TtsCommand::Show, None, &store).await.unwrap();
    execute(
        &TtsCommand::List {
            backend: None,
            model: None,
        },
        Some(&program),
        &store,
    )
    .await
    .unwrap();
    assert!(!path.exists());
    let (_bad, bad) = crate::tts::client::tests::fixture("incompatible-cli");
    assert!(
        execute(
            &TtsCommand::Setup(SetupArgs {
                program: Some(bad),
                ..Default::default()
            }),
            None,
            &store
        )
        .await
        .is_err()
    );
    assert!(!path.exists());
    assert!(
        execute(&TtsCommand::Setup(SetupArgs::default()), None, &store)
            .await
            .is_err()
    );
    assert!(!path.exists());
    let configured = SetupArgs {
        program: Some(program.clone()),
        ..Default::default()
    };
    assert!(
        execute(
            &TtsCommand::Setup(configured.clone()),
            Some(&program),
            &store
        )
        .await
        .is_err()
    );
    assert!(!path.exists());
    execute(&TtsCommand::Setup(configured), None, &store)
        .await
        .unwrap();
    assert!(store.load().unwrap().tts.sound.is_none());
    assert_eq!(
        store.load().unwrap().tts.program,
        Some(program.canonicalize().unwrap())
    );
}

#[test]
fn default_model_resolution_keeps_voice_and_rejected_transactions_do_not_write() {
    let caps: Capabilities = serde_json::from_str(r#"{"backend":"moss","model":"nano","model_name":"Nano","voices":["A","B"],"default_voice":"A","compiled_devices":["cpu"],"native_streaming":true,"style":false,"cloning":false,"pronunciation":false}"#).unwrap();
    let base = SoundConfig {
        backend: "moss".into(),
        model: None,
        voice: "B".into(),
        style: None,
        device: Device::Cpu,
    };
    let sound = SetupArgs {
        device: Some(SetupDevice::Cpu),
        ..Default::default()
    }
    .sound(&base, &[caps])
    .unwrap();
    assert_eq!(sound.voice, "B");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let original = "# preserve comments on rejected edits\n[tts]\nprogram = 'before'\n";
    std::fs::write(&path, original).unwrap();
    let store = ConfigStore::new(path.clone());
    assert!(
        store
            .try_update(|config| {
                config.tts.program = Some("after".into());
                anyhow::bail!("stale edit");
            })
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[tokio::test]
async fn setup_normalizes_blank_styles_and_rejects_long_styles_without_writing() {
    let (fixture, program) = crate::tts::client::tests::fixture("cli-style");
    let path = fixture.path().join("config.toml");
    let store = ConfigStore::new(path.clone());
    let args = SetupArgs {
        program: Some(program),
        backend: Some("qwen".into()),
        model: Some("custom".into()),
        style: Some("温".repeat(200)),
        ..Default::default()
    };
    execute(&TtsCommand::Setup(args.clone()), None, &store)
        .await
        .unwrap();
    let original = std::fs::read(&path).unwrap();
    assert!(
        execute(
            &TtsCommand::Setup(SetupArgs {
                style: Some("温".repeat(201)),
                ..args.clone()
            }),
            None,
            &store,
        )
        .await
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    execute(
        &TtsCommand::Setup(SetupArgs {
            style: Some(" \t\n".into()),
            ..args
        }),
        None,
        &store,
    )
    .await
    .unwrap();
    assert_eq!(store.load().unwrap().tts.sound.unwrap().style, None);
}
