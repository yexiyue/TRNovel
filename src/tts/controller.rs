//! UI-facing actor: starts on explicit actions and filters stale playback events.
use super::client::{Client, Received, discover};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tts_protocol::{
    Capabilities, Command, Config, ConfigPatch, EndReason, Event, SessionState, SourceId,
    StartRequest, TextRange,
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
    pub capabilities: Option<Capabilities>,
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
    pub alignment: String,
    pub buffer: String,
    pub devices: std::collections::BTreeMap<String, (Vec<tts_protocol::Device>, String)>,
    resource_sequence: u64,
    instance: Option<String>,
    session: Option<String>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            config: None,
            capabilities: None,
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
            alignment: "片段高亮".into(),
            buffer: String::new(),
            devices: Default::default(),
            resource_sequence: 0,
            instance: None,
            session: None,
        }
    }
}
impl Snapshot {
    pub fn matches(&self, request: &StartRequest) -> bool {
        self.source.as_ref() == Some(&request.source)
            && self.text_hash.as_deref() == Some(&request.text_hash)
    }
    pub fn should_continue(&self, request: &StartRequest, has_next: bool) -> bool {
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
                self.capabilities = self
                    .backends
                    .iter()
                    .find(|caps| caps.matches(&config.backend, config.model.as_deref()))
                    .cloned();
                self.config = Some(config);
            }
            Event::ConfigChanged(config) => {
                self.error = None;
                let switched = self.config.as_ref().is_some_and(|old| {
                    old.model != config.model
                        || old.backend != config.backend
                        || old.tts_device != config.tts_device
                        || old.alignment_device != config.alignment_device
                        || old.alignment_enabled != config.alignment_enabled
                });
                self.capabilities = self
                    .backends
                    .iter()
                    .find(|caps| caps.matches(&config.backend, config.model.as_deref()))
                    .cloned();
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
                    self.alignment = "片段高亮".into();
                    for (_, status) in self.devices.values_mut() {
                        *status = "未准备".into();
                    }
                }
                if let Some(id) = message.session_id
                    && self.session.is_some()
                    && self.session.as_ref() != Some(&id)
                {
                    self.session = Some(id);
                    self.range = None;
                    self.buffer.clear();
                    self.terminal = None;
                    if self.state != SessionState::Paused {
                        self.state = SessionState::Generating;
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
            Event::AlignmentStatus {
                sentence_highlight,
                reason,
            } if message.session_id.is_none() => {
                self.alignment = if sentence_highlight {
                    "逐句高亮".into()
                } else {
                    reason.map_or_else(
                        || "片段高亮".into(),
                        |reason| format!("片段高亮 · {reason}"),
                    )
                };
            }
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
                Event::AlignmentStatus {
                    sentence_highlight,
                    reason,
                } => {
                    self.alignment = if sentence_highlight {
                        "逐句高亮".into()
                    } else {
                        reason.map_or_else(
                            || "片段高亮".into(),
                            |reason| format!("片段高亮 · {reason}"),
                        )
                    };
                }
                Event::SegmentStarted { range, text_hash }
                | Event::SentenceStarted { range, text_hash }
                    if self.text_hash.as_deref() == Some(&text_hash) =>
                {
                    self.range = Some(range)
                }
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
    switching: Arc<AtomicBool>,
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
    Toggle(StartRequest),
    Chapter(StartRequest, bool),
    Stop,
    Restart,
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
    patch.model.is_some()
        || patch.backend.is_some()
        || patch.tts_device.is_some()
        || patch.alignment_device.is_some()
        || patch.alignment_enabled.is_some()
}

impl Handle {
    pub fn new() -> (Self, JoinHandle<()>) {
        let (actions, receiver) = mpsc::channel(32);
        let (controls, control_receiver) = watch::channel(Controls::default());
        let generation = Arc::new(AtomicU64::new(0));
        let switching = Arc::new(AtomicBool::new(false));
        let (snapshot, watched) = watch::channel(Snapshot::default());
        let stop = tokio_util::sync::CancellationToken::new();
        let (finished, done) = watch::channel(false);
        let actor = Actor {
            receiver,
            controls: control_receiver,
            generation: generation.clone(),
            switching: switching.clone(),
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
        };
        (
            Self(Arc::new(HandleInner {
                actions,
                controls,
                generation,
                switching,
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
    pub fn toggle(&self, request: StartRequest) {
        self.send(Action::Toggle(request));
    }
    pub fn chapter(&self, request: StartRequest, continue_playing: bool) {
        self.control(Action::Chapter(request, continue_playing));
    }
    pub fn stop(&self) {
        self.control(Action::Stop);
    }
    pub fn restart(&self) {
        self.send(Action::Restart);
    }
    pub fn update(&self, patch: ConfigPatch) {
        let switching = resource_update(&patch);
        if switching {
            if self
                .0
                .switching
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return;
            }
        } else if self.0.switching.load(Ordering::SeqCst) {
            return;
        }
        if !self.send(Action::Update(patch)) && switching {
            self.0.switching.store(false, Ordering::SeqCst);
        }
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
    switching: Arc<AtomicBool>,
    releases: u64,
    cancellations: u64,
    observed_generation: u64,
    snapshot: watch::Sender<Snapshot>,
    view: Snapshot,
    client: Option<Client>,
    event_task: Option<JoinHandle<()>>,
    path: Option<PathBuf>,
    last: Option<StartRequest>,
    pending: Option<StartRequest>,
    next_session: u64,
}
impl Actor {
    async fn connect(&mut self) -> anyhow::Result<()> {
        if self.client.as_ref().is_some_and(Client::is_connected) {
            return Ok(());
        }
        self.release().await;
        let path = discover(
            self.path.as_deref(),
            &std::env::current_exe()?,
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
    async fn begin(&mut self, request: StartRequest) -> anyhow::Result<()> {
        self.next_session += 1;
        let id = format!("session-{}", self.next_session);
        self.last = Some(request.clone());
        self.command(Some(id.clone()), Command::Start(request.clone()))
            .await?;
        self.view.session = Some(id);
        self.view.source = Some(request.source.clone());
        self.view.text_hash = Some(request.text_hash.clone());
        self.view.range = None;
        self.view.buffer.clear();
        self.view.terminal = None;
        self.view.error = None;
        self.view.state = SessionState::Generating;
        Ok(())
    }
    async fn start(&mut self, request: StartRequest) -> anyhow::Result<()> {
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
    }
    async fn action(&mut self, action: Action) -> anyhow::Result<bool> {
        match action {
            Action::Open => {
                self.connect().await?;
                self.command(None, Command::GetConfig).await?;
            }
            Action::Prepare => {
                self.connect().await?;
                self.view.error = None;
                if !self.view.model_ready && self.view.session.is_none() {
                    self.view.state = SessionState::Preparing;
                }
                self.command(None, Command::PrepareModel).await?;
            }
            Action::CancelPrepare => {
                self.pending = None;
                if self.client.as_ref().is_some_and(Client::is_connected) {
                    self.command(None, Command::CancelPrepare).await?;
                }
                if self.view.session.is_none() {
                    self.view.state = SessionState::Idle;
                }
            }
            Action::Toggle(request) => {
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
                    self.command(self.view.session.clone(), command).await?;
                } else {
                    self.start(request).await?;
                }
            }
            Action::Chapter(request, continue_playing) => {
                self.stop().await?;
                self.last = Some(request.clone());
                if continue_playing && self.client.as_ref().is_some_and(Client::is_connected) {
                    self.start(request).await?;
                }
            }
            Action::Stop => self.stop().await?,
            Action::Restart => {
                if let Some(mut request) = self.last.clone() {
                    self.connect().await?;
                    request.resume_byte = Some(0);
                    request.restore_checkpoint = false;
                    self.start(request).await?;
                }
            }
            Action::Update(mut patch) => {
                self.connect().await?;
                let switched = if let Some(config) = &self.view.config {
                    patch.expected_revision = config.revision;
                    patch
                        .model
                        .as_ref()
                        .is_some_and(|model| Some(model) != config.model.as_ref())
                        || patch
                            .backend
                            .as_ref()
                            .is_some_and(|backend| backend != &config.backend)
                        || patch
                            .tts_device
                            .is_some_and(|device| device != config.tts_device)
                        || patch
                            .alignment_device
                            .is_some_and(|device| device != config.alignment_device)
                        || patch
                            .alignment_enabled
                            .is_some_and(|enabled| enabled != config.alignment_enabled)
                } else {
                    false
                };
                self.command(None, Command::UpdateConfig(patch)).await?;
                if switched {
                    self.pending = None;
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
            let switching = matches!(&delivery,Delivery::Action(QueuedAction {action:Action::Update(patch),..}) if resource_update(patch));
            let result = tokio::select! {
                biased;
                _ = stop.cancelled() => break,
                result = self.deliver(delivery) => result,
            };
            if let Err(error) = result {
                self.pending = None;
                self.view.error = Some(error.to_string());
                if self.view.session.is_none()
                    || self.view.state == SessionState::Preparing
                    || !self.client.as_ref().is_some_and(Client::is_connected)
                {
                    self.view.state = SessionState::Failed;
                }
            }
            self.snapshot.send_replace(self.view.clone());
            if switching {
                self.switching.store(false, Ordering::SeqCst);
            }
        }
        self.switching.store(false, Ordering::SeqCst);
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
    fn request(chapter: &str) -> StartRequest {
        StartRequest {
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
        let (_directory, path) = crate::tts::client::tests::fixture("worker");
        let (handle, task) = Handle::new();
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
    fn alignment_switch_invalidates_timeline_and_resource_updates_are_serialized() {
        let mut view = Snapshot {
            instance: Some("worker".into()),
            config: Some(Config::default()),
            model_ready: true,
            session: Some("old".into()),
            range: Some(TextRange { start: 0, end: 3 }),
            alignment: "逐句高亮".into(),
            ..Default::default()
        };
        let mut changed = message(
            "worker",
            "old",
            Event::ConfigChanged(Config {
                alignment_enabled: true,
                ..Default::default()
            }),
        );
        changed.session_id = None;
        view.apply(changed);
        assert!(!view.model_ready);
        assert!(view.session.is_none() && view.range.is_none());
        assert_eq!(view.alignment, "片段高亮");
        assert!(resource_update(&ConfigPatch {
            alignment_enabled: Some(true),
            ..Default::default()
        }));
        assert!(resource_update(&ConfigPatch {
            tts_device: Some(tts_protocol::Device::Cpu),
            ..Default::default()
        }));
        assert!(!resource_update(&ConfigPatch {
            volume: Some(2.0),
            ..Default::default()
        }));
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
}
