use super::Playback;
use crate::backend::Pcm;
use rodio::Source;
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

/// Shared PCM avoids copying the same audio for playback and alignment.
struct SharedPcm {
    audio: Arc<Pcm>,
    index: usize,
    base: Duration,
    cursor: Arc<AtomicU64>,
}
impl Iterator for SharedPcm {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.audio.samples.get(self.index).copied()?;
        self.index += 1;
        if self.index.is_multiple_of(self.audio.channels as usize) {
            let elapsed = Duration::from_secs_f64(
                self.index as f64 / self.audio.channels as f64 / self.audio.sample_rate as f64,
            );
            self.cursor
                .store((self.base + elapsed).as_micros() as u64, Ordering::Release);
        }
        Some(sample)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.audio.samples.len() - self.index;
        (left, Some(left))
    }
}
impl Source for SharedPcm {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.audio.samples.len() - self.index)
    }
    fn channels(&self) -> u16 {
        self.audio.channels
    }
    fn sample_rate(&self) -> u32 {
        self.audio.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.audio.samples.len() as f64
                / self.audio.channels as f64
                / self.audio.sample_rate as f64,
        ))
    }
}
/// Output and queue remain on their owner thread; the mixer reports source progress.
pub struct AudioPlayer {
    stream: rodio::OutputStream,
    sink: RefCell<rodio::Sink>,
    cursor: RefCell<Arc<AtomicU64>>,
    queued: RefCell<Duration>,
}
impl AudioPlayer {
    pub fn open() -> Result<Self, crate::backend::BackendError> {
        let mut stream = rodio::OutputStreamBuilder::open_default_stream()
            .map_err(|e| crate::backend::BackendError::Initialize(e.to_string()))?;
        stream.log_on_drop(false);
        let sink = rodio::Sink::connect_new(stream.mixer());
        Ok(Self {
            stream,
            sink: RefCell::new(sink),
            cursor: RefCell::new(Arc::new(AtomicU64::new(0))),
            queued: RefCell::new(Duration::ZERO),
        })
    }
}
impl Playback for AudioPlayer {
    fn append(&self, audio: Arc<Pcm>) {
        let mut queued = self.queued.borrow_mut();
        let source = SharedPcm {
            audio,
            index: 0,
            base: *queued,
            cursor: self.cursor.borrow().clone(),
        };
        *queued += source.total_duration().expect("PCM has a finite duration");
        self.sink.borrow().append(source);
    }
    fn position(&self) -> Duration {
        Duration::from_micros(self.cursor.borrow().load(Ordering::Acquire))
    }
    fn is_empty(&self) -> bool {
        self.sink.borrow().empty()
    }
    fn pause(&self) {
        self.sink.borrow().pause();
    }
    fn resume(&self) {
        self.sink.borrow().play();
    }
    fn stop(&self) {
        self.sink.borrow().stop();
        self.sink
            .replace(rodio::Sink::connect_new(self.stream.mixer()));
        // Sources already fetched by the old mixer keep their own cursor.
        // Replacing this cursor prevents their late reads affecting the session.
        self.cursor.replace(Arc::new(AtomicU64::new(0)));
        self.queued.replace(Duration::ZERO);
    }
    fn configure(&self, volume: f32, speed: f32) {
        self.sink.borrow().set_volume(volume);
        self.sink.borrow().set_speed(speed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_source_reports_audio_time_across_packets() {
        let cursor = Arc::new(AtomicU64::new(0));
        let audio = Arc::new(Pcm {
            samples: vec![0.1; 200],
            sample_rate: 1000,
            channels: 2,
        });
        let mut source = SharedPcm {
            audio,
            index: 0,
            base: Duration::from_millis(500),
            cursor: cursor.clone(),
        };
        assert_eq!(source.by_ref().take(100).count(), 100);
        assert_eq!(cursor.load(Ordering::Acquire), 550000);
        assert_eq!(source.count(), 100);
        assert_eq!(cursor.load(Ordering::Acquire), 600000);
    }
}
