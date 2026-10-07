//! Thread-local session state machine with bounded prefetch and durable text positions.
use crate::{
    Playback,
    backend::{Backend, BackendError, Pcm},
    checkpoint::{CheckpointError, CheckpointStore},
};
use std::{
    cell::Cell,
    collections::HashSet,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, mpsc},
    task::JoinHandle,
};
use tts_protocol::{
    Config, EndReason, ErrorInfo, Event, SessionState, StartRequest, TextRange, text_hash,
};

/// Per-session event before the transport adds instance IDs and sequence numbers.
#[derive(Debug)]
pub struct SessionEvent {
    pub session_id: String,
    pub event: Event,
}

/// Errors preserve the failing stage and never implicitly skip text.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error(transparent)]
    Checkpoint(#[from] CheckpointError),
    #[error("invalid session: {0}")]
    Invalid(String),
    #[error("event consumer disconnected")]
    Disconnected,
}

mod buffering;
mod prefetch;
mod producer;
use prefetch::{Budget, Packet};

struct AbortOnDrop(JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Default)]
struct PendingWrites {
    count: AtomicUsize,
    idle: tokio::sync::Notify,
}
struct WriteGuard(Arc<PendingWrites>);
impl Drop for WriteGuard {
    fn drop(&mut self) {
        if self.0.count.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.idle.notify_one();
        }
    }
}

struct Job {
    id: String,
    request: StartRequest,
    paused: Rc<Cell<bool>>,
    phase: Rc<Cell<SessionState>>,
    buffering: Rc<Cell<bool>>,
    speed: Rc<Cell<f32>>,
    position: Rc<Cell<usize>>,
    terminal: Rc<Cell<bool>>,
    writes: Arc<PendingWrites>,
    voice: String,
    style: Option<String>,
    task: Option<AbortOnDrop>,
}

/// Runs inside a Tokio LocalSet; audio/model objects never move between threads.
pub struct SessionManager {
    backend: Rc<dyn Backend>,
    aligner: Option<Arc<dyn crate::alignment::Aligner>>,
    player: Rc<dyn Playback>,
    checkpoints: CheckpointStore,
    events: mpsc::Sender<SessionEvent>,
    job: Option<Job>,
    used_ids: HashSet<String>,
}

impl SessionManager {
    /// Inject real or test implementations with an explicit event consumer.
    pub fn new(
        backend: Rc<dyn Backend>,
        player: Rc<dyn Playback>,
        checkpoints: CheckpointStore,
        events: mpsc::Sender<SessionEvent>,
    ) -> Self {
        Self {
            backend,
            aligner: None,
            player,
            checkpoints,
            events,
            job: None,
            used_ids: HashSet::new(),
        }
    }

    /// Attach an independent alignment engine before starting playback.
    pub fn with_aligner(mut self, aligner: Arc<dyn crate::alignment::Aligner>) -> Self {
        self.aligner = Some(aligner);
        self
    }

    /// Start explicitly. Restoring a checkpoint never starts this method on its own.
    pub async fn start(
        &mut self,
        id: String,
        request: StartRequest,
        config: &Config,
    ) -> Result<(), SessionError> {
        if id.is_empty()
            || self.used_ids.contains(&id)
            || self.used_ids.len() >= 65_536
            || request.text_hash != text_hash(&request.text)
        {
            return Err(SessionError::Invalid(
                "missing, reused or exhausted session ID, or text digest mismatch".into(),
            ));
        }
        let mut byte = request.resume_byte.unwrap_or(0);
        if request.restore_checkpoint
            && request.resume_byte.is_none()
            && let Some(checkpoint) = self.checkpoints.load(&request.source, &request.text)?
        {
            byte = if checkpoint.completed {
                0
            } else {
                checkpoint.resume_byte
            };
        }
        if byte > request.text.len() || !request.text.is_char_boundary(byte) {
            return Err(SessionError::Invalid(
                "invalid UTF-8 resume position".into(),
            ));
        }
        crate::config::validate(config, &self.backend.capabilities())
            .map_err(|error| SessionError::Invalid(error.to_string()))?;
        self.stop().await?;
        self.player.configure(config.volume, config.speed);
        self.player.pause();
        self.used_ids.insert(id.clone());
        let phase = Rc::new(Cell::new(SessionState::Generating));
        let position = Rc::new(Cell::new(byte));
        let paused = Rc::new(Cell::new(false));
        let terminal = Rc::new(Cell::new(false));
        let buffering = Rc::new(Cell::new(true));
        let speed = Rc::new(Cell::new(config.speed));
        let writes = Arc::new(PendingWrites::default());
        let runner = Runner {
            id: id.clone(),
            request: request.clone(),
            backend: self.backend.clone(),
            aligner: self.aligner.clone(),
            player: self.player.clone(),
            checkpoints: self.checkpoints.clone(),
            events: self.events.clone(),
            paused: paused.clone(),
            phase: phase.clone(),
            buffering: buffering.clone(),
            speed: speed.clone(),
            position: position.clone(),
            text: Arc::from(request.text.as_str()),
            writes: writes.clone(),
        };
        let voice = config.voice.clone();
        let style = config.style.clone();
        let job_style = style.clone();
        let finished = terminal.clone();
        let final_phase = phase.clone();
        let job_voice = voice.clone();
        let task = tokio::task::spawn_local(async move {
            let result = runner.run(byte, voice, style).await;
            if let Err(error) = &result {
                runner.player.stop();
                let _ = runner
                    .emit(Event::Error(ErrorInfo {
                        code: "session_failed".into(),
                        stage: "listening".into(),
                        message: error.to_string(),
                        retryable: true,
                    }))
                    .await;
            }
            finished.set(true);
            final_phase.set(if result.is_ok() {
                SessionState::Stopped
            } else {
                SessionState::Failed
            });
            let reason = if result.is_ok() {
                EndReason::Completed
            } else {
                EndReason::Failed
            };
            let _ = runner
                .emit(Event::SessionEnded {
                    reason,
                    text_hash: runner.request.text_hash.clone(),
                })
                .await;
        });
        self.job = Some(Job {
            id,
            request,
            paused,
            phase,
            buffering,
            speed,
            position,
            terminal,
            writes,
            voice: job_voice,
            style: job_style,
            task: Some(AbortOnDrop(task)),
        });
        Ok(())
    }

    /// Pause actual playback while allowing only bounded prefetch.
    pub async fn pause(&self, id: &str) -> Result<(), SessionError> {
        let job = self.active(id)?;
        job.paused.set(true);
        self.player.pause();
        self.emit(
            id,
            Event::SessionState {
                state: SessionState::Paused,
            },
        )
        .await
    }

    /// Continue the same playback position.
    pub async fn resume(&self, id: &str) -> Result<(), SessionError> {
        let job = self.active(id)?;
        job.paused.set(false);
        if !job.buffering.get() {
            self.player.resume();
        }
        self.emit(
            id,
            Event::SessionState {
                state: job.phase.get(),
            },
        )
        .await
    }

    /// Current playback state is owned by the session, not the transport.
    pub fn status(&self) -> (Option<String>, SessionState) {
        match &self.job {
            Some(job) => (
                Some(job.id.clone()),
                if !job.terminal.get() && job.paused.get() {
                    SessionState::Paused
                } else {
                    job.phase.get()
                },
            ),
            None => (None, SessionState::Idle),
        }
    }

    /// Voice changes replace the session at its unfinished original-text position.
    pub async fn update_settings(
        &mut self,
        new_id: String,
        config: &Config,
    ) -> Result<bool, SessionError> {
        crate::config::validate(config, &self.backend.capabilities())
            .map_err(|error| SessionError::Invalid(error.to_string()))?;
        if let Some(job) = &self.job
            && !job.terminal.get()
            && (job.voice != config.voice || job.style != config.style)
        {
            let paused = job.paused.get();
            let mut request = job.request.clone();
            request.resume_byte = Some(job.position.get());
            request.restore_checkpoint = false;
            self.start(new_id.clone(), request, config).await?;
            if paused {
                self.pause(&new_id).await?;
            }
            return Ok(true);
        }
        if let Some(job) = &self.job {
            job.speed.set(config.speed);
        }
        self.player.configure(config.volume, config.speed);
        Ok(false)
    }

    fn active(&self, id: &str) -> Result<&Job, SessionError> {
        self.job
            .as_ref()
            .filter(|job| job.id == id && !job.terminal.get())
            .ok_or_else(|| SessionError::Invalid("stale or inactive session".into()))
    }

    /// Cancel generation and clear playback, keeping the model and checkpoint.
    pub async fn stop(&mut self) -> Result<(), SessionError> {
        self.player.stop();
        if let Some(job) = &mut self.job {
            if let Some(mut task) = job.task.take() {
                task.0.abort();
                let _ = (&mut task.0).await;
            }
            // A blocking atomic write cannot be aborted. Finish it before a new session
            // writes the same source, so a cancelled session cannot replace newer progress.
            while job.writes.count.load(Ordering::SeqCst) != 0 {
                job.writes.idle.notified().await;
            }
            if !job.terminal.replace(true) {
                job.phase.set(SessionState::Stopped);
                self.events
                    .send(SessionEvent {
                        session_id: job.id.clone(),
                        event: Event::SessionEnded {
                            reason: EndReason::Cancelled,
                            text_hash: job.request.text_hash.clone(),
                        },
                    })
                    .await
                    .map_err(|_| SessionError::Disconnected)?;
            }
        }
        Ok(())
    }

    /// Seek creates a new session and re-synthesizes from a stable original-text byte.
    pub async fn seek(
        &mut self,
        old_id: &str,
        new_id: String,
        byte: usize,
        config: &Config,
    ) -> Result<(), SessionError> {
        let mut request = self.active(old_id)?.request.clone();
        request.resume_byte = Some(byte);
        request.restore_checkpoint = false;
        self.start(new_id, request, config).await
    }

    async fn emit(&self, id: &str, event: Event) -> Result<(), SessionError> {
        self.events
            .send(SessionEvent {
                session_id: id.into(),
                event,
            })
            .await
            .map_err(|_| SessionError::Disconnected)
    }
}

impl Drop for SessionManager {
    fn drop(&mut self) {
        self.player.stop();
        self.job.take();
    }
}

mod playback;
use playback::Runner;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backend::{AudioChunk, Streaming},
        text::preprocess_text,
    };
    use tts_protocol::{Capabilities, SourceId};

    struct FakeBackend {
        calls: Cell<usize>,
        fail_at: Option<usize>,
    }
    impl Backend for FakeBackend {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                model: None,
                model_name: String::new(),
                default_voice: "Weiguo".into(),
                voice_names: Default::default(),
                backend: "moss".into(),
                voices: vec!["Weiguo".into(), "Zf002".into()],
                native_streaming: false,
                style: false,
                compiled_devices: Vec::new(),
                cloning: false,
                pronunciation: false,
            }
        }
        fn stream<'a>(&'a self, _: &'a str, _: &'a str) -> Streaming<'a> {
            Box::pin(async move {
                let index = self.calls.get();
                self.calls.set(index + 1);
                if self.fail_at == Some(index) {
                    return Err(BackendError::Synthesis("injected failure".into()));
                }
                let (tx, rx) = mpsc::channel(2);
                tx.send(Ok(AudioChunk::Pcm(Pcm {
                    samples: vec![0.1; 240],
                    sample_rate: 24000,
                    channels: 1,
                })))
                .await
                .unwrap();
                tx.send(Ok(AudioChunk::End)).await.unwrap();
                Ok(rx)
            })
        }
    }
    #[derive(Default)]
    struct FakePlayer {
        empty: Cell<bool>,
        appended: Cell<usize>,
        cursor: Cell<Duration>,
    }
    impl Playback for FakePlayer {
        fn append(&self, _: Arc<Pcm>) {
            self.appended.set(self.appended.get() + 1);
        }
        fn position(&self) -> Duration {
            self.cursor.get()
        }
        fn is_empty(&self) -> bool {
            self.empty.get()
        }
        fn pause(&self) {}
        fn resume(&self) {}
        fn stop(&self) {}
        fn configure(&self, _: f32, _: f32) {}
    }
    #[derive(Default)]
    struct PausablePlayer {
        paused: Cell<bool>,
        queued: Cell<Duration>,
        cursor: Cell<Duration>,
    }
    impl Playback for PausablePlayer {
        fn append(&self, pcm: Arc<Pcm>) {
            self.queued
                .set(self.queued.get() + Duration::from_millis(pcm.duration_ms().unwrap() as u64));
        }
        fn position(&self) -> Duration {
            self.cursor.get()
        }
        fn is_empty(&self) -> bool {
            self.cursor.get() >= self.queued.get()
        }
        fn pause(&self) {
            self.paused.set(true);
        }
        fn resume(&self) {
            self.paused.set(false);
        }
        fn stop(&self) {
            self.paused.set(true);
        }
        fn configure(&self, _: f32, _: f32) {}
    }
    struct GatedBackend(Arc<tokio::sync::Notify>);
    impl Backend for GatedBackend {
        fn capabilities(&self) -> Capabilities {
            FakeBackend {
                calls: Cell::new(0),
                fail_at: None,
            }
            .capabilities()
        }
        fn stream<'a>(&'a self, _: &'a str, _: &'a str) -> Streaming<'a> {
            Box::pin(async move {
                let (tx, rx) = mpsc::channel(1);
                let gate = self.0.clone();
                tokio::task::spawn_local(async move {
                    for milliseconds in [100, 3000] {
                        tx.send(Ok(AudioChunk::Pcm(Pcm {
                            samples: vec![0.1; milliseconds],
                            sample_rate: 1000,
                            channels: 1,
                        })))
                        .await
                        .unwrap();
                        if milliseconds == 100 {
                            gate.notified().await;
                        }
                    }
                    tx.send(Ok(AudioChunk::End)).await.unwrap();
                });
                Ok(rx)
            })
        }
    }
    #[tokio::test]
    async fn user_resume_cannot_bypass_buffer_and_refill_cannot_override_pause() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let directory = tempfile::tempdir().unwrap();
                let player = Rc::new(PausablePlayer::default());
                let gate = Arc::new(tokio::sync::Notify::new());
                let (tx, mut rx) = mpsc::channel(64);
                let mut manager = SessionManager::new(
                    Rc::new(GatedBackend(gate.clone())),
                    player.clone(),
                    CheckpointStore::new(directory.path()),
                    tx,
                );
                manager
                    .start("buffer".into(), request("正文。"), &Config::default())
                    .await
                    .unwrap();
                loop {
                    if matches!(
                        rx.recv().await.unwrap().event,
                        Event::SessionState {
                            state: SessionState::Buffering
                        }
                    ) {
                        break;
                    }
                }
                manager.resume("buffer").await.unwrap();
                assert!(player.paused.get());
                manager.pause("buffer").await.unwrap();
                gate.notify_one();
                tokio::time::sleep(Duration::from_millis(50)).await;
                assert!(player.paused.get());
                assert_eq!(manager.status().1, SessionState::Paused);
                while let Ok(event) = rx.try_recv() {
                    assert!(!matches!(event.event, Event::SegmentStarted { .. }));
                }
                manager.resume("buffer").await.unwrap();
                assert!(!player.paused.get());
                player.cursor.set(player.queued.get());
                assert_eq!(terminal(&mut rx).await.0, EndReason::Completed);
            })
            .await;
    }
    struct MergedBackend(FakeBackend);
    impl Backend for MergedBackend {
        fn capabilities(&self) -> Capabilities {
            self.0.capabilities()
        }
        fn stream<'a>(&'a self, text: &'a str, voice: &'a str) -> Streaming<'a> {
            self.0.stream(text, voice)
        }
        fn segments<'a>(&'a self, text: &'a str) -> crate::backend::Segmentation<'a> {
            Box::pin(async move {
                Ok(vec![crate::text::TextSegment {
                    text: text.into(),
                    start: 0,
                    end: text.len(),
                }])
            })
        }
    }
    struct FakeAligner;
    impl crate::alignment::Aligner for FakeAligner {
        fn align<'a>(
            &'a self,
            text: &'a crate::alignment::SpeechText,
            _: &'a crate::alignment::AudioClip,
        ) -> crate::alignment::Alignment<'a> {
            Box::pin(async move {
                Ok(text
                    .sentences
                    .iter()
                    .enumerate()
                    .map(|(i, sentence)| crate::alignment::SentenceTiming {
                        range: sentence.range,
                        start_frame: i as u64 * 120,
                        end_frame: (i + 1) as u64 * 120,
                    })
                    .collect())
            })
        }
    }
    #[tokio::test]
    async fn aligned_sentences_follow_playback_clock_and_pause() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let directory = tempfile::tempdir().unwrap();
                let checkpoints = CheckpointStore::new(directory.path());
                let (tx, mut rx) = mpsc::channel(64);
                let player = Rc::new(FakePlayer::default());
                let mut manager = SessionManager::new(
                    Rc::new(MergedBackend(FakeBackend {
                        calls: Cell::new(0),
                        fail_at: None,
                    })),
                    player.clone(),
                    checkpoints.clone(),
                    tx,
                )
                .with_aligner(Arc::new(FakeAligner));
                let source = request("第一句。第二句。");
                manager
                    .start("aligned".into(), source.clone(), &Config::default())
                    .await
                    .unwrap();
                loop {
                    if let Event::SentenceStarted { range, .. } =
                        tokio::time::timeout(Duration::from_secs(2), rx.recv())
                            .await
                            .unwrap()
                            .unwrap()
                            .event
                    {
                        assert_eq!(range.start, 0);
                        break;
                    }
                }
                player.cursor.set(Duration::from_millis(6));
                loop {
                    if let Event::SentenceStarted { range, .. } =
                        tokio::time::timeout(Duration::from_secs(2), rx.recv())
                            .await
                            .unwrap()
                            .unwrap()
                            .event
                    {
                        assert_eq!(range.start, 12);
                        break;
                    }
                }
                assert_eq!(
                    checkpoints
                        .load(&source.source, &source.text)
                        .unwrap()
                        .unwrap()
                        .resume_byte,
                    12
                );
                manager.pause("aligned").await.unwrap();
                player.cursor.set(Duration::from_millis(10));
                tokio::time::sleep(Duration::from_millis(30)).await;
                assert_eq!(manager.job.as_ref().unwrap().position.get(), 12);
                manager.resume("aligned").await.unwrap();
                assert_eq!(terminal(&mut rx).await.0, EndReason::Completed);
            })
            .await;
    }
    fn request(text: &str) -> StartRequest {
        StartRequest {
            source: SourceId {
                namespace: "test".into(),
                book: "book".into(),
                chapter: "1".into(),
            },
            text: text.into(),
            text_hash: text_hash(text),
            resume_byte: None,
            restore_checkpoint: false,
        }
    }
    async fn terminal(rx: &mut mpsc::Receiver<SessionEvent>) -> (EndReason, Vec<Event>) {
        let mut events = Vec::new();
        loop {
            let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap()
                .event;
            if let Event::SessionEnded { reason, .. } = event {
                return (reason, events);
            }
            events.push(event);
        }
    }

    #[tokio::test]
    async fn failures_during_prebuffering_preserve_unplayed_source() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for fail_at in 0..3 {
                    let directory = tempfile::tempdir().unwrap();
                    let player = Rc::new(FakePlayer {
                        empty: Cell::new(true),
                        ..Default::default()
                    });
                    let (tx, mut rx) = mpsc::channel(64);
                    let mut manager = SessionManager::new(
                        Rc::new(FakeBackend {
                            calls: Cell::new(0),
                            fail_at: Some(fail_at),
                        }),
                        player.clone(),
                        CheckpointStore::new(directory.path()),
                        tx,
                    );
                    manager
                        .start(
                            "a".into(),
                            request("第一句。\n第二句。\n第三句。"),
                            &Config::default(),
                        )
                        .await
                        .unwrap();
                    assert_eq!(terminal(&mut rx).await.0, EndReason::Failed);
                    assert_eq!(player.appended.get(), fail_at);
                    manager.stop().await.unwrap();
                    assert!(rx.try_recv().is_err());
                    let mut retry = request("第一句。\n第二句。\n第三句。");
                    let store = CheckpointStore::new(directory.path());
                    let checkpoint = store.load(&retry.source, &retry.text).unwrap().unwrap();
                    let segments = preprocess_text(&retry.text, 200);
                    // Enqueued packets are paused during prebuffering, so none were played.
                    let expected = segments[0].start;
                    assert_eq!(checkpoint.resume_byte, expected);
                    assert!(!checkpoint.completed);
                    retry.restore_checkpoint = true;
                    let (tx, mut rx) = mpsc::channel(64);
                    // A fresh manager models a process restart after the failed attempt.
                    let mut restarted = SessionManager::new(
                        Rc::new(FakeBackend {
                            calls: Cell::new(0),
                            fail_at: None,
                        }),
                        Rc::new(FakePlayer {
                            empty: Cell::new(true),
                            ..Default::default()
                        }),
                        store.clone(),
                        tx,
                    );
                    restarted
                        .start("retry".into(), retry.clone(), &Config::default())
                        .await
                        .unwrap();
                    let (reason, events) = terminal(&mut rx).await;
                    assert_eq!(reason, EndReason::Completed);
                    let first = events.iter().find_map(|event| match event {
                        Event::SegmentStarted { range, .. } => Some(range.start),
                        _ => None,
                    });
                    assert_eq!(first, Some(segments[0].start));
                    assert!(
                        store
                            .load(&retry.source, &retry.text)
                            .unwrap()
                            .unwrap()
                            .completed
                    );
                }
            })
            .await;
    }

    #[tokio::test]
    async fn empty_text_completes_and_bad_resume_is_rejected() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let directory = tempfile::tempdir().unwrap();
                let (tx, mut rx) = mpsc::channel(64);
                let mut manager = SessionManager::new(
                    Rc::new(FakeBackend {
                        calls: Cell::new(0),
                        fail_at: None,
                    }),
                    Rc::new(FakePlayer::default()),
                    CheckpointStore::new(directory.path()),
                    tx,
                );
                manager
                    .start("empty".into(), request("\r\n  "), &Config::default())
                    .await
                    .unwrap();
                assert_eq!(terminal(&mut rx).await.0, EndReason::Completed);
                let mut req = request("中文🙂");
                req.resume_byte = Some(7);
                assert!(
                    manager
                        .start("bad".into(), req, &Config::default())
                        .await
                        .is_err()
                );
            })
            .await;
    }

    #[tokio::test]
    async fn synthesis_does_not_advance_playback_and_cancel_has_one_terminal() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let directory = tempfile::tempdir().unwrap();
                let (tx, mut rx) = mpsc::channel(64);
                let backend = Rc::new(FakeBackend {
                    calls: Cell::new(0),
                    fail_at: None,
                });
                let player = Rc::new(FakePlayer::default());
                let mut manager = SessionManager::new(
                    backend.clone(),
                    player.clone(),
                    CheckpointStore::new(directory.path()),
                    tx,
                );
                manager
                    .start(
                        "old".into(),
                        request("第一句。\n第二句。\n第三句。\n第四句。"),
                        &Config::default(),
                    )
                    .await
                    .unwrap();
                loop {
                    if matches!(rx.recv().await.unwrap().event, Event::SegmentStarted { .. }) {
                        break;
                    }
                }
                manager.pause("old").await.unwrap();
                tokio::time::sleep(Duration::from_millis(40)).await;
                assert!((1..=4).contains(&player.appended.get()));
                assert!(backend.calls.get() <= 4);
                assert_eq!(manager.job.as_ref().unwrap().position.get(), 0);
                manager.stop().await.unwrap();
                assert_eq!(terminal(&mut rx).await.0, EndReason::Cancelled);
                manager.stop().await.unwrap();
                assert!(rx.try_recv().is_err());
                assert!(manager.resume("old").await.is_err());
            })
            .await;
    }

    #[tokio::test]
    async fn seek_and_replacement_cancel_old_sessions_once() {
        tokio::task::LocalSet::new().run_until(async {
            let directory = tempfile::tempdir().unwrap();
            let (tx, mut rx) = mpsc::channel(64);
            let player = Rc::new(FakePlayer::default());
            let mut manager = SessionManager::new(
                Rc::new(FakeBackend { calls: Cell::new(0), fail_at: None }),
                player.clone(), CheckpointStore::new(directory.path()), tx,
            );
            let req = request("第一句。\n第二句。\n第三句。");
            manager.start("first".into(), req.clone(), &Config::default()).await.unwrap();
            loop { if matches!(rx.recv().await.unwrap().event, Event::SegmentStarted{..}) { break; } }
            manager.pause("first").await.unwrap();
            manager.resume("first").await.unwrap();
            assert!(manager.seek("first", "invalid".into(), 1, &Config::default()).await.is_err());
            let byte = preprocess_text(&req.text, 200)[1].start;
            manager.seek("first", "seeked".into(), byte, &Config::default()).await.unwrap();
            let mut cancelled = Vec::new();
            loop {
                let event = rx.recv().await.unwrap();
                if matches!(event.event, Event::SessionEnded{reason: EndReason::Cancelled, ..}) { cancelled.push(event.session_id.clone()); }
                if event.session_id == "seeked" && matches!(event.event, Event::SegmentStarted{..}) {
                    assert!(matches!(event.event, Event::SegmentStarted{range, ..} if range.start == byte));
                    break;
                }
            }
            assert!(manager.pause("first").await.is_err());
            let mut next = request("新章节。 ");
            next.source.chapter = "2".into();
            manager.start("next".into(), next, &Config { voice: "Zf002".into(), ..Config::default() }).await.unwrap();
            manager.stop().await.unwrap();
            manager.stop().await.unwrap();
            while let Ok(event) = rx.try_recv() {
                if matches!(event.event, Event::SessionEnded{reason: EndReason::Cancelled, ..}) { cancelled.push(event.session_id); }
            }
            assert_eq!(cancelled, ["first", "seeked", "next"]);
        }).await;
    }

    #[tokio::test]
    async fn budget_releases_only_after_audio_is_consumed() {
        let budget = Budget::default();
        let audio = || Pcm {
            samples: vec![0.0; 480_000],
            sample_rate: 24_000,
            channels: 1,
        };
        let packet = budget.acquire(audio()).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(10), budget.acquire(audio()))
                .await
                .is_err()
        );
        drop(packet);
        assert!(budget.acquire(audio()).await.is_ok());
    }

    #[tokio::test]
    async fn restart_uses_snapshot_position_after_segmentation_changes() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let directory = tempfile::tempdir().unwrap();
                let store = CheckpointStore::new(directory.path());
                let text = "中文🙂\r\n第二句。";
                let mut req = request(text);
                store.save(&req.source, text, 12, false).unwrap();
                req.restore_checkpoint = true;
                let (tx, mut rx) = mpsc::channel(64);
                let mut manager = SessionManager::new(
                    Rc::new(FakeBackend {
                        calls: Cell::new(0),
                        fail_at: None,
                    }),
                    Rc::new(FakePlayer {
                        empty: Cell::new(true),
                        ..Default::default()
                    }),
                    store,
                    tx,
                );
                manager
                    .start("restored".into(), req, &Config::default())
                    .await
                    .unwrap();
                let (reason, events) = terminal(&mut rx).await;
                assert_eq!(reason, EndReason::Completed);
                let ranges: Vec<_> = events
                    .iter()
                    .filter_map(|event| {
                        if let Event::SegmentStarted { range, .. } = event {
                            Some(*range)
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(ranges, vec![TextRange { start: 12, end: 24 }]);
            })
            .await;
    }
    struct FragmentedBackend {
        ended: bool,
        format_change: bool,
    }
    impl Backend for FragmentedBackend {
        fn capabilities(&self) -> Capabilities {
            FakeBackend {
                calls: Cell::new(0),
                fail_at: None,
            }
            .capabilities()
        }
        fn stream<'a>(&'a self, _: &'a str, _: &'a str) -> Streaming<'a> {
            Box::pin(async move {
                let (tx, rx) = mpsc::channel(4);
                for channels in [1, if self.format_change { 2 } else { 1 }] {
                    tx.send(Ok(AudioChunk::Pcm(Pcm {
                        samples: vec![0.1; 240],
                        sample_rate: 24000,
                        channels,
                    })))
                    .await
                    .unwrap();
                }
                if self.ended {
                    tx.send(Ok(AudioChunk::End)).await.unwrap();
                }
                Ok(rx)
            })
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn fragmented_streams_require_completion_and_consistent_format() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for (ended, format_change, reason) in [
                    (true, false, EndReason::Completed),
                    (false, false, EndReason::Failed),
                    (true, true, EndReason::Failed),
                ] {
                    let directory = tempfile::tempdir().unwrap();
                    let checkpoints = CheckpointStore::new(directory.path());
                    let (tx, mut rx) = mpsc::channel(64);
                    let player = Rc::new(FakePlayer {
                        empty: Cell::new(true),
                        appended: Cell::new(0),
                        cursor: Cell::new(Duration::ZERO),
                    });
                    let mut manager = SessionManager::new(
                        Rc::new(FragmentedBackend {
                            ended,
                            format_change,
                        }),
                        player.clone(),
                        checkpoints.clone(),
                        tx,
                    );
                    let req = request("原文。 ");
                    manager
                        .start("stream".into(), req.clone(), &Config::default())
                        .await
                        .unwrap();
                    let (actual, events) = terminal(&mut rx).await;
                    assert_eq!(actual, reason);
                    assert_eq!(
                        events
                            .iter()
                            .filter(|e| matches!(e, Event::SegmentStarted { .. }))
                            .count(),
                        usize::from(reason == EndReason::Completed)
                    );
                    if reason == EndReason::Completed {
                        assert_eq!(player.appended.get(), 2);
                    } else {
                        assert!(
                            !events
                                .iter()
                                .any(|e| matches!(e, Event::SegmentFinished { .. }))
                        );
                    }
                }
            })
            .await;
    }
}
