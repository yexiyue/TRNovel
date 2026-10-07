//! Experimental real-model verification without playback or Python runtime.
use candle_core::{DType, Device};
use moss_tts::{Model, codec::AudioCodec};
use std::{path::PathBuf, time::Instant};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() >= 5,
        "usage: local_probe MODEL CODEC OUTPUT.wav TEXT [cpu|cuda]"
    );
    let device = match args.get(5).map(String::as_str).unwrap_or("cpu") {
        "cpu" => Device::Cpu,
        #[cfg(feature = "cuda")]
        "cuda" => Device::new_cuda(0)?,
        _ => anyhow::bail!("device is unavailable in this build"),
    };
    let dtype = if device.is_cpu() {
        DType::F32
    } else {
        DType::F16
    };
    let start = Instant::now();
    let mode = std::env::var("MOSS_MODE").unwrap_or_else(|_| "local-1.7b".into());
    let model_dtype = match std::env::var("MOSS_DTYPE").as_deref() {
        Ok("bf16") => DType::BF16,
        Ok("f32") => DType::F32,
        Ok("f16") => DType::F16,
        Err(_) if device.is_cuda() => DType::BF16,
        Err(_) => dtype,
        Ok(other) => anyhow::bail!("unknown model dtype {other}"),
    };
    let mut model = Model::load(&mode, &PathBuf::from(&args[1]), &device, model_dtype)?;
    eprintln!("model loaded {:.3}s", start.elapsed().as_secs_f64());
    let mut codec = if args[2] == "-" {
        None
    } else {
        Some(AudioCodec::load(&PathBuf::from(&args[2]), &device, dtype)?)
    };
    eprintln!("codec loaded {:.3}s", start.elapsed().as_secs_f64());
    let reference = if let Ok(path) = std::env::var("MOSS_REFERENCE") {
        let mut wav = hound::WavReader::open(path)?;
        anyhow::ensure!(
            wav.spec().channels == 1 && wav.spec().sample_rate == 24000,
            "probe reference must be mono 24kHz"
        );
        let samples: Vec<f32> = if wav.spec().sample_format == hound::SampleFormat::Float {
            wav.samples::<f32>().collect::<Result<_, _>>()?
        } else {
            let scale = 2f32.powi(wav.spec().bits_per_sample as i32 - 1);
            wav.samples::<i32>()
                .map(|v| v.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()?
        };
        Some(
            codec
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("reference needs codec"))?
                .encode(&samples, model.codebooks(), &|| false)?,
        )
    } else {
        None
    };
    if let Some(reference) = &reference {
        std::fs::write(
            PathBuf::from(&args[3]).with_extension("reference-tokens.json"),
            serde_json::to_vec(reference)?,
        )?;
    }
    let mut request = moss_tts::Generation::new(&args[4]);
    request.reference = reference.as_deref();
    let instruction = std::env::var("MOSS_INSTRUCTION").ok();
    request.instruction = instruction.as_deref();
    if std::env::var_os("MOSS_CANCEL_PROBE").is_some() {
        let cancelled = Instant::now();
        let result = model.generate(&request, &|| false, |_| Ok(false));
        anyhow::ensure!(result.is_err(), "cancellation incorrectly returned EOS");
        eprintln!(
            "cancel-and-reuse first frame: {:.3}s",
            cancelled.elapsed().as_secs_f64()
        );
    }
    let start = Instant::now();
    let mut all_frames = Vec::new();
    let mut samples = Vec::new();
    let mut pending = Vec::new();
    let mut first = None;
    model.generate(&request, &|| false, |frame| {
        all_frames.push(frame.to_vec());
        pending.push(frame.to_vec());
        if pending.len() == 10 {
            if let Some(codec) = &mut codec {
                samples.extend(codec.decode(&pending, &|| false)?);
            }
            pending.clear();
            if first.is_none() {
                first = Some(start.elapsed().as_secs_f64());
            }
            eprintln!(
                "frames={} elapsed={:.3}",
                all_frames.len(),
                start.elapsed().as_secs_f64()
            );
        }
        Ok(true)
    })?;
    if !pending.is_empty()
        && let Some(codec) = &mut codec
    {
        samples.extend(codec.decode(&pending, &|| false)?);
    }
    std::fs::write(
        PathBuf::from(&args[3]).with_extension("tokens.json"),
        serde_json::to_vec(&all_frames)?,
    )?;
    if codec.is_none() {
        println!(
            "{}",
            serde_json::json!({"mode":mode,"frames":all_frames.len(),"generation_seconds":start.elapsed().as_secs_f64(),"eos":true})
        );
        return Ok(());
    }
    let sample_rate = codec.expect("checked codec").sample_rate;
    anyhow::ensure!(!samples.is_empty(), "empty generated audio");
    let seconds = samples.len() as f64 / sample_rate as f64;
    let elapsed = start.elapsed().as_secs_f64();
    let mut wav = hound::WavWriter::create(
        &args[3],
        hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    for sample in samples {
        wav.write_sample(sample)?;
    }
    wav.finalize()?;
    std::fs::write(
        PathBuf::from(&args[3]).with_extension("tokens.json"),
        serde_json::to_vec(&all_frames)?,
    )?;
    println!(
        "{}",
        serde_json::json!({"frames":all_frames.len(),"audio_seconds":seconds,"generation_seconds":elapsed,"rtf":elapsed/seconds,"first_pcm_seconds":first,"eos":true})
    );
    Ok(())
}
