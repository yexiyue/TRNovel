//! Exercise the C ABI before making this model available in the worker.
use std::{path::PathBuf, time::Instant};
use voxcpm_sys::{Device, Model, Outcome, Reference};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let directory = PathBuf::from(args.next().ok_or("model directory required")?);
    let device = match args.next().as_deref().unwrap_or("cpu") {
        "cpu" => Device::Cpu,
        "cuda" => Device::Cuda,
        "metal" => Device::Metal,
        _ => return Err("unknown device".into()),
    };
    let output = args.next().ok_or("output WAV required")?;
    let reference = args
        .next()
        .map(|path| {
            let mut wav = hound::WavReader::open(path)?;
            let rate = wav.spec().sample_rate;
            if wav.spec().channels != 1 || wav.spec().sample_format != hound::SampleFormat::Float {
                return Err("probe reference must be a mono floating-point WAV".into());
            }
            let samples = wav.samples::<f32>().collect::<Result<Vec<_>, _>>()?;
            Ok::<_, Box<dyn std::error::Error>>((samples, rate))
        })
        .transpose()?;
    let started = Instant::now();
    let mut model = Model::load(
        &directory.join("VoxCPM2-BaseLM-Q8_0.gguf"),
        &directory.join("VoxCPM2-Acoustic-F16.gguf"),
        device,
    )?;
    let load_ms = started.elapsed().as_millis();
    let text = "你好，欢迎收听。山风吹过松林，他转身说道：我们明天再见。";
    let started = Instant::now();
    let mut cancellation_requested = None;
    let result = model.generate(text, None, 200, |_| {
        cancellation_requested = Some(Instant::now());
        false
    })?;
    assert_eq!(result, Outcome::Cancelled);
    let cancel_ms = cancellation_requested
        .ok_or("cancel probe received no PCM")?
        .elapsed()
        .as_millis();
    let cancel_first_pcm_ms = started.elapsed().as_millis();
    let mut samples = Vec::new();
    let mut first_ms = None;
    let started = Instant::now();
    let encoded = reference
        .as_ref()
        .map(|(samples, rate)| model.encode_reference(samples, *rate))
        .transpose()?;
    let reference = encoded.as_ref().map(|samples| Reference {
        samples,
        sample_rate: 0,
        transcript: text,
        encoded: true,
    });
    let result = model.generate(text, reference, 200, |pcm| {
        if !pcm.is_empty() {
            first_ms.get_or_insert_with(|| started.elapsed().as_millis());
            samples.extend_from_slice(pcm);
        }
        true
    })?;
    let total_ms = started.elapsed().as_millis();
    assert_eq!(result, Outcome::Complete);
    assert!(!samples.is_empty() && samples.iter().all(|s| s.is_finite()));
    let mut wav = hound::WavWriter::create(
        output,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    for sample in &samples {
        wav.write_sample(*sample)?;
    }
    wav.finalize()?;
    assert_eq!(model.generate(text, None, 1, |_| true)?, Outcome::Truncated);
    println!(
        "load_ms={load_ms} first_pcm_ms={first_ms:?} total_ms={total_ms} audio_seconds={} cancel_ms={cancel_ms} cancel_first_pcm_ms={cancel_first_pcm_ms} eos=true truncation_detected=true",
        samples.len() as f64 / 48000.0
    );
    Ok(())
}
