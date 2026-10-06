use super::producer::Item;
use super::*;
pub(super) struct Runner {
    pub(super) id: String,
    pub(super) request: StartRequest,
    pub(super) backend: Rc<dyn Backend>,
    pub(super) aligner: Option<Arc<dyn crate::alignment::Aligner>>,
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
        self.emit(Event::AlignmentStatus {
            sentence_highlight: false,
            reason: None,
        })
        .await?;
        self.backend.select_voice(&voice);
        let first = if byte == self.text.len() {
            None
        } else {
            self.backend
                .next_segment(&self.text[byte..])
                .await?
                .map(|mut segment| {
                    segment.start += byte;
                    segment.end += byte;
                    segment
                })
        };
        if let Some(segment) = &first {
            if !(TextRange {
                start: segment.start,
                end: segment.end,
            })
            .is_valid(&self.text)
                || segment.start >= segment.end
            {
                return Err(SessionError::Invalid("invalid backend source range".into()));
            }
            self.position.set(segment.start);
            self.save(segment.start, false).await?;
        }
        let (_producer, mut rx) =
            super::producer::spawn(self.backend.clone(), self.text.clone(), first, voice);
        struct Marker {
            range: TextRange,
            start: Duration,
            end: Option<Duration>,
            started: bool,
            had_audio: bool,
            format: Option<(u32, u16)>,
            blocks: Vec<Arc<Pcm>>,
            leases: Vec<Arc<super::prefetch::Lease>>,
            timeline: Vec<crate::alignment::SentenceTiming>,
            sentence: usize,
            sentence_started: bool,
            fallback: Option<String>,
        }
        let mut completed = false;
        let mut markers = std::collections::VecDeque::<Marker>::new();
        let mut pending = std::collections::VecDeque::new();
        let mut queued = Duration::ZERO;
        let (alignment_tx, mut alignment_rx) = mpsc::channel::<(
            TextRange,
            Result<Vec<crate::alignment::SentenceTiming>, String>,
        )>(1);
        let mut alignment_task: Option<AbortOnDrop> = None;
        loop {
            if let Ok((range, result)) = alignment_rx.try_recv() {
                alignment_task.take();
                let current = markers
                    .front()
                    .is_some_and(|marker| marker.range == range && marker.started);
                if let Some(marker) = markers.iter_mut().find(|marker| marker.range == range) {
                    match result {
                        Ok(timeline) => {
                            let speech = crate::alignment::SpeechText::from_source(
                                &self.text[range.start..range.end],
                                range.start,
                            );
                            let frames =
                                marker.end.zip(marker.format).map_or(0, |(end, (rate, _))| {
                                    (end.saturating_sub(marker.start).as_secs_f64() * rate as f64)
                                        .round() as u64
                                });
                            if valid_timeline(&timeline, &speech, frames) {
                                marker.timeline = timeline;
                            } else {
                                marker.fallback =
                                    Some("aligner returned an invalid sentence timeline".into());
                            }
                        }
                        Err(reason) => {
                            marker.fallback = Some(reason);
                        }
                    }
                    if current && marker.fallback.is_some() {
                        self.emit(Event::AlignmentStatus {
                            sentence_highlight: false,
                            reason: marker.fallback.clone(),
                        })
                        .await?;
                    }
                }
            }
            if !self.paused.get() {
                let cursor = if self.player.is_empty() {
                    queued
                } else {
                    self.player.position()
                };
                while pending.front().is_some_and(|(end, _)| *end <= cursor) {
                    pending.pop_front();
                }
                while let Some(marker) = markers.front_mut() {
                    if marker.had_audio && !marker.started && marker.start <= cursor {
                        marker.started = true;
                        self.position.set(marker.range.start);
                        self.save(marker.range.start, false).await?;
                        self.phase.set(SessionState::Playing);
                        self.emit(Event::AlignmentStatus {
                            sentence_highlight: false,
                            reason: marker.fallback.clone(),
                        })
                        .await?;
                        self.emit(Event::SegmentStarted {
                            range: marker.range,
                            text_hash: self.request.text_hash.clone(),
                        })
                        .await?;
                        self.emit(Event::SessionState {
                            state: SessionState::Playing,
                        })
                        .await?;
                    }
                    if let Some((sample_rate, _)) = marker.format {
                        let frame =
                            cursor.saturating_sub(marker.start).as_secs_f64() * sample_rate as f64;
                        while let Some(sentence) = marker.timeline.get(marker.sentence) {
                            if frame < sentence.start_frame as f64 {
                                break;
                            }
                            if !marker.sentence_started {
                                // Late alignment must not move the source cursor backwards.
                                if sentence.range.start >= self.position.get()
                                    && frame < sentence.end_frame as f64
                                {
                                    self.position.set(sentence.range.start);
                                    self.emit(Event::AlignmentStatus {
                                        sentence_highlight: true,
                                        reason: None,
                                    })
                                    .await?;
                                    self.emit(Event::SentenceStarted {
                                        range: sentence.range,
                                        text_hash: self.request.text_hash.clone(),
                                    })
                                    .await?;
                                }
                                marker.sentence_started = true;
                            }
                            if frame < sentence.end_frame as f64 {
                                break;
                            }
                            if sentence.range.end > self.position.get() {
                                self.position.set(sentence.range.end);
                                self.save(sentence.range.end, false).await?;
                                self.emit(Event::SentenceFinished {
                                    range: sentence.range,
                                    text_hash: self.request.text_hash.clone(),
                                })
                                .await?;
                            }
                            marker.sentence += 1;
                            marker.sentence_started = false;
                        }
                    }
                    if !marker.end.is_some_and(|end| end <= cursor) {
                        break;
                    }
                    let marker = markers.pop_front().expect("front exists");
                    self.position.set(marker.range.end);
                    self.save(marker.range.end, false).await?;
                    self.emit(Event::SegmentFinished {
                        range: marker.range,
                        text_hash: self.request.text_hash.clone(),
                    })
                    .await?;
                }
            }
            if !self.paused.get() && !completed {
                let phase = if self.player.is_empty() {
                    SessionState::Generating
                } else {
                    SessionState::Playing
                };
                if phase != self.phase.get() {
                    self.phase.set(phase);
                    self.emit(Event::SessionState { state: phase }).await?;
                }
            }
            if completed && markers.is_empty() {
                break;
            }
            let item = tokio::select! {
                item = rx.recv(), if !completed => item,
                _ = tokio::time::sleep(Duration::from_millis(10)) => continue,
            };
            let Some(item) = item else {
                return Err(SessionError::Invalid(
                    "audio producer disconnected before completion".into(),
                ));
            };
            match item? {
                Item::Finished => completed = true,
                Item::Start(range) => {
                    if !range.is_valid(&self.request.text) || range.start >= range.end {
                        return Err(SessionError::Invalid("invalid source range".into()));
                    }
                    markers.push_back(Marker {
                        range,
                        start: queued,
                        end: None,
                        started: false,
                        had_audio: false,
                        format: None,
                        blocks: Vec::new(),
                        leases: Vec::new(),
                        timeline: Vec::new(),
                        sentence: 0,
                        sentence_started: false,
                        fallback: None,
                    });
                }
                Item::Audio(packet) => {
                    let marker = markers.back_mut().ok_or_else(|| {
                        SessionError::Invalid("audio without source marker".into())
                    })?;
                    let format = (packet.audio.sample_rate, packet.audio.channels);
                    if marker.format.is_some_and(|old| old != format) {
                        return Err(SessionError::Invalid(
                            "PCM format changed inside segment".into(),
                        ));
                    }
                    marker.format = Some(format);
                    queued += Duration::from_secs_f64(
                        packet.audio.samples.len() as f64
                            / packet.audio.channels as f64
                            / packet.audio.sample_rate as f64,
                    );
                    let pcm = Arc::new(packet.audio);
                    self.player.append(pcm.clone());
                    if self.aligner.is_some() {
                        marker.blocks.push(pcm);
                        marker.leases.push(packet.lease.clone());
                    }
                    pending.push_back((queued, packet.lease));
                    marker.had_audio = true;
                }
                Item::End(range) => {
                    let marker = markers.back_mut().ok_or_else(|| {
                        SessionError::Invalid("completion without source marker".into())
                    })?;
                    if marker.range != range || !marker.had_audio {
                        return Err(SessionError::Invalid(
                            "backend produced no audio or invalid completion".into(),
                        ));
                    }
                    marker.end = Some(queued);
                    if alignment_task.is_none()
                        && (self.paused.get() || self.player.position() < queued)
                        && let Some(aligner) = self.aligner.clone()
                    {
                        let (sample_rate, channels) = marker.format.expect("validated PCM");
                        let audio = crate::alignment::AudioClip {
                            blocks: std::mem::take(&mut marker.blocks),
                            sample_rate,
                            channels,
                            retention: std::mem::take(&mut marker.leases)
                                .into_iter()
                                .map(|lease| lease as Arc<dyn Send + Sync + std::fmt::Debug>)
                                .collect(),
                        };
                        let text = crate::alignment::SpeechText::from_source(
                            &self.text[range.start..range.end],
                            range.start,
                        );
                        let sender = alignment_tx.clone();
                        alignment_task = Some(AbortOnDrop(tokio::task::spawn_local(async move {
                            let result = tokio::time::timeout(
                                Duration::from_secs(20),
                                aligner.align(&text, &audio),
                            )
                            .await
                            .map_err(|_| "alignment timed out".to_string())
                            .and_then(|result| result);
                            let _ = sender.send((range, result)).await;
                        })));
                    } else {
                        if self.aligner.is_some() && alignment_task.is_some() {
                            marker.fallback =
                                Some("alignment backlog; using fragment highlight".into());
                        }
                        marker.blocks.clear();
                        marker.leases.clear();
                    }
                }
            }
        }
        self.save(self.request.text.len(), true).await?;
        Ok(())
    }
}

fn valid_timeline(
    timeline: &[crate::alignment::SentenceTiming],
    speech: &crate::alignment::SpeechText,
    frames: u64,
) -> bool {
    timeline.len() == speech.sentences.len()
        && !timeline.is_empty()
        && timeline
            .iter()
            .zip(&speech.sentences)
            .all(|(timing, sentence)| {
                timing.range == sentence.range
                    && timing.start_frame < timing.end_frame
                    && timing.end_frame <= frames
            })
        && timeline
            .windows(2)
            .all(|pair| pair[0].end_frame <= pair[1].start_frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeline_rejects_out_of_bounds_reversed_and_mismatched_sentences() {
        use crate::alignment::{SentenceTiming, SpeechText};
        let speech = SpeechText::from_source("你好。再见。", 10);
        let mut timeline = vec![
            SentenceTiming {
                range: speech.sentences[0].range,
                start_frame: 0,
                end_frame: 100,
            },
            SentenceTiming {
                range: speech.sentences[1].range,
                start_frame: 100,
                end_frame: 200,
            },
        ];
        assert!(valid_timeline(&timeline, &speech, 200));
        assert!(!valid_timeline(&timeline, &speech, 199));
        timeline[1].start_frame = 99;
        assert!(!valid_timeline(&timeline, &speech, 200));
        timeline[1].start_frame = 200;
        assert!(!valid_timeline(&timeline, &speech, 200));
        timeline[1].start_frame = 100;
        timeline[1].range.end += 1;
        assert!(!valid_timeline(&timeline, &speech, 200));
    }
}
