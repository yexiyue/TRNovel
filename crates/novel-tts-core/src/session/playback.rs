use super::*;
pub(super) struct Runner {
    pub(super) id: String,
    pub(super) request: StartRequest,
    pub(super) backend: Rc<dyn Backend>,
    pub(super) player: Rc<dyn Playback>,
    pub(super) checkpoints: CheckpointStore,
    pub(super) events: mpsc::Sender<SessionEvent>,
    pub(super) paused: Rc<Cell<bool>>,
    pub(super) phase: Rc<Cell<SessionState>>,
    pub(super) position: Rc<Cell<usize>>,
    pub(super) text: Arc<str>,
    pub(super) writes: Arc<PendingWrites>,
}

impl Runner {
    pub(super) async fn emit(&self, event: Event) -> Result<(), SessionError> {
        self.events
            .send(SessionEvent {
                session_id: self.id.clone(),
                event,
            })
            .await
            .map_err(|_| SessionError::Disconnected)
    }

    async fn save(&self, byte: usize, completed: bool) -> Result<(), SessionError> {
        let checkpoints = self.checkpoints.clone();
        let source = self.request.source.clone();
        let text = self.text.clone();
        self.writes.count.fetch_add(1, Ordering::SeqCst);
        let pending = WriteGuard(self.writes.clone());
        tokio::task::spawn_blocking(move || {
            let _pending = pending;
            checkpoints.save(&source, &text, byte, completed)
        })
        .await
        .map_err(|error| SessionError::Invalid(error.to_string()))??;
        Ok(())
    }

    pub(super) async fn run(&self, byte: usize, voice: String) -> Result<(), SessionError> {
        self.emit(Event::SessionState {
            state: if self.paused.get() {
                SessionState::Paused
            } else {
                self.phase.get()
            },
        })
        .await?;
        let segments = preprocess_text(&self.request.text, 200)
            .into_iter()
            .filter(|segment| segment.end > byte)
            .collect::<Vec<_>>();
        // Persist the first unfinished range before synthesis, including seeks
        // and an error in the very first generated segment.
        if let Some(first) = segments.first() {
            self.position.set(first.start);
            self.save(first.start, false).await?;
        }
        let (tx, mut rx) = mpsc::channel::<(usize, Result<Packet, SessionError>)>(1);
        let backend = self.backend.clone();
        let _producer = AbortOnDrop(tokio::task::spawn_local(async move {
            let budget = Budget::default();
            for segment in segments {
                // Reserve the channel slot before allocating the next audio buffer.
                let Ok(permit) = tx.reserve().await else {
                    break;
                };
                let start = segment.start;
                let result = match backend.synthesize(&segment.text, &voice).await {
                    Ok(audio) => budget.acquire(audio, segment).await,
                    Err(error) => Err(error.into()),
                };
                let failed = result.is_err();
                permit.send((start, result));
                if failed {
                    break;
                }
            }
        }));
        while let Some((start, result)) = rx.recv().await {
            if result.is_err() {
                self.save(start, false).await?;
            }
            let packet = result?;
            while self.paused.get() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let range = TextRange {
                start: packet.segment.start,
                end: packet.segment.end,
            };
            if !range.is_valid(&self.request.text) {
                return Err(SessionError::Invalid(
                    "segment has invalid original-text range".into(),
                ));
            }
            self.position.set(range.start);
            self.save(range.start, false).await?;
            while self.paused.get() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            self.phase.set(SessionState::Playing);
            self.player.append(packet.audio);
            self.emit(Event::SegmentStarted {
                range,
                text_hash: self.request.text_hash.clone(),
            })
            .await?;
            self.emit(Event::SessionState {
                state: if self.paused.get() {
                    SessionState::Paused
                } else {
                    self.phase.get()
                },
            })
            .await?;
            while self.paused.get() || !self.player.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            self.position.set(range.end);
            self.phase.set(SessionState::Generating);
            self.save(range.end, false).await?;
            self.emit(Event::SegmentFinished {
                range,
                text_hash: self.request.text_hash.clone(),
            })
            .await?;
            self.emit(Event::SessionState {
                state: if self.paused.get() {
                    SessionState::Paused
                } else {
                    self.phase.get()
                },
            })
            .await?;
            // Permits are released here, only after the audio is actually consumed.
        }
        self.save(self.request.text.len(), true).await?;
        Ok(())
    }
}
