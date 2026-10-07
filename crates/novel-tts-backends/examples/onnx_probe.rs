//! Real adapter benchmark; construction and synthesis timings are separated.
use novel_tts_backends::{Registry, omnivoice, omnivoice_onnx, qwen, qwen_onnx};
use std::{path::PathBuf, rc::Rc, time::Instant};
use tts_core::backend::{AudioChunk, Backend};
use tts_protocol::Device;
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() >= 8,
        "backend model root output.wav cpu|coreml|metal text voice [style]"
    );
    let root = PathBuf::from(&args[3]);
    let device = match args[5].as_str() {
        "cpu" => Device::Cpu,
        "coreml" => Device::Coreml,
        "metal" => Device::Metal,
        _ => anyhow::bail!("unknown device"),
    };
    let start = Instant::now();
    let backend: Rc<dyn Backend> = match args[1].as_str() {
        "qwen-onnx" => {
            Rc::new(qwen_onnx::load(&root, qwen_onnx::model(Some(&args[2]))?, device).await?)
        }
        "omnivoice-onnx" => Rc::new(omnivoice_onnx::load(&root, device).await?),
        "qwen" => Rc::new(
            qwen::QwenBackend::load_on(
                qwen::models::Model::parse(Some(&args[2]))?.directory(&root),
                device,
            )
            .await?,
        ),
        "omnivoice" => {
            Rc::new(omnivoice::OmniBackend::load_on(omnivoice::directory(&root), device).await?)
        }
        _ => anyhow::bail!("unknown backend"),
    };
    let load_ms = start.elapsed().as_secs_f64() * 1000.;
    let _catalog = Registry::new(Some(root))?.catalog()?;
    let style = args.get(8).map(String::as_str);
    if std::env::var_os("NOVEL_TTS_PROBE_CANCEL").is_some() {
        let mut stream = backend.stream_with_style(&args[6], &args[7], style).await?;
        anyhow::ensure!(
            matches!(stream.recv().await.transpose()?, Some(AudioChunk::Pcm(_))),
            "no PCM before cancellation"
        );
        drop(stream);
    }
    if std::env::var_os("NOVEL_TTS_PROBE_CANCEL_EARLY").is_some() {
        let stream = backend.stream_with_style(&args[6], &args[7], style).await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(stream);
        println!("{}", serde_json::json!({"cancelled_before_pcm":true}));
    }
    let runs = std::env::var("NOVEL_TTS_PROBE_RUNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    anyhow::ensure!((1..=10).contains(&runs), "probe runs must be 1..10");
    let voices = std::env::var("NOVEL_TTS_PROBE_VOICES").unwrap_or_else(|_| args[7].clone());
    let voices: Vec<_> = voices.split(',').collect();
    for voice in &voices {
        for iteration in 0..runs {
            let output = if voices.len() > 1 {
                PathBuf::from(&args[4]).with_extension(format!("{voice}.run{iteration}.wav"))
            } else if runs == 1 {
                PathBuf::from(&args[4])
            } else {
                PathBuf::from(&args[4]).with_extension(format!("run{iteration}.wav"))
            };
            let start = Instant::now();
            let mut first = None;
            let mut samples = Vec::new();
            for segment in backend.segments(&args[6]).await? {
                let mut stream = backend
                    .stream_with_style(&segment.text, voice, style)
                    .await?;
                let mut eos = false;
                while let Some(chunk) = stream.recv().await {
                    match chunk? {
                        AudioChunk::Pcm(pcm) => {
                            pcm.duration_ms()?;
                            anyhow::ensure!(
                                pcm.channels == 1 && pcm.sample_rate == 24000,
                                "unexpected format"
                            );
                            first.get_or_insert_with(|| start.elapsed().as_secs_f64() * 1000.);
                            samples.extend(pcm.samples);
                        }
                        AudioChunk::End => {
                            eos = true;
                            break;
                        }
                    }
                }
                anyhow::ensure!(eos, "incomplete stream");
            }
            let generate_ms = start.elapsed().as_secs_f64() * 1000.;
            let seconds = samples.len() as f64 / 24000.;
            anyhow::ensure!(seconds > 0., "empty generation");
            let mut wav = hound::WavWriter::create(
                &output,
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 24000,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                },
            )?;
            for sample in samples {
                wav.write_sample(sample)?;
            }
            wav.finalize()?;
            println!(
                "{}",
                serde_json::json!({"backend":args[1],"model":args[2],"device":args[5],"voice":voice,"style":style,"load_ms":load_ms,"first_pcm_ms":first,"generate_ms":generate_ms,"audio_seconds":seconds,"rtf":generate_ms/1000./seconds,"eos":true,"iteration":iteration,"warmup":runs>1 && iteration==0,"output":output})
            );
        }
    }
    if std::env::var_os("NOVEL_TTS_PROBE_DROP_EARLY").is_some() {
        let stream = backend.stream(&args[6], &args[7]).await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let start = Instant::now();
        drop(backend);
        drop(stream);
        println!(
            "{}",
            serde_json::json!({"owner_drop_with_live_stream_ms":start.elapsed().as_millis()})
        );
    }
    Ok(())
}
