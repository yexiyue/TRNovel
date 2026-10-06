use super::Playback;
/// An output device and its queue remain owned by the listening thread.
pub struct AudioPlayer {
    stream: rodio::OutputStream,
    sink: std::cell::RefCell<rodio::Sink>,
}

impl AudioPlayer {
    /// Open the device only when the user initiates listening.
    pub fn open() -> Result<Self, crate::backend::BackendError> {
        let mut stream = rodio::OutputStreamBuilder::open_default_stream()
            .map_err(|error| crate::backend::BackendError::Initialize(error.to_string()))?;
        stream.log_on_drop(false);
        let sink = rodio::Sink::connect_new(stream.mixer());
        Ok(Self {
            stream,
            sink: std::cell::RefCell::new(sink),
        })
    }
}

impl Playback for AudioPlayer {
    fn append(&self, audio: crate::backend::Pcm) {
        self.sink.borrow().append(rodio::buffer::SamplesBuffer::new(
            audio.channels,
            audio.sample_rate,
            audio.samples,
        ));
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
    }
    fn configure(&self, volume: f32, speed: f32) {
        self.sink.borrow().set_volume(volume);
        self.sink.borrow().set_speed(speed);
    }
}
