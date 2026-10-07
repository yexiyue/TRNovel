//! Serial synthesis, duration learning and boundary processing ahead of playback.
use super::*;
pub(super) enum Item {
    Start(TextRange),
    Audio(Packet),
    End(TextRange),
    Finished,
}
pub(super) fn spawn(
    backend: Rc<dyn Backend>,
    source_text: Arc<str>,
    first: Option<crate::text::TextSegment>,
    voice: String,
    style: Option<String>,
) -> (AbortOnDrop, mpsc::Receiver<Result<Item, SessionError>>) {
    let (tx, rx) = mpsc::channel(1);
    let task = AbortOnDrop(tokio::task::spawn_local(async move {
        let result: Result<(), SessionError> = async {
            let budget = Budget::default();
            let mut next = first;
            while let Some(segment) = next.take() {
                let range = TextRange {
                    start: segment.start,
                    end: segment.end,
                };
                if !range.is_valid(&source_text) || range.start >= range.end {
                    return Err(SessionError::Invalid("invalid backend source range".into()));
                }
                let paragraph_end =
                    backend.paragraph_end(&segment.text, &source_text[segment.end..]);
                let mut silence = crate::audio::BoundarySilence::default();
                tx.send(Ok(Item::Start(range)))
                    .await
                    .map_err(|_| SessionError::Disconnected)?;
                let mut audio = backend
                    .stream_with_style(&segment.text, &voice, style.as_deref())
                    .await?;
                let mut ended = false;
                let mut generated_seconds = 0.0;
                while let Some(chunk) = audio.recv().await {
                    match chunk? {
                        crate::backend::AudioChunk::Pcm(pcm) => {
                            generated_seconds += pcm.duration_ms()? as f64 / 1000.0;
                            let Some(pcm) = silence.push(pcm)? else {
                                continue;
                            };
                            let packet = budget.acquire(pcm).await?;
                            tx.send(Ok(Item::Audio(packet)))
                                .await
                                .map_err(|_| SessionError::Disconnected)?;
                        }
                        crate::backend::AudioChunk::End => {
                            if let Some(pcm) = silence.finish(paragraph_end)? {
                                let packet = budget.acquire(pcm).await?;
                                tx.send(Ok(Item::Audio(packet)))
                                    .await
                                    .map_err(|_| SessionError::Disconnected)?;
                            }
                            ended = true;
                            break;
                        }
                    }
                }
                if !ended {
                    return Err(SessionError::Invalid(
                        "synthesis stream ended without completion".into(),
                    ));
                }
                backend.observe_duration(&segment.text, &voice, generated_seconds);
                tx.send(Ok(Item::End(range)))
                    .await
                    .map_err(|_| SessionError::Disconnected)?;
                next = if range.end == source_text.len() {
                    None
                } else {
                    backend
                        .next_segment(&source_text[range.end..])
                        .await?
                        .map(|mut segment| {
                            segment.start += range.end;
                            segment.end += range.end;
                            segment
                        })
                };
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                let _ = tx.send(Ok(Item::Finished)).await;
            }
            Err(error) => {
                let _ = tx.send(Err(error)).await;
            }
        }
    }));
    (task, rx)
}
