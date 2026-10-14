use super::*;
use ratatui_kit::test_util::render_frames;

fn catalog() -> Snapshot {
    let caps = tts_protocol::Capabilities {
        backend: "moss".into(),
        model: Some("nano".into()),
        model_name: "MOSS Nano".into(),
        default_voice: "A".into(),
        voices: vec!["A".into(), "B".into()],
        voice_names: [("A".into(), "甲".into()), ("B".into(), "乙".into())].into(),
        compiled_devices: vec![Device::Cpu],
        native_streaming: true,
        style: false,
        cloning: false,
        pronunciation: false,
        continuation: true,
        parameters: vec![],
    };
    let mut qwen = caps.clone();
    qwen.backend = "qwen".into();
    qwen.model = Some("custom".into());
    qwen.model_name = "Qwen Custom".into();
    qwen.style = true;
    qwen.compiled_devices = vec![Device::Metal];
    let config = tts_protocol::Config {
        backend: "moss".into(),
        model: Some("nano".into()),
        voice: "A".into(),
        ..Default::default()
    };
    let mut snapshot = Snapshot::default();
    snapshot.config = Some(config.clone());
    snapshot.sound = Some(SoundConfig::from(&config));
    snapshot.backends = vec![caps, qwen];
    snapshot
}
#[test]
fn live_filter_matches_display_names_ids_and_models_without_changing_selection() {
    let snapshot = catalog();
    assert_eq!(
        picker::choices(&snapshot, PickerKind::Model, "QWEN").len(),
        1
    );
    assert_eq!(picker::choices(&snapshot, PickerKind::Voice, "乙").len(), 1);
    assert_eq!(picker::choices(&snapshot, PickerKind::Voice, "b").len(), 1);
    assert!(picker::choices(&snapshot, PickerKind::Model, "absent").is_empty());
    assert_eq!(snapshot.sound.unwrap().voice, "A");
}
#[test]
fn sound_rows_follow_pending_model_capabilities_and_remove_alignment() {
    let mut snapshot = catalog();
    assert!(
        !rows(&snapshot, Panel::Sound)
            .iter()
            .any(|row| row.label == "朗读风格")
    );
    snapshot.sound.as_mut().unwrap().backend = "qwen".into();
    snapshot.sound.as_mut().unwrap().model = Some("custom".into());
    assert!(
        rows(&snapshot, Panel::Sound)
            .iter()
            .any(|row| row.label == "朗读风格")
    );
    let sound_rows = rows(&snapshot, Panel::Sound);
    let apply = sound_rows
        .iter()
        .position(|row| row.label == "应用并重播")
        .unwrap();
    assert!(
        sound_rows
            .iter()
            .position(|row| row.label == "从头重听")
            .is_some_and(|replay| replay == apply + 1)
    );
    assert_eq!(rows(&snapshot, Panel::Playback).len(), 6);
    assert!(
        rows(&snapshot, Panel::Playback)
            .iter()
            .any(|row| row.label == "播放策略" && row.value == "流式播放")
    );
    for panel in [Panel::Playback, Panel::Sound, Panel::Advanced] {
        assert!(
            !rows(&snapshot, panel)
                .iter()
                .any(|row| row.label.contains("对齐") || row.label == "逐句高亮")
        );
    }
}
#[derive(Props)]
struct SceneProps {
    handle: super::super::Handle,
    snapshot: Snapshot,
}
#[component]
fn Scene(props: &SceneProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let context = TtsContext {
        handle: props.handle.clone(),
        snapshot: hooks.use_state(|| props.snapshot.clone()),
    };
    element!(PaletteProvider(palette:Palette::default()) {ContextProvider(value:Context::owned(context)) {
        picker::SoundPicker(kind:Some(PickerKind::Model))
    }})
}
fn text(buffer: &ratatui::buffer::Buffer) -> String {
    buffer
        .content
        .iter()
        .flat_map(|cell| cell.symbol().chars())
        .filter(|c| !c.is_whitespace())
        .collect()
}
#[tokio::test]
async fn actual_picker_renders_at_normal_and_small_terminal_sizes() {
    let (handle, task) = super::super::Handle::new();
    for (width, height) in [(100, 32), (52, 18)] {
        let display = render_frames(
            element!(Scene(
                ..SceneProps {
                    handle: handle.clone(),
                    snapshot: catalog(),
                }
            )),
            width,
            height,
            3,
        );
        assert!(text(&display).contains("搜索名称或ID"));
        assert!(text(&display).contains("MOSSNano"));
    }
    handle.shutdown().await;
    task.await.unwrap();
}
