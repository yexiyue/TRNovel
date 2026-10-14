//! UI-facing actor: starts on explicit actions and filters stale playback events.
use super::ChapterRequest;
use super::client::{Client, Received, discover};
use crate::config::{ConfigStore, SoundConfig};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tts_protocol::{
    Capabilities, Command, Config, ConfigPatch, EndReason, Event, SessionState, SourceId, TextRange,
};

fn download_progress(resource: &str, downloaded: u64, total: u64) -> String {
    let name = resource.rsplit('/').next().unwrap_or(resource);
    let mib = 1024.0 * 1024.0;
    if total == 0 {
        return format!("下载 {name} · {:.1} MiB", downloaded as f64 / mib);
    }
    let fraction = (downloaded as f64 / total as f64).clamp(0.0, 1.0);
    let filled = (fraction * 10.0) as usize;
    let bar = format!("{}{}", "━".repeat(filled), "─".repeat(10 - filled));
    format!(
        "下载 {name}\n[{bar}] {:.1}% · {:.1} / {:.1} MiB",
        fraction * 100.0,
        downloaded as f64 / mib,
        total as f64 / mib
    )
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub config: Option<Config>,
    pub sound: Option<SoundConfig>,
    pub playback_sound: Option<SoundConfig>,
    /// Reader-side chapter strategy applied when the next session starts.
    pub playback: crate::config::PlaybackPreference,
    pub backends: Vec<Capabilities>,
    pub state: SessionState,
    pub source: Option<SourceId>,
    pub text_hash: Option<String>,
    pub range: Option<TextRange>,
    pub terminal: Option<EndReason>,
    pub terminal_revision: u64,
    pub error: Option<String>,
    pub progress: String,
    pub model_ready: bool,
    pub played_end: usize,
    pub buffer: String,
    pub devices: std::collections::BTreeMap<String, (Vec<tts_protocol::Device>, String)>,
    resource_sequence: u64,
    instance: Option<String>,
    session: Option<String>,
    text: Arc<str>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            config: None,
            sound: None,
            playback_sound: None,
            playback: Default::default(),
            backends: Vec::new(),
            state: SessionState::Idle,
            source: None,
            text_hash: None,
            range: None,
            terminal: None,
            terminal_revision: 0,
            error: None,
            progress: "未启用模型".into(),
            model_ready: false,
            played_end: 0,
            buffer: String::new(),
            devices: Default::default(),
            resource_sequence: 0,
            instance: None,
            session: None,
            text: Arc::from(""),
        }
    }
}
impl Snapshot {
    pub fn sound_capabilities(&self) -> Option<&Capabilities> {
        let sound = self.sound.as_ref()?;
        self.backends
            .iter()
            .find(|caps| caps.matches(&sound.backend, sound.model.as_deref()))
    }
    pub fn has_pending_sound(&self) -> bool {
        self.playback_sound
            .as_ref()
            .is_some_and(|active| self.sound.as_ref() != Some(active))
    }
    fn advance_played(&mut self, byte: usize) {
        if byte <= self.text.len() && self.text.is_char_boundary(byte) {
            self.played_end = self.played_end.max(byte);
        }
    }
    pub fn matches(&self, request: &ChapterRequest) -> bool {
        self.source.as_ref() == Some(&request.source)
            && self.text_hash.as_deref() == Some(&request.text_hash)
    }
    pub fn should_continue(&self, request: &ChapterRequest, has_next: bool) -> bool {
        has_next
            && self.matches(request)
            && self.terminal == Some(EndReason::Completed)
            && self.config.as_ref().is_some_and(|config| config.auto_play)
    }
    fn apply(&mut self, message: tts_protocol::Message) {
        if self.instance.as_deref() != Some(&message.instance_id) {
            return;
        }
        if message.session_id.is_none() && message.sequence < self.resource_sequence {
            return;
        }
        match message.event {
            Event::DeviceStatus {
                component,
                compiled,
                available,
                selected,
                reason,
            } => {
                let status = format!(
                    "{selected:?} · 可用 {available:?}{}",
                    reason.map(|r| format!(" · {r}")).unwrap_or_default()
                );
                self.devices.insert(component, (compiled, status));
            }
            Event::Config(config) => {
                self.config = Some(config);
            }
            Event::ConfigChanged(config) => {
                self.error = None;
                let switched = self.config.as_ref().is_some_and(|old| {
                    old.model != config.model
                        || old.backend != config.backend
                        || old.tts_device != config.tts_device
                });
                self.config = Some(config);
                if switched {
                    self.resource_sequence = message.sequence;
                    self.model_ready = false;
                    self.session = None;
                    self.range = None;
                    self.buffer.clear();
                    self.terminal = None;
                    self.state = SessionState::Idle;
                    self.progress = "听书设置已切换，请启用模型".into();
                    for (_, status) in self.devices.values_mut() {
                        *status = "未准备".into();
                    }
                }
            }
            Event::ResourceState { stage, resource } => {
                self.progress = format!("{stage} {resource}")
            }
            Event::ModelProgress {
                resource,
                downloaded,
                total,
            } => self.progress = download_progress(&resource, downloaded, total),
            Event::ModelReady => {
                self.error = None;
                self.model_ready = true;
                self.progress = "模型就绪".into();
                if self.session.is_none() {
                    self.state = SessionState::Idle;
                }
            }
            Event::Error(error)
                if message.session_id.is_none() || message.session_id == self.session =>
            {
                self.error = Some(format!("{}: {}", error.stage, error.message))
            }
            event if message.session_id == self.session && self.session.is_some() => match event {
                Event::SessionState { state } => self.state = state,
                Event::BufferStatus {
                    buffered_ms,
                    target_ms,
                    underruns,
                } => {
                    self.buffer = format!(
                        "缓冲 {:.1}/{:.1}s · 耗尽 {} 次",
                        buffered_ms as f64 / 1000.0,
                        target_ms as f64 / 1000.0,
                        underruns
                    );
                }
                Event::SegmentStarted { range, text_hash }
                    if self.text_hash.as_deref() == Some(&text_hash)
                        && range.is_valid(&self.text) =>
                {
                    self.range = Some(range)
                }
                Event::SegmentFinished { range, text_hash }
                    if self.text_hash.as_deref() == Some(&text_hash)
                        && range.is_valid(&self.text) =>
                {
                    self.advance_played(range.end);
                }
                Event::Progress(progress) => self.advance_played(progress.played_end),
                Event::SessionEnded { reason, text_hash }
                    if self.text_hash.as_deref() == Some(&text_hash) && self.terminal.is_none() =>
                {
                    self.terminal = Some(reason);
                    self.terminal_revision += 1;
                    self.state = if reason == EndReason::Failed {
                        SessionState::Failed
                    } else {
                        SessionState::Stopped
                    };
                }
                _ => {}
            },
            _ => {}
        }
    }
}

#[derive(Clone)]
pub struct Handle(Arc<HandleInner>);
struct HandleInner {
    actions: mpsc::Sender<QueuedAction>,
    controls: watch::Sender<Controls>,
    generation: Arc<AtomicU64>,
    snapshot: watch::Receiver<Snapshot>,
    stop: tokio_util::sync::CancellationToken,
    done: watch::Receiver<bool>,
}
impl Drop for HandleInner {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[derive(Clone)]
enum Action {
    Open,
    Prepare,
    CancelPrepare,
    Toggle(ChapterRequest),
    ToggleCurrent,
    Chapter(ChapterRequest, bool),
    Stop,
    Restart,
    RestartFromStart,
    SetPlayback(crate::config::PlaybackPreference),
    Update(ConfigPatch),
    Incoming(String, Received),
    Release,
}
// Lifecycle intents must survive a saturated event/command mailbox. Counters
// retain release and cancellation even when a later chapter replaces the intent.
#[derive(Clone, Default)]
struct Controls {
    generation: u64,
    releases: u64,
    cancellations: u64,
    path: Option<PathBuf>,
    intent: Option<Action>,
}
struct QueuedAction {
    generation: Option<u64>,
    action: Action,
}
enum Delivery {
    Controls(Controls),
    Action(QueuedAction),
}
fn resource_update(patch: &ConfigPatch) -> bool {
    patch.model.is_some() || patch.backend.is_some() || patch.tts_device.is_some()
}

impl Handle {
    pub fn new() -> (Self, JoinHandle<()>) {
        Self::with_store(None)
    }
    fn with_store(store: Option<PathBuf>) -> (Self, JoinHandle<()>) {
        let (actions, receiver) = mpsc::channel(32);
        let (controls, control_receiver) = watch::channel(Controls::default());
        let generation = Arc::new(AtomicU64::new(0));
        let (snapshot, watched) = watch::channel(Snapshot::default());
        let stop = tokio_util::sync::CancellationToken::new();
        let (finished, done) = watch::channel(false);
        let actor = Actor {
            receiver,
            controls: control_receiver,
            generation: generation.clone(),
            releases: 0,
            cancellations: 0,
            observed_generation: 0,
            stop: stop.clone(),
            finished,
            actions: actions.clone(),
            snapshot,
            view: Snapshot::default(),
            client: None,
            event_task: None,
            path: None,
            last: None,
            pending: None,
            next_session: 0,
            store,
            pending_paused: false,
        };
        (
            Self(Arc::new(HandleInner {
                actions,
                controls,
                generation,
                snapshot: watched,
                stop,
                done,
            })),
            tokio::spawn(actor.run()),
        )
    }
    pub fn watch(&self) -> watch::Receiver<Snapshot> {
        self.0.snapshot.clone()
    }
    fn send(&self, action: Action) -> bool {
        let queued = QueuedAction {
            generation: Some(self.0.generation.load(Ordering::SeqCst)),
            action,
        };
        if let Err(error) = self.0.actions.try_send(queued) {
            eprintln!("listening action could not be queued: {error}");
            return false;
        }
        true
    }
    fn control(&self, action: Action) {
        self.0.controls.send_modify(|controls| {
            controls.generation = self.0.generation.fetch_add(1, Ordering::SeqCst) + 1;
            if matches!(action, Action::Release) {
                controls.releases += 1;
            }
            if matches!(action, Action::CancelPrepare) {
                controls.cancellations += 1;
            }
            controls.intent = Some(action);
        });
    }
    pub fn set_path(&self, path: Option<PathBuf>) {
        self.0.controls.send_modify(|controls| controls.path = path);
    }
    pub fn open(&self) {
        self.send(Action::Open);
    }
    pub fn prepare(&self) {
        self.send(Action::Prepare);
    }
    pub fn cancel_prepare(&self) {
        self.control(Action::CancelPrepare);
    }
    pub fn toggle(&self, request: ChapterRequest) {
        self.send(Action::Toggle(request));
    }
    pub fn toggle_current(&self) {
        self.send(Action::ToggleCurrent);
    }
    pub fn chapter(&self, request: ChapterRequest, continue_playing: bool) {
        self.control(Action::Chapter(request, continue_playing));
    }
    pub fn stop(&self) {
        self.control(Action::Stop);
    }
    pub fn restart(&self) {
        self.send(Action::Restart);
    }
    pub fn restart_from_start(&self) {
        self.send(Action::RestartFromStart);
    }
    pub fn set_playback(&self, playback: crate::config::PlaybackPreference) {
        self.send(Action::SetPlayback(playback));
    }
    pub fn update(&self, patch: ConfigPatch) {
        self.send(Action::Update(patch));
    }
    pub fn release(&self) {
        self.control(Action::Release);
    }
    pub async fn shutdown(&self) {
        self.0.stop.cancel();
        let mut done = self.0.done.clone();
        while !*done.borrow() {
            if done.changed().await.is_err() {
                break;
            }
        }
    }
}

struct Actor {
    stop: tokio_util::sync::CancellationToken,
    finished: watch::Sender<bool>,
    receiver: mpsc::Receiver<QueuedAction>,
    actions: mpsc::Sender<QueuedAction>,
    controls: watch::Receiver<Controls>,
    generation: Arc<AtomicU64>,
    releases: u64,
    cancellations: u64,
    observed_generation: u64,
    snapshot: watch::Sender<Snapshot>,
    view: Snapshot,
    client: Option<Client>,
    event_task: Option<JoinHandle<()>>,
    path: Option<PathBuf>,
    last: Option<ChapterRequest>,
    pending: Option<ChapterRequest>,
    next_session: u64,
    store: Option<PathBuf>,
    pending_paused: bool,
}
impl Actor {
    fn config_store(&self) -> anyhow::Result<ConfigStore> {
        self.store
            .as_ref()
            .map_or_else(ConfigStore::user_default, |path| {
                Ok(ConfigStore::new(path.clone()))
            })
    }
    async fn connect(&mut self) -> anyhow::Result<()> {
        if self.client.as_ref().is_some_and(Client::is_connected) {
            return Ok(());
        }
        self.release().await;
        let preferences = self.config_store()?.load()?.tts;
        let configured = preferences.program;
        self.view.playback = preferences.playback;
        if self.view.sound.is_none() {
            self.view.sound = preferences.sound;
        }
        let path = discover(
            self.path.as_deref(),
            configured.as_deref(),
            std::env::var_os("PATH").as_deref(),
        )?;
        let (client, ready) = Client::connect(&path).await?;
        self.view.instance = Some(client.instance().into());
        if let Event::Ready(capabilities) = ready.event {
            self.view.backends = capabilities;
        }
        let mut events = client.subscribe();
        let actions = self.actions.clone();
        let instance = client.instance().to_string();
        self.event_task = Some(tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    Err(error) => Received::Disconnected(format!("听书事件通道异常: {error}")),
                };
                let ended = matches!(event, Received::Disconnected(_));
                if actions
                    .send(QueuedAction {
                        generation: None,
                        action: Action::Incoming(instance.clone(), event),
                    })
                    .await
                    .is_err()
                    || ended
                {
                    break;
                }
            }
        }));
        self.client = Some(client);
        self.command(None, Command::GetConfig).await?;
        if self.view.sound.is_none() {
            self.view.sound = self.view.config.as_ref().map(SoundConfig::from);
        }
        self.view.progress = "未准备模型".into();
        self.view.error = None;
        Ok(())
    }
    async fn command(&mut self, session: Option<String>, command: Command) -> anyhow::Result<()> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("听书程序未连接"))?;
        let response = client.command(session, command).await?;
        self.view.apply(response);
        Ok(())
    }
    async fn begin(&mut self, request: ChapterRequest) -> anyhow::Result<()> {
        self.next_session += 1;
        let id = format!("session-{}", self.next_session);
        self.last = Some(request.clone());
        // Preparation may outlive edits to the next-play preferences.
        let sound = self
            .view
            .config
            .as_ref()
            .map(SoundConfig::from)
            .ok_or_else(|| anyhow::anyhow!("声音设置尚未加载"))?;
        let playback = self.config_store()?.load()?.tts.playback;
        self.command(
            Some(id.clone()),
            Command::Start(Box::new(request.plan(&sound, playback))),
        )
        .await?;
        self.view.session = Some(id);
        self.view.source = Some(request.source.clone());
        self.view.text_hash = Some(request.text_hash.clone());
        self.view.text = Arc::from(request.text.as_str());
        self.view.played_end = request.resume_byte.unwrap_or(0);
        self.view.playback_sound = Some(sound);
        self.view.range = None;
        self.view.buffer.clear();
        self.view.terminal = None;
        self.view.error = None;
        self.view.state = SessionState::Generating;
        if self.pending_paused {
            self.command(self.view.session.clone(), Command::Pause)
                .await?;
            self.view.state = SessionState::Paused;
        }
        self.pending_paused = false;
        Ok(())
    }
    /// Reopen the last chapter with the staged sound. `from_start` discards the
    /// confirmed position so comparison listening replays the whole chapter
    /// and always resumes playback.
    async fn action_restart(&mut self, from_start: bool) -> anyhow::Result<()> {
        if let Some(mut request) = self.last.clone() {
            self.connect().await?;
            let paused = self.view.state == SessionState::Paused;
            let current_source = self.view.matches(&request);
            if current_source && let Some(id) = self.view.session.clone() {
                self.command(Some(id), Command::GetProgress).await?;
            }
            request.resume_byte = Some(if !from_start && current_source {
                self.view.played_end
            } else {
                0
            });
            request.restore_checkpoint = false;
            self.stop().await?;
            self.pending_paused = paused && !from_start;
            self.start(request).await?;
        }
        Ok(())
    }
    async fn start(&mut self, request: ChapterRequest) -> anyhow::Result<()> {
        self.apply_sound().await?;
        if self.view.model_ready {
            self.begin(request).await
        } else {
            self.pending = Some(request);
            self.view.state = SessionState::Preparing;
            self.command(None, Command::PrepareModel).await
        }
    }
    async fn stop(&mut self) -> anyhow::Result<()> {
        self.pending = None;
        let id = self.view.session.take();
        let preparing = self.view.state == SessionState::Preparing && !self.view.model_ready;
        // Invalidate before awaiting the worker, including a stale Stop rejection.
        self.view.range = None;
        self.view.buffer.clear();
        self.view.terminal = None;
        self.view.state = SessionState::Stopped;
        self.pending_paused = false;
        if self.client.as_ref().is_some_and(Client::is_connected) {
            if preparing {
                self.command(None, Command::CancelPrepare).await?;
            }
            if let Some(id) = id {
                self.command(Some(id), Command::Stop).await?;
            }
        }
        Ok(())
    }
    async fn release(&mut self) {
        self.pending = None;
        if let Some(task) = self.event_task.take() {
            task.abort();
        }
        // Keep ownership until close finishes. If shutdown cancels this future,
        // the final release still owns the child and can finish closing/reaping it.
        if let Some(client) = &self.client {
            client.close().await;
        }
        self.client.take();
        self.view.resource_sequence = 0;
        self.view.instance = None;
        self.view.session = None;
        self.view.model_ready = false;
        self.view.range = None;
        self.view.buffer.clear();
        self.view.terminal = None;
        self.view.state = SessionState::Idle;
        self.view.progress = "资源已释放".into();
        self.view.playback_sound = None;
        self.pending_paused = false;
    }

    async fn apply_sound(&mut self) -> anyhow::Result<()> {
        let sound = self
            .view
            .sound
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("声音设置尚未加载"))?;
        let config = self
            .view
            .config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("听书配置尚未加载"))?;
        if *sound == SoundConfig::from(config) {
            return Ok(());
        }
        let patch = ConfigPatch {
            backend: Some(sound.backend.clone()),
            model: sound.model.clone(),
            voice: Some(sound.voice.clone()),
            style: Some(sound.style.clone().unwrap_or_default()),
            tts_device: Some(sound.device),
            expected_revision: config.revision,
            ..Default::default()
        };
        self.command(None, Command::UpdateConfig(patch)).await
    }

    async fn save_sound(&mut self, patch: &ConfigPatch) -> anyhow::Result<()> {
        let mut sound = self
            .view
            .sound
            .clone()
            .ok_or_else(|| anyhow::anyhow!("声音设置尚未加载"))?;
        if let Some(backend) = &patch.backend {
            if *backend != sound.backend {
                sound.model = None;
            }
            sound.backend.clone_from(backend);
        }
        if let Some(model) = &patch.model {
            sound.model = Some(model.clone());
        }
        let caps = self
            .view
            .backends
            .iter()
            .find(|caps| caps.matches(&sound.backend, sound.model.as_deref()))
            .ok_or_else(|| anyhow::anyhow!("当前 Talechime 不支持所选模型"))?;
        if let Some(voice) = &patch.voice {
            sound.voice.clone_from(voice);
        }
        if !caps.voices.contains(&sound.voice) {
            sound.voice.clone_from(&caps.default_voice);
        }
        if let Some(style) = &patch.style {
            sound.style = (!style.is_empty()).then(|| style.clone());
        }
        if !caps.style {
            sound.style = None;
        }
        if let Some(device) = patch.tts_device {
            sound.device = device;
        }
        if sound.device != tts_protocol::Device::Auto
            && !caps.compiled_devices.contains(&sound.device)
        {
            sound.device = tts_protocol::Device::Auto;
        }
        let store = self.config_store()?;
        let saved = sound.clone();
        tokio::task::spawn_blocking(move || store.update(|config| config.tts.sound = Some(saved)))
            .await??;
        self.view.sound = Some(sound);
        self.view.error = None;
        Ok(())
    }
    async fn action_toggle(&mut self, request: ChapterRequest) -> anyhow::Result<()> {
        self.connect().await?;
        if self.view.matches(&request)
            && self.view.terminal.is_none()
            && self.view.session.is_some()
        {
            let command = if self.view.state == SessionState::Paused {
                Command::Resume
            } else {
                Command::Pause
            };
            self.command(self.view.session.clone(), command).await
        } else {
            self.start(request).await
        }
    }
    async fn action(&mut self, action: Action) -> anyhow::Result<bool> {
        match action {
            Action::Open => {
                self.connect().await?;
                self.command(None, Command::GetConfig).await?;
            }
            Action::Prepare => {
                self.connect().await?;
                anyhow::ensure!(
                    self.view.session.is_none() || self.view.terminal.is_some(),
                    "播放中请使用“应用并重播”切换声音"
                );
                self.apply_sound().await?;
                self.view.error = None;
                if !self.view.model_ready && self.view.session.is_none() {
                    self.view.state = SessionState::Preparing;
                }
                self.command(None, Command::PrepareModel).await?;
            }
            Action::CancelPrepare => {
                self.pending = None;
                self.pending_paused = false;
                if self.client.as_ref().is_some_and(Client::is_connected) {
                    self.command(None, Command::CancelPrepare).await?;
                }
                if self.view.session.is_none() {
                    self.view.state = SessionState::Idle;
                }
            }
            Action::Toggle(request) => {
                self.action_toggle(request).await?;
            }
            Action::ToggleCurrent => {
                if let Some(request) = self.last.clone() {
                    // Use the same session identity checks as the reader's P action.
                    self.action_toggle(request).await?;
                }
            }
            Action::Chapter(request, continue_playing) => {
                self.stop().await?;
                self.last = Some(request.clone());
                self.view.played_end = 0;
                if continue_playing && self.client.as_ref().is_some_and(Client::is_connected) {
                    self.start(request).await?;
                }
            }
            Action::Stop => self.stop().await?,
            Action::Restart => self.action_restart(false).await?,
            Action::RestartFromStart => self.action_restart(true).await?,
            Action::SetPlayback(playback) => {
                let store = self.config_store()?;
                tokio::task::spawn_blocking(move || {
                    store.update(|config| config.tts.playback = playback)
                })
                .await??;
                self.view.playback = playback;
                self.view.error = None;
            }
            Action::Update(mut patch) => {
                self.connect().await?;
                if resource_update(&patch) || patch.voice.is_some() || patch.style.is_some() {
                    self.save_sound(&patch).await?;
                }
                if patch.volume.is_some() || patch.speed.is_some() || patch.auto_play.is_some() {
                    patch.expected_revision = self
                        .view
                        .config
                        .as_ref()
                        .map_or(0, |config| config.revision);
                    patch.backend = None;
                    patch.model = None;
                    patch.voice = None;
                    patch.style = None;
                    patch.tts_device = None;
                    self.command(None, Command::UpdateConfig(patch)).await?;
                }
            }
            Action::Incoming(instance, Received::Message(message)) => {
                if self.view.instance.as_deref() != Some(&instance) {
                    return Ok(true);
                }
                if matches!(&message.event, Event::Error(_))
                    && message.session_id.is_none()
                    && self.view.state == SessionState::Preparing
                {
                    self.pending = None;
                    self.pending_paused = false;
                    self.view.state = SessionState::Failed;
                }
                self.view.apply(*message);
                if self.view.model_ready
                    && let Some(request) = self.pending.take()
                {
                    self.begin(request).await?;
                }
            }
            Action::Incoming(instance, Received::Disconnected(reason)) => {
                if self.view.instance.as_deref() != Some(&instance) {
                    return Ok(true);
                }
                self.pending = None;
                self.view.error = Some(reason);
                self.view.state = SessionState::Failed;
                self.view.model_ready = false;
                self.view.session = None;
                self.view.terminal = Some(EndReason::Failed);
                self.view.terminal_revision += 1;
            }
            Action::Release => self.release().await,
        }
        Ok(true)
    }
    async fn apply_controls(&mut self, controls: Controls) -> anyhow::Result<bool> {
        self.path = controls.path;
        if controls.releases != self.releases {
            self.release().await;
            self.releases = controls.releases;
        }
        if controls.cancellations != self.cancellations {
            self.cancellations = controls.cancellations;
            if let Err(error) = self.action(Action::CancelPrepare).await {
                self.view.error = Some(error.to_string());
            }
        }
        // A newer Stop may arrive while releasing or cancelling resources.
        // Retain those operations, but do not execute the superseded chapter.
        if controls.generation != self.generation.load(Ordering::SeqCst)
            || controls.generation == self.observed_generation
        {
            return Ok(true);
        }
        self.observed_generation = controls.generation;
        match controls.intent {
            Some(Action::Release | Action::CancelPrepare) | None => Ok(true),
            Some(action) => self.action(action).await,
        }
    }
    async fn deliver(&mut self, delivery: Delivery) -> anyhow::Result<bool> {
        match delivery {
            Delivery::Controls(controls) => self.apply_controls(controls).await,
            Delivery::Action(queued) => {
                if queued
                    .generation
                    .is_some_and(|generation| generation != self.generation.load(Ordering::SeqCst))
                {
                    self.view.error =
                        Some("排队的听书操作已被停止、切章或释放资源操作取消；请重新操作".into());
                    return Ok(true);
                }
                self.action(queued.action).await
            }
        }
    }
    async fn run(mut self) {
        let stop = self.stop.clone();
        loop {
            let delivery = tokio::select! {
                biased;
                _ = stop.cancelled() => break,
                changed = self.controls.changed() => {
                    if changed.is_err() { break; }
                    Delivery::Controls(self.controls.borrow_and_update().clone())
                }
                queued = self.receiver.recv() => {
                    match queued {
                        Some(queued) => Delivery::Action(queued),
                        None => break,
                    }
                },
            };
            let result = tokio::select! {
                biased;
                _ = stop.cancelled() => break,
                result = self.deliver(delivery) => result,
            };
            if let Err(error) = result {
                self.pending = None;
                self.pending_paused = false;
                self.view.error = Some(error.to_string());
                if self.view.session.is_none()
                    || self.view.state == SessionState::Preparing
                    || !self.client.as_ref().is_some_and(Client::is_connected)
                {
                    self.view.state = SessionState::Failed;
                }
            }
            self.snapshot.send_replace(self.view.clone());
        }
        self.release().await;
        self.finished.send_replace(true);
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ListeningHandle")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn resource_progress_uses_readable_units_and_bounded_percent() {
        assert_eq!(
            super::download_progress("tts/weights.data", 1024 * 1024, 4 * 1024 * 1024),
            "下载 weights.data\n[━━────────] 25.0% · 1.0 / 4.0 MiB"
        );
        assert!(super::download_progress("weights", 8, 4).contains("100.0%"));
        assert_eq!(
            super::download_progress("weights", 0, 0),
            "下载 weights · 0.0 MiB"
        );
    }
    use super::*;
    use tts_protocol::{Message, PROTOCOL_VERSION, text_hash};
    fn request(chapter: &str) -> ChapterRequest {
        ChapterRequest {
            source: SourceId {
                namespace: "reader".into(),
                book: "book".into(),
                chapter: chapter.into(),
            },
            text: "中文🙂".into(),
            text_hash: text_hash("中文🙂"),
            resume_byte: None,
            restore_checkpoint: true,
        }
    }
    fn message(instance: &str, session: &str, event: Event) -> Message {
        Message {
            protocol_version: PROTOCOL_VERSION,
            instance_id: instance.into(),
            session_id: Some(session.into()),
            sequence: 1,
            request_id: None,
            event,
        }
    }
    #[test]
    fn only_current_completed_session_can_continue_and_last_chapter_does_not() {
        let request = request("1");
        let mut view = Snapshot {
            instance: Some("current".into()),
            session: Some("new".into()),
            source: Some(request.source.clone()),
            text_hash: Some(request.text_hash.clone()),
            config: Some(Config {
                auto_play: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        for (instance, session, hash) in [
            ("old", "new", request.text_hash.as_str()),
            ("current", "old", request.text_hash.as_str()),
            ("current", "new", "changed"),
        ] {
            view.apply(message(
                instance,
                session,
                Event::SessionEnded {
                    reason: EndReason::Completed,
                    text_hash: hash.into(),
                },
            ));
            assert!(!view.should_continue(&request, true));
        }
        for reason in [
            EndReason::Failed,
            EndReason::Cancelled,
            EndReason::Completed,
        ] {
            view.terminal = None;
            view.apply(message(
                "current",
                "new",
                Event::SessionEnded {
                    reason,
                    text_hash: request.text_hash.clone(),
                },
            ));
            assert_eq!(
                view.should_continue(&request, true),
                reason == EndReason::Completed
            );
            assert!(!view.should_continue(&request, false));
        }
    }
    async fn wait_for(
        watched: &mut watch::Receiver<Snapshot>,
        predicate: impl Fn(&Snapshot) -> bool,
    ) -> Snapshot {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let snapshot = watched.borrow().clone();
                if predicate(&snapshot) {
                    return snapshot;
                }
                watched.changed().await.unwrap();
            }
        })
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn actor_starts_only_on_explicit_action_and_reuses_worker_across_chapters() {
        let (directory, path) = crate::tts::client::tests::fixture("worker");
        let (handle, task) = Handle::with_store(Some(directory.path().join("config.toml")));
        let mut watched = handle.watch();
        handle.set_path(Some(path));
        handle.chapter(request("1"), false);
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(watched.borrow().config.is_none());
        handle.toggle(request("1"));
        let first = wait_for(&mut watched, |view| {
            view.terminal == Some(EndReason::Completed)
        })
        .await;
        handle.update(ConfigPatch {
            volume: Some(0.7),
            ..Default::default()
        });
        wait_for(&mut watched, |view| {
            view.config
                .as_ref()
                .is_some_and(|config| config.volume == 0.7)
        })
        .await;
        handle.chapter(request("2"), true);
        let second = wait_for(&mut watched, |view| {
            view.source
                .as_ref()
                .is_some_and(|source| source.chapter == "2")
                && view.terminal == Some(EndReason::Completed)
        })
        .await;
        assert_eq!(first.instance, second.instance);
        assert_eq!(second.terminal_revision, first.terminal_revision + 1);
        assert_eq!(second.config.unwrap().revision, 1);
        handle.shutdown().await;
        task.await.unwrap();
    }
    #[test]
    fn backend_switch_invalidates_model_and_older_resource_events() {
        let mut view = Snapshot {
            instance: Some("worker".into()),
            config: Some(Config::default()),
            model_ready: true,
            session: Some("old".into()),
            ..Default::default()
        };
        let mut changed = message(
            "worker",
            "old",
            Event::ConfigChanged(Config {
                backend: "kokoro".into(),
                voice: "Zf001".into(),
                ..Default::default()
            }),
        );
        changed.session_id = None;
        changed.sequence = 10;
        view.apply(changed);
        assert!(!view.model_ready);
        assert!(view.session.is_none());
        let mut stale = message("worker", "old", Event::ModelReady);
        stale.session_id = None;
        stale.sequence = 9;
        view.apply(stale);
        assert!(!view.model_ready);
    }

    #[test]
    fn progress_only_advances_for_current_playback_and_valid_utf8_boundaries() {
        let mut view = Snapshot {
            instance: Some("worker".into()),
            session: Some("current".into()),
            text_hash: Some(tts_protocol::text_hash("中文🙂")),
            text: Arc::from("中文🙂"),
            ..Default::default()
        };
        let hash = view.text_hash.clone().unwrap();
        view.apply(message(
            "worker",
            "current",
            Event::SegmentStarted {
                range: TextRange { start: 0, end: 10 },
                text_hash: hash.clone(),
            },
        ));
        assert_eq!(view.played_end, 0);
        for (session, hash, end) in [
            ("old", hash.clone(), 10),
            ("current", "wrong".into(), 10),
            ("current", hash.clone(), 4),
        ] {
            view.apply(message(
                "worker",
                session,
                Event::SegmentFinished {
                    range: TextRange { start: 0, end },
                    text_hash: hash,
                },
            ));
            assert_eq!(view.played_end, 0);
        }
        view.apply(message(
            "worker",
            "current",
            Event::SegmentFinished {
                range: TextRange { start: 0, end: 6 },
                text_hash: hash.clone(),
            },
        ));
        view.apply(message(
            "worker",
            "current",
            Event::SegmentFinished {
                range: TextRange { start: 0, end: 3 },
                text_hash: hash,
            },
        ));
        assert_eq!(view.played_end, 6);
    }

    fn requests(path: &std::path::Path) -> Vec<tts_protocol::Request> {
        std::fs::read_to_string(path.with_extension("requests.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn cancelling_paused_replay_does_not_pause_a_later_explicit_start() {
        let (directory, path) = crate::tts::client::tests::fixture("playing-cancel-prepare");
        let (handle, task) = Handle::with_store(Some(directory.path().join("reader/config.toml")));
        handle.set_path(Some(path));
        let mut watched = handle.watch();
        handle.toggle(request("1"));
        wait_for(&mut watched, |view| view.range.is_some()).await;
        handle.toggle_current();
        wait_for(&mut watched, |view| view.state == SessionState::Paused).await;
        handle.update(ConfigPatch {
            backend: Some("qwen".into()),
            model: Some("custom".into()),
            ..Default::default()
        });
        wait_for(&mut watched, |view| {
            view.sound
                .as_ref()
                .is_some_and(|sound| sound.backend == "qwen")
        })
        .await;
        handle.restart();
        wait_for(&mut watched, |view| view.state == SessionState::Preparing).await;
        handle.cancel_prepare();
        wait_for(&mut watched, |view| {
            view.state == SessionState::Idle && view.model_ready
        })
        .await;
        handle.toggle(request("1"));
        let next = wait_for(&mut watched, |view| view.range.is_some()).await;
        assert_ne!(next.state, SessionState::Paused);
        handle.shutdown().await;
        task.await.unwrap();
    }

    #[tokio::test]
    async fn failed_paused_replay_does_not_pause_a_later_explicit_start() {
        let (directory, path) = crate::tts::client::tests::fixture("playing-fail-prepare");
        let (handle, task) = Handle::with_store(Some(directory.path().join("config.toml")));
        handle.set_path(Some(path));
        let mut watched = handle.watch();
        handle.toggle(request("1"));
        wait_for(&mut watched, |view| view.range.is_some()).await;
        handle.toggle_current();
        wait_for(&mut watched, |view| view.state == SessionState::Paused).await;
        handle.update(ConfigPatch {
            backend: Some("qwen".into()),
            model: Some("custom".into()),
            ..Default::default()
        });
        wait_for(&mut watched, |view| {
            view.sound
                .as_ref()
                .is_some_and(|sound| sound.backend == "qwen")
        })
        .await;
        handle.restart();
        wait_for(&mut watched, |view| view.state == SessionState::Failed).await;
        handle.toggle_current();
        let next = wait_for(&mut watched, |view| view.range.is_some()).await;
        assert_ne!(next.state, SessionState::Paused);
        handle.shutdown().await;
        task.await.unwrap();
    }

    #[tokio::test]
    async fn sound_is_staged_persisted_and_applied_from_played_boundary_preserving_pause() {
        let (directory, path) = crate::tts::client::tests::fixture("playing");
        let store = directory.path().join("reader/config.toml");
        let (handle, task) = Handle::with_store(Some(store.clone()));
        handle.set_path(Some(path.clone()));
        let mut watched = handle.watch();
        handle.toggle(request("1"));
        wait_for(&mut watched, |view| view.range.is_some()).await;
        handle.toggle(request("1"));
        let paused = wait_for(&mut watched, |view| view.state == SessionState::Paused).await;
        handle.update(ConfigPatch {
            backend: Some("qwen".into()),
            model: Some("custom".into()),
            voice: Some("C".into()),
            tts_device: Some(tts_protocol::Device::Metal),
            ..Default::default()
        });
        let staged = wait_for(&mut watched, |view| {
            view.sound
                .as_ref()
                .is_some_and(|sound| sound.backend == "qwen")
        })
        .await;
        assert_eq!(staged.session, paused.session);
        assert_eq!(staged.state, SessionState::Paused);
        assert_eq!(staged.config.as_ref().unwrap().backend, "moss");
        assert_eq!(staged.playback_sound.as_ref().unwrap().voice, "A");
        assert!(staged.has_pending_sound());
        assert_eq!(
            ConfigStore::new(store.clone()).load().unwrap().tts.sound,
            staged.sound
        );
        assert!(
            !requests(&path)
                .iter()
                .any(|r| matches!(r.command, Command::UpdateConfig(_)))
        );
        handle.update(ConfigPatch {
            speed: Some(1.3),
            volume: Some(0.7),
            ..Default::default()
        });
        wait_for(&mut watched, |view| {
            view.config
                .as_ref()
                .is_some_and(|config| config.volume == 0.7 && config.speed == 1.3)
        })
        .await;
        handle.restart();
        let restarted = wait_for(&mut watched, |view| {
            view.session != paused.session && view.state == SessionState::Paused
        })
        .await;
        assert_eq!(restarted.played_end, 3);
        assert!(!restarted.has_pending_sound());
        assert_eq!(restarted.playback_sound.as_ref().unwrap().backend, "qwen");
        let starts: Vec<_> = requests(&path)
            .into_iter()
            .filter_map(|r| {
                if let Command::Start(plan) = r.command {
                    Some(plan)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(starts.len(), 2);
        assert!(starts[0].sealed && starts[0].playback == tts_protocol::PlanPlayback::Streaming);
        assert_eq!(starts[0].voices, vec!["A"]);
        assert_eq!(starts[0].spans[0].range, TextRange { start: 0, end: 10 });
        assert_eq!(starts[1].resume_byte, Some(3));
        assert!(!starts[1].restore_checkpoint);
        assert_eq!(starts[1].voices, vec!["C"]);
        handle.restart_from_start();
        let replayed = wait_for(&mut watched, |view| {
            view.session != restarted.session && view.state == SessionState::Generating
        })
        .await;
        assert_eq!(replayed.played_end, 0);
        let starts: Vec<_> = requests(&path)
            .into_iter()
            .filter_map(|r| {
                if let Command::Start(plan) = r.command {
                    Some(plan)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(starts.len(), 3);
        assert_eq!(starts[2].resume_byte, Some(0));
        assert!(!starts[2].restore_checkpoint);
        assert_eq!(starts[2].voices, vec!["C"]);
        handle.shutdown().await;
        task.await.unwrap();
        let (next, task) = Handle::with_store(Some(store));
        next.set_path(Some(path));
        next.open();
        let restored = wait_for(&mut next.watch(), |view| view.config.is_some()).await;
        assert_eq!(restored.sound, staged.sound);
        next.shutdown().await;
        task.await.unwrap();
    }
}
