//! Opt-in checks of the real rendered settings and worker-backed adjustment callbacks.
use super::*;
use crate::tts::{Handle, Snapshot};
use ratatui_kit::test_util::render_frames;
use tokio::sync::watch;

async fn configuration(watched: &mut watch::Receiver<Snapshot>, revision: Option<u64>) -> Snapshot {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let value = watched.borrow().clone();
            assert!(value.error.is_none(), "{:?}", value.error);
            if value
                .config
                .as_ref()
                .is_some_and(|config| revision.is_none_or(|revision| config.revision > revision))
            {
                return value;
            }
            watched.changed().await.unwrap();
        }
    })
    .await
    .expect("worker configuration timeout")
}

#[derive(Props)]
struct SceneProps {
    handle: Handle,
    snapshot: Snapshot,
    adjustment: Option<(Setting, bool)>,
}

#[component]
fn Scene(props: &SceneProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let context = TtsContext {
        handle: props.handle.clone(),
        snapshot: hooks.use_state(|| props.snapshot.clone()),
    };
    hooks.use_effect(
        {
            let context = context.clone();
            let adjustment = props.adjustment;
            move || {
                if let Some((kind, increase)) = adjustment {
                    change(&context, kind, increase);
                }
            }
        },
        (),
    );
    element!(PaletteProvider(palette:Palette::default()) {
        ContextProvider(value:Context::owned(context)) {
            TTSManager(open:true,is_editing:true)
        }
    })
}

fn rendered(handle: &Handle, snapshot: &Snapshot, adjustment: Option<(Setting, bool)>) -> String {
    let buffer = render_frames(
        element!(Scene(
            ..SceneProps {
                handle: handle.clone(),
                snapshot: snapshot.clone(),
                adjustment,
            }
        )),
        160,
        80,
        3,
    );
    // Wide CJK glyphs occupy a second blank cell in the terminal buffer.
    buffer
        .content
        .iter()
        .flat_map(|cell| cell.symbol().chars())
        .filter(|character| !character.is_whitespace())
        .collect()
}

async fn adjust(
    handle: &Handle,
    current: &mut Snapshot,
    watched: &mut watch::Receiver<Snapshot>,
    kind: Setting,
    increase: bool,
) -> Snapshot {
    let revision = current.config.as_ref().unwrap().revision;
    rendered(handle, current, Some((kind, increase)));
    let snapshot = configuration(watched, Some(revision)).await;
    *current = snapshot.clone();
    snapshot
}

#[tokio::test]
async fn moss_models_switch_between_nano_and_gpu_without_reusing_voice_or_device() {
    let Some(worker) = std::env::var_os("TRNOVEL_MOSS_CANDLE_TUI_WORKER") else {
        return;
    };
    let (handle, task) = Handle::new();
    handle.set_path(Some(worker.into()));
    let mut watched = handle.watch();
    handle.open();
    let mut snapshot = configuration(&mut watched, None).await;
    let revision = snapshot.config.as_ref().unwrap().revision;
    handle.update(ConfigPatch {
        expected_revision: revision,
        backend: Some("moss".into()),
        model: Some("nano".into()),
        voice: Some("Weiguo".into()),
        tts_device: Some(tts_protocol::Device::Cpu),
        style: Some(String::new()),
        ..Default::default()
    });
    snapshot = configuration(&mut watched, Some(revision)).await;
    assert!(rendered(&handle, &snapshot, None).contains("MOSSNano"));
    let local = adjust(&handle, &mut snapshot, &mut watched, Setting::Model, true).await;
    assert_eq!(
        local.config.as_ref().unwrap().model.as_deref(),
        Some("local-1.7b")
    );
    assert_eq!(
        local.config.as_ref().unwrap().tts_device,
        tts_protocol::Device::Auto
    );
    assert_eq!(local.config.as_ref().unwrap().voice, "narrator");
    assert!(rendered(&handle, &snapshot, None).contains("MOSSLocal1.7B"));
    let realtime = adjust(&handle, &mut snapshot, &mut watched, Setting::Model, true).await;
    assert_eq!(
        realtime.config.as_ref().unwrap().model.as_deref(),
        Some("realtime-1.7b")
    );
    assert!(rendered(&handle, &snapshot, None).contains("MOSSRealtime1.7B"));
    let voice = adjust(&handle, &mut snapshot, &mut watched, Setting::Voice, true).await;
    assert_eq!(voice.config.as_ref().unwrap().voice, "custom:candle_trial");
    adjust(&handle, &mut snapshot, &mut watched, Setting::Model, false).await;
    let nano = adjust(&handle, &mut snapshot, &mut watched, Setting::Model, false).await;
    assert_eq!(nano.config.as_ref().unwrap().model.as_deref(), Some("nano"));
    assert_eq!(nano.config.as_ref().unwrap().voice, "Weiguo");
    assert!(rendered(&handle, &snapshot, None).contains("MOSSNano"));
    handle.shutdown().await;
    task.await.unwrap();
}

#[tokio::test]
async fn experimental_voxcpm_model_is_labelled_and_can_switch_back() {
    let Some(worker) = std::env::var_os("TRNOVEL_VOX_EXPERIMENT_TUI_WORKER") else {
        return;
    };
    let (handle, task) = Handle::new();
    handle.set_path(Some(worker.into()));
    let mut watched = handle.watch();
    handle.open();
    let mut snapshot = configuration(&mut watched, None).await;
    let revision = snapshot.config.as_ref().unwrap().revision;
    handle.update(ConfigPatch {
        expected_revision: revision,
        backend: Some("voxcpm".into()),
        model: Some("2b-q8_0".into()),
        voice: Some("narrator".into()),
        tts_device: Some(tts_protocol::Device::Cpu),
        ..Default::default()
    });
    snapshot = configuration(&mut watched, Some(revision)).await;
    let trial = adjust(&handle, &mut snapshot, &mut watched, Setting::Model, true).await;
    assert_eq!(
        trial.config.as_ref().unwrap().model.as_deref(),
        Some("2b-bf16")
    );
    assert_eq!(
        trial.config.as_ref().unwrap().tts_device,
        tts_protocol::Device::Auto
    );
    assert_eq!(trial.config.as_ref().unwrap().voice, "narrator");
    assert_eq!(
        trial.capabilities.as_ref().unwrap().compiled_devices,
        vec![tts_protocol::Device::Cuda]
    );
    let display = rendered(&handle, &snapshot, None);
    assert!(display.contains("原始BF16（实验）"));
    assert!(display.contains("验收待完成"));
    let restored = adjust(&handle, &mut snapshot, &mut watched, Setting::Model, false).await;
    assert_eq!(
        restored.config.as_ref().unwrap().model.as_deref(),
        Some("2b-q8_0")
    );
    assert!(!rendered(&handle, &snapshot, None).contains("验收待完成"));
    handle.shutdown().await;
    task.await.unwrap();
}

#[tokio::test]
async fn real_worker_model_voice_and_backend_changes_follow_rendered_capabilities() {
    let Some(worker) = std::env::var_os("TRNOVEL_TTS_TUI_WORKER") else {
        return;
    };
    // The supplied worker/proxy owns an isolated config and a prepared voice directory.
    let (handle, task) = Handle::new();
    handle.set_path(Some(worker.into()));
    let mut watched = handle.watch();
    handle.open();
    let mut snapshot = configuration(&mut watched, None).await;
    let revision = snapshot.config.as_ref().unwrap().revision;
    handle.update(ConfigPatch {
        expected_revision: revision,
        backend: Some("qwen".into()),
        model: Some("0.6b-customvoice".into()),
        voice: Some("uncle_fu".into()),
        tts_device: Some(tts_protocol::Device::Cpu),
        style: Some(String::new()),
        ..Default::default()
    });
    snapshot = configuration(&mut watched, Some(revision)).await;
    assert_eq!(snapshot.config.as_ref().unwrap().backend, "qwen");
    assert_eq!(
        snapshot.config.as_ref().unwrap().model.as_deref(),
        Some("0.6b-customvoice")
    );
    assert!(rendered(&handle, &snapshot, None).contains("0.6B"));
    let custom = adjust(&handle, &mut snapshot, &mut watched, Setting::Model, true).await;
    assert_eq!(
        custom.config.as_ref().unwrap().model.as_deref(),
        Some("1.7b-customvoice")
    );
    assert!(custom.capabilities.as_ref().unwrap().style);
    assert!(rendered(&handle, &snapshot, None).contains("朗读风格"));
    let styled = adjust(&handle, &mut snapshot, &mut watched, Setting::Style, true).await;
    assert!(styled.config.as_ref().unwrap().style.is_some());
    let base = adjust(&handle, &mut snapshot, &mut watched, Setting::Model, true).await;
    assert_eq!(
        base.config.as_ref().unwrap().model.as_deref(),
        Some("1.7b-base")
    );
    assert!(base.capabilities.as_ref().unwrap().cloning);
    assert!(base.config.as_ref().unwrap().style.is_none());
    assert!(!rendered(&handle, &snapshot, None).contains("朗读风格"));
    let voice = base.config.as_ref().unwrap().voice.clone();
    let changed = adjust(&handle, &mut snapshot, &mut watched, Setting::Voice, true).await;
    assert_ne!(changed.config.as_ref().unwrap().voice, voice);
    adjust(&handle, &mut snapshot, &mut watched, Setting::Model, false).await;
    // Qwen is the final backend; increasing at the boundary must preserve its 1.7B model.
    let before = snapshot.config.clone();
    rendered(&handle, &snapshot, Some((Setting::Backend, true)));
    tokio::task::yield_now().await;
    assert_eq!(watched.borrow().config, before);
    let omni = adjust(
        &handle,
        &mut snapshot,
        &mut watched,
        Setting::Backend,
        false,
    )
    .await;
    assert_eq!(omni.config.as_ref().unwrap().backend, "omnivoice");
    assert!(!omni.capabilities.as_ref().unwrap().native_streaming);
    assert_eq!(omni.config.as_ref().unwrap().voice, "narrator");
    let cloned = adjust(&handle, &mut snapshot, &mut watched, Setting::Voice, true).await;
    assert!(cloned.config.as_ref().unwrap().voice.starts_with("custom:"));
    assert!(rendered(&handle, &snapshot, None).contains("验收"));
    handle.shutdown().await;
    task.await.unwrap();
}
