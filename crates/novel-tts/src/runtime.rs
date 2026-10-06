//! Worker application state, independent of JSON Lines transport.
use crate::resources::Resources;
use std::{
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::mpsc, task::JoinHandle};
use tts_core::{
    AudioPlayer,
    checkpoint::CheckpointStore,
    config::{ConfigError, ConfigStore},
    session::{SessionEvent, SessionManager},
};
use tts_protocol::{Command, ErrorInfo, Event, Request, SessionState};

type Preparation = JoinHandle<anyhow::Result<crate::preparation::Prepared>>;

pub struct Response {
    pub session: Option<String>,
    pub event: Event,
}
impl Response {
    fn global(event: Event) -> Self {
        Self {
            session: None,
            event,
        }
    }
    fn error(code: &str, stage: &str, error: impl ToString) -> Self {
        Self::global(Event::Error(ErrorInfo {
            code: code.into(),
            stage: stage.into(),
            message: error.to_string(),
            retryable: true,
        }))
    }
}

pub struct Worker {
    store: ConfigStore,
    checkpoints: CheckpointStore,
    resources: Resources,
    events: mpsc::Sender<SessionEvent>,
    progress: mpsc::Sender<Event>,
    preparing: Option<Preparation>,
    progress_forwarder: Option<JoinHandle<()>>,
    manager: Option<SessionManager>,
    resource_state: SessionState,
    next_session: u64,
}
impl Worker {
    pub fn new(
        store: ConfigStore,
        checkpoints: CheckpointStore,
        resources: Resources,
        events: mpsc::Sender<SessionEvent>,
        progress: mpsc::Sender<Event>,
    ) -> Self {
        Self {
            store,
            checkpoints,
            resources,
            events,
            progress,
            preparing: None,
            progress_forwarder: None,
            manager: None,
            resource_state: SessionState::Idle,
            next_session: 0,
        }
    }
    pub fn catalog(&self) -> anyhow::Result<Vec<tts_protocol::Capabilities>> {
        self.resources.catalog()
    }
    #[cfg(any(feature = "moss", feature = "kokoro", feature = "qwen"))]
    pub fn unprepared_device_status(&self, component: &str) -> anyhow::Result<Event> {
        Ok(crate::preparation::unprepared_device_status(
            component,
            &self.store.load()?.backend,
            &self.resources,
        ))
    }
    pub fn is_preparing(&self) -> bool {
        self.preparing.is_some()
    }
    #[cfg(any(feature = "moss", feature = "kokoro", feature = "qwen"))]
    pub fn has_prepared_model(&self) -> bool {
        self.manager.is_some()
    }
    fn status(&self) -> (Option<String>, SessionState) {
        self.manager
            .as_ref()
            .map_or((None, self.resource_state), SessionManager::status)
    }
    /// Poll only while preparation exists; dropping this future keeps the task owned.
    pub async fn prepared(&mut self) -> Response {
        let Some(task) = &mut self.preparing else {
            return Response::error("prepare_failed", "model", "no preparation is running");
        };
        let loaded = task.await;
        self.preparing.take();
        let prepared = match loaded {
            Ok(Ok(backend)) => backend,
            Ok(Err(error)) => {
                self.resource_state = SessionState::Failed;
                return Response::error("prepare_failed", "model", error);
            }
            Err(error) => {
                self.resource_state = SessionState::Failed;
                return Response::error("prepare_failed", "model", error);
            }
        };
        match AudioPlayer::open() {
            Ok(player) => {
                self.manager = Some(SessionManager::new(
                    prepared.backend,
                    Rc::new(player),
                    self.checkpoints.clone(),
                    self.events.clone(),
                ));
                if let Some(aligner) = prepared.aligner {
                    self.manager = self
                        .manager
                        .take()
                        .map(|manager| manager.with_aligner(aligner));
                }
                self.resource_state = SessionState::Idle;
                Response::global(Event::ModelReady)
            }
            Err(error) => {
                self.resource_state = SessionState::Failed;
                Response::error("device_unavailable", "audio", error)
            }
        }
    }
    async fn cancel_prepare(&mut self) {
        if let Some(task) = self.preparing.take() {
            task.abort();
            let _ = task.await;
            self.disconnect_progress();
        }
        self.resource_state = SessionState::Idle;
    }
    fn disconnect_progress(&mut self) {
        if let Some(task) = self.progress_forwarder.take() {
            task.abort();
        }
    }
    pub async fn command(&mut self, request: &Request) -> Response {
        match &request.command {
            Command::GetStatus => {
                let (session, state) = self.status();
                Response {
                    session,
                    event: Event::SessionState { state },
                }
            }
            Command::GetConfig => match self.store.load() {
                Ok(config) => Response::global(Event::Config(config)),
                Err(error) => Response::error("config_invalid", "config", error),
            },
            Command::UpdateConfig(patch) => {
                let old = match self.store.load() {
                    Ok(config) => config,
                    Err(error) => return Response::error("config_invalid", "config", error),
                };
                if let Err(error) = crate::preparation::validate_alignment_enabled(
                    patch.alignment_enabled.unwrap_or(old.alignment_enabled),
                ) {
                    return Response::error("alignment_unavailable", "config", error);
                }
                let target = patch.backend.as_deref().unwrap_or(&old.backend);
                let caps = match self.resources.capabilities(target) {
                    Ok(caps) => caps,
                    Err(error) => return Response::error("backend_unavailable", "config", error),
                };
                #[cfg(any(feature = "moss", feature = "kokoro", feature = "qwen"))]
                for (component, device) in [
                    (
                        "tts",
                        patch
                            .tts_device
                            .or_else(|| (target != old.backend).then_some(old.tts_device)),
                    ),
                    ("alignment", patch.alignment_device),
                ]
                .into_iter()
                .filter_map(|(component, device)| device.map(|device| (component, device)))
                {
                    if let Err(error) = crate::preparation::validate_device(
                        component,
                        device,
                        target,
                        &self.resources,
                    ) {
                        return Response::error("device_unavailable", "config", error);
                    }
                }
                let config = match self.store.update(patch, &caps) {
                    Ok(config) => config,
                    Err(error) => {
                        let code = if matches!(error, ConfigError::RevisionConflict) {
                            "revision_conflict"
                        } else {
                            "config_invalid"
                        };
                        return Response::error(code, "config", error);
                    }
                };
                if config.backend != old.backend
                    || config.tts_device != old.tts_device
                    || config.alignment_device != old.alignment_device
                    || config.alignment_enabled != old.alignment_enabled
                {
                    self.cancel_prepare().await;
                    self.disconnect_progress();
                    if let Some(mut manager) = self.manager.take() {
                        let _ = manager.stop().await;
                    }
                    self.resource_state = SessionState::Idle;
                    return Response::global(Event::ConfigChanged(config));
                }
                self.next_session += 1;
                let nonce = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let id = format!(
                    "worker-voice-{}-{nonce}-{}",
                    std::process::id(),
                    self.next_session
                );
                let session = if let Some(manager) = &mut self.manager {
                    match manager.update_settings(id.clone(), &config).await {
                        Ok(true) => Some(id),
                        Ok(false) => None,
                        Err(error) => return Response::error("session_invalid", "session", error),
                    }
                } else {
                    None
                };
                Response {
                    session,
                    event: Event::ConfigChanged(config),
                }
            }
            Command::PrepareModel => {
                if self.manager.is_some() {
                    return Response::global(Event::ModelReady);
                }
                if self.preparing.is_none() {
                    let config = match self.store.load() {
                        Ok(config) => config,
                        Err(error) => return Response::error("config_invalid", "config", error),
                    };
                    let caps = match self.resources.capabilities(&config.backend) {
                        Ok(caps) => caps,
                        Err(error) => {
                            return Response::error("backend_unavailable", "model", error);
                        }
                    };
                    if let Err(error) = tts_core::config::validate(&config, &caps) {
                        return Response::error("config_invalid", "config", error);
                    }
                    let resources = self.resources.clone();
                    self.disconnect_progress();
                    let (progress, mut updates) = mpsc::channel(16);
                    let outgoing = self.progress.clone();
                    self.progress_forwarder = Some(tokio::task::spawn_local(async move {
                        while let Some(event) = updates.recv().await {
                            if outgoing.send(event).await.is_err() {
                                break;
                            }
                        }
                    }));
                    self.preparing = Some(tokio::task::spawn_local(async move {
                        crate::preparation::prepare(resources, config, progress).await
                    }));
                    self.resource_state = SessionState::Preparing;
                }
                Response::global(Event::Accepted)
            }
            Command::CancelPrepare => {
                self.cancel_prepare().await;
                Response::global(Event::Accepted)
            }
            Command::Hello | Command::Shutdown => Response::error(
                "invalid_request",
                "protocol",
                "transport command reached worker",
            ),
            command => self.session_command(request, command).await,
        }
    }
    async fn session_command(&mut self, request: &Request, command: &Command) -> Response {
        let Some(manager) = &mut self.manager else {
            return Response::error(
                "model_not_ready",
                "session",
                "prepare_model must finish before playback",
            );
        };
        let Some(id) = request.session_id.as_deref().filter(|id| !id.is_empty()) else {
            return Response::error(
                "session_id_required",
                "session",
                "session command requires a nonempty ID",
            );
        };
        let result = match command {
            Command::Start(start) => match self.store.load() {
                Ok(config) => manager.start(id.into(), start.clone(), &config).await,
                Err(error) => return Response::error("config_invalid", "config", error),
            },
            Command::Pause => manager.pause(id).await,
            Command::Resume => manager.resume(id).await,
            Command::Stop => {
                if manager.status().0.as_deref() == Some(id) {
                    manager.stop().await
                } else {
                    Err(tts_core::session::SessionError::Invalid(
                        "stale session".into(),
                    ))
                }
            }
            Command::Seek {
                byte,
                new_session_id,
            } => match self.store.load() {
                Ok(config) => {
                    manager
                        .seek(id, new_session_id.clone(), *byte, &config)
                        .await
                }
                Err(error) => return Response::error("config_invalid", "config", error),
            },
            _ => return Response::error("invalid_request", "session", "not a session command"),
        };
        match result {
            Ok(()) => Response {
                session: Some(match command {
                    Command::Seek { new_session_id, .. } => new_session_id.clone(),
                    _ => id.into(),
                }),
                event: Event::Accepted,
            },
            Err(error) => Response::error("session_invalid", "session", error),
        }
    }
    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        self.cancel_prepare().await;
        self.disconnect_progress();
        if let Some(manager) = &mut self.manager {
            tokio::time::timeout(Duration::from_secs(3), manager.stop()).await??;
        }
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.disconnect_progress();
        if let Some(task) = &self.preparing {
            task.abort();
        }
    }
}

#[cfg(all(test, feature = "moss", feature = "alignment"))]
mod tests {
    use super::*;
    use std::{cell::Cell, sync::Arc};
    use tts_core::{
        backend::{Backend, BackendError, Pcm, Streaming},
        player::Playback,
    };
    struct SilentBackend(tts_protocol::Capabilities);
    impl Backend for SilentBackend {
        fn capabilities(&self) -> tts_protocol::Capabilities {
            self.0.clone()
        }
        fn stream<'a>(&'a self, _: &'a str, _: &'a str) -> Streaming<'a> {
            Box::pin(async { Err(BackendError::Synthesis("not used".into())) })
        }
    }
    #[derive(Default)]
    struct IdlePlayer(Cell<usize>);
    impl Playback for IdlePlayer {
        fn append(&self, _: Arc<Pcm>) {}
        fn position(&self) -> Duration {
            Duration::ZERO
        }
        fn is_empty(&self) -> bool {
            true
        }
        fn pause(&self) {}
        fn resume(&self) {}
        fn stop(&self) {
            self.0.set(self.0.get() + 1);
        }
        fn configure(&self, _: f32, _: f32) {}
    }
    #[tokio::test]
    async fn alignment_toggle_releases_prepared_manager_and_preserves_checkpoint_files() {
        let root = tempfile::tempdir().unwrap();
        let resources = Resources::new(Some(root.path().join("models"))).unwrap();
        let caps = resources.capabilities("moss").unwrap();
        let checkpoints = CheckpointStore::new(root.path().join("checkpoints"));
        std::fs::create_dir_all(root.path().join("checkpoints")).unwrap();
        let progress_file = root.path().join("checkpoints/existing.json");
        std::fs::write(&progress_file, "reliable progress").unwrap();
        let (events, _) = mpsc::channel(32);
        let (progress, _) = mpsc::channel(32);
        let player = Rc::new(IdlePlayer::default());
        let mut worker = Worker::new(
            ConfigStore::new(root.path().join("config.json")),
            checkpoints.clone(),
            resources,
            events.clone(),
            progress,
        );
        worker.manager = Some(SessionManager::new(
            Rc::new(SilentBackend(caps)),
            player.clone(),
            checkpoints,
            events,
        ));
        let response = worker
            .command(&Request {
                protocol_version: tts_protocol::PROTOCOL_VERSION,
                request_id: "toggle".into(),
                session_id: None,
                command: Command::UpdateConfig(tts_protocol::ConfigPatch {
                    alignment_enabled: Some(true),
                    ..Default::default()
                }),
            })
            .await;
        assert!(matches!(response.event, Event::ConfigChanged(config) if config.alignment_enabled));
        assert!(worker.manager.is_none());
        assert_eq!(worker.resource_state, SessionState::Idle);
        assert!(player.0.get() > 0);
        assert_eq!(
            std::fs::read_to_string(progress_file).unwrap(),
            "reliable progress"
        );
    }
}
