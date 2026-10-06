//! Worker application state, independent of JSON Lines transport.
use crate::resources::Resources;
use std::{
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::mpsc, task::JoinHandle};
use tts_core::{
    AudioPlayer,
    backend::KokoroBackend,
    checkpoint::CheckpointStore,
    config::{ConfigError, ConfigStore},
    session::{SessionEvent, SessionManager},
};
use tts_protocol::{Command, ErrorInfo, Event, Request, SessionState};

type Preparation = JoinHandle<anyhow::Result<Rc<KokoroBackend>>>;

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
            manager: None,
            resource_state: SessionState::Idle,
            next_session: 0,
        }
    }
    pub fn is_preparing(&self) -> bool {
        self.preparing.is_some()
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
        let backend = match loaded {
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
                    backend,
                    Rc::new(player),
                    self.checkpoints.clone(),
                    self.events.clone(),
                ));
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
        }
        self.resource_state = SessionState::Idle;
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
                let config = match self.store.update(patch, &KokoroBackend::capabilities()) {
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
                    self.preparing = Some(tokio::task::spawn_local(
                        self.resources.clone().prepare(self.progress.clone()),
                    ));
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
        if let Some(manager) = &mut self.manager {
            tokio::time::timeout(Duration::from_secs(3), manager.stop()).await??;
        }
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(task) = &self.preparing {
            task.abort();
        }
    }
}
