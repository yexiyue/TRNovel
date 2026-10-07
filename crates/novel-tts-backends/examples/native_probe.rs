//! Export the exact adapter stream; model files must already be verified locally.
use std::{path::PathBuf, rc::Rc, time::Instant};
use tts_core::backend::{AudioChunk, Backend};
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tokio::task::LocalSet::new().run_until(run()).await
}
async fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() >= 7,
        "backend directory output.wav cpu|cuda|metal text voice"
    );
    let directory = PathBuf::from(&args[2]);
    let text = if let Some(path) = args[5].strip_prefix('@') {
        std::fs::read_to_string(path)?
    } else {
        args[5].clone()
    };
    let device = match args[4].as_str() {
        "cpu" => tts_protocol::Device::Cpu,
        "cuda" => tts_protocol::Device::Cuda,
        "metal" => tts_protocol::Device::Metal,
        _ => anyhow::bail!("invalid device"),
    };
    let load = || async {
        let backend: Rc<dyn Backend> = match args[1].as_str() {
            #[cfg(feature = "moss-nano-candle")]
            "moss-nano-candle" => Rc::new(
                novel_tts_backends::moss::nano::NanoBackend::load_on(directory.clone(), device)
                    .await?,
            ),
            #[cfg(feature = "moss")]
            "moss-nano-onnx" => Rc::new(
                novel_tts_backends::moss::MossBackend::load_on(directory.join("moss"), device)
                    .await?,
            ),
            #[cfg(feature = "moss-candle")]
            "moss" => Rc::new(
                novel_tts_backends::moss::candle::CandleBackend::load_on(
                    directory.clone(),
                    novel_tts_backends::moss::candle::Mode::parse(
                        &std::env::var("MOSS_MODEL").unwrap_or_else(|_| "realtime-1.7b".into()),
                    )?,
                    device,
                )
                .await?,
            ),
            #[cfg(feature = "qwen")]
            "qwen" => Rc::new(
                novel_tts_backends::qwen::QwenBackend::load_on(directory.clone(), device).await?,
            ),
            #[cfg(feature = "voxcpm")]
            "voxcpm" => Rc::new(
                novel_tts_backends::voxcpm::VoxBackend::load_on(directory.clone(), device).await?,
            ),
            #[cfg(feature = "omnivoice")]
            "omnivoice" => Rc::new(
                novel_tts_backends::omnivoice::OmniBackend::load_on(directory.clone(), device)
                    .await?,
            ),
            _ => anyhow::bail!("backend not compiled"),
        };
        Ok::<_, anyhow::Error>(backend)
    };
    if std::env::var_os("NOVEL_TTS_PROBE_CANCEL_LOAD").is_some() {
        let started = Instant::now();
        let cancelled = tokio::time::timeout(std::time::Duration::from_millis(100), load()).await;
        println!(
            "{}",
            serde_json::json!({"cancelled_loading":cancelled.is_err(),"cancel_load_and_release_ms":started.elapsed().as_millis()})
        );
        // A completed initializer is owned and released before the next load as well.
        drop(cancelled);
    }
    let started = Instant::now();
    let backend = load().await?;
    let load_ms = started.elapsed().as_millis();
    if std::env::var_os("NOVEL_TTS_PROBE_CANCEL").is_some() {
        let mut stream = backend.stream(&text, &args[6]).await?;
        anyhow::ensure!(
            matches!(stream.recv().await.transpose()?, Some(AudioChunk::Pcm(_))),
            "no PCM before cancel"
        );
        drop(stream);
    }
    if std::env::var_os("NOVEL_TTS_PROBE_CANCEL_EARLY").is_some() {
        let stream = backend.stream(&text, &args[6]).await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(stream);
    }
    let started = Instant::now();
    let style = std::env::var("NOVEL_TTS_PROBE_STYLE").ok();
    let mut first = None;
    let mut audio = None;
    let segments = backend.segments(&text).await?;
    anyhow::ensure!(!segments.is_empty(), "empty input");
    let mut boundaries = Vec::new();
    for segment in &segments {
        let offset = audio
            .as_ref()
            .map_or(0, |pcm: &tts_core::backend::Pcm| pcm.samples.len());
        let mut stream = backend
            .stream_with_style(&segment.text, &args[6], style.as_deref())
            .await?;
        let mut ended = false;
        while let Some(chunk) = stream.recv().await {
            match chunk? {
                AudioChunk::Pcm(pcm) => {
                    pcm.duration_ms()?;
                    first.get_or_insert_with(|| started.elapsed().as_millis());
                    if let Some(samples) = &mut audio {
                        let samples: &mut tts_core::backend::Pcm = samples;
                        anyhow::ensure!(
                            samples.sample_rate == pcm.sample_rate
                                && samples.channels == pcm.channels,
                            "PCM format changed"
                        );
                        samples.samples.extend(pcm.samples);
                    } else {
                        audio = Some(pcm);
                    }
                }
                AudioChunk::End => {
                    ended = true;
                    break;
                }
            }
        }
        anyhow::ensure!(ended, "stream did not end normally");
        let end = audio.as_ref().map_or(0, |pcm| pcm.samples.len());
        anyhow::ensure!(end > offset, "segment produced no PCM");
        boundaries.push((offset, end, &segment.text));
    }
    let audio = audio.ok_or_else(|| anyhow::anyhow!("empty PCM"))?;
    let generate_ms = started.elapsed().as_millis();
    let duration = audio.samples.len() as f64 / audio.sample_rate as f64 / audio.channels as f64;
    let spec = hound::WavSpec {
        channels: audio.channels,
        sample_rate: audio.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    write_wav(std::path::Path::new(&args[3]), spec, &audio.samples)?;
    let output = PathBuf::from(&args[3]);
    let directory = output.with_file_name(format!(
        "{}-segments",
        output.file_stem().unwrap().to_string_lossy()
    ));
    std::fs::create_dir_all(&directory)?;
    let mut index = Vec::new();
    for (number, (start, end, text)) in boundaries.iter().enumerate() {
        let filename = format!("{:03}.wav", number + 1);
        write_wav(
            &directory.join(&filename),
            spec,
            &audio.samples[*start..*end],
        )?;
        index.push(
            serde_json::json!({"file":filename,"text":text,"start_sample":start,"end_sample":end}),
        );
    }
    std::fs::write(
        directory.join("index.json"),
        serde_json::to_vec_pretty(&index)?,
    )?;
    println!(
        "{}",
        serde_json::json!({"backend":args[1],"device":format!("{device:?}"),"load_ms":load_ms,"first_pcm_ms":first,"generate_ms":generate_ms,"audio_seconds":duration,"segments":segments.len(),"rtf":generate_ms as f64 / 1000.0 / duration,"eos":true})
    );
    if std::env::var_os("NOVEL_TTS_PROBE_DROP_EARLY").is_some() {
        let stream = backend.stream(&text, &args[6]).await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(stream);
        let cancelled = Instant::now();
        drop(backend);
        println!(
            "{}",
            serde_json::json!({"cancel_and_release_ms":cancelled.elapsed().as_millis()})
        );
    }
    Ok(())
}

fn write_wav(path: &std::path::Path, spec: hound::WavSpec, samples: &[f32]) -> anyhow::Result<()> {
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}
