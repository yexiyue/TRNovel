use super::*;
/// Pending plus active audio is limited to 30 seconds and 16 MiB.
#[derive(Clone)]
pub(super) struct Budget {
    milliseconds: Arc<Semaphore>,
    bytes: Arc<Semaphore>,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            milliseconds: Arc::new(Semaphore::new(30_000)),
            bytes: Arc::new(Semaphore::new(16 * 1024 * 1024)),
        }
    }
}
pub(super) struct Packet {
    pub(super) audio: Pcm,
    pub(super) segment: TextSegment,
    _time: OwnedSemaphorePermit,
    _bytes: OwnedSemaphorePermit,
}

impl Budget {
    pub(super) async fn acquire(
        &self,
        audio: Pcm,
        segment: TextSegment,
    ) -> Result<Packet, SessionError> {
        let duration = audio.duration_ms()?;
        let bytes = u32::try_from(audio.samples.len() * size_of::<f32>())
            .map_err(|_| SessionError::Invalid("PCM memory overflow".into()))?;
        if duration > 30_000 || bytes > 16 * 1024 * 1024 {
            return Err(SessionError::Invalid(
                "single segment exceeds audio prefetch budget".into(),
            ));
        }
        let time = self
            .milliseconds
            .clone()
            .acquire_many_owned(duration)
            .await
            .map_err(|_| SessionError::Disconnected)?;
        let bytes = self
            .bytes
            .clone()
            .acquire_many_owned(bytes)
            .await
            .map_err(|_| SessionError::Disconnected)?;
        Ok(Packet {
            audio,
            segment,
            _time: time,
            _bytes: bytes,
        })
    }
}
