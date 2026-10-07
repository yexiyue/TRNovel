//! Standalone Candle migration probe; never changes production engine selection.
use candle_core::{Device, Result};
use std::{path::Path, time::Instant};
use voxcpm::{Model, Options, SAMPLE_RATE};
fn main() -> anyhow::Result<()> {
    let a: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        a.len() >= 5,
        "probe MODEL_DIRECTORY cpu|cuda OUTPUT.wav TEXT [REFERENCE.wav TRANSCRIPT]"
    );
    let device = match a[2].as_str() {
        "cpu" => Device::Cpu,
        "cuda" => Device::new_cuda(0)?,
        "metal" => Device::new_metal(0)?,
        _ => anyhow::bail!("unknown device"),
    };
    let now = Instant::now();
    let mut model = Model::load(Path::new(&a[1]), &device)?;
    device.synchronize()?;
    let load = now.elapsed();
    let ids = model.tokenizer.encode(&a[4])?;
    eprintln!("token IDs: {ids:?}");
    let encode = Instant::now();
    let reference = if a.len() >= 7 {
        let mut wav = hound::WavReader::open(&a[5])?;
        anyhow::ensure!(
            wav.spec().channels == 1 && wav.spec().sample_rate == 16000,
            "reference must be mono 16kHz"
        );
        let samples = if wav.spec().sample_format == hound::SampleFormat::Float {
            wav.samples::<f32>()
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            wav.samples::<i16>()
                .map(|s| s.map(|v| v as f32 / 32768.0))
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        Some(model.reference(&samples, Some(a[6].clone()), &|| false)?)
    } else {
        None
    };
    device.synchronize()?;
    let encoding = encode.elapsed();
    let mut writer = hound::WavWriter::create(
        &a[3],
        hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    let start = Instant::now();
    let mut first = None;
    let mut count = 0;
    let outcome = model.generate(
        &a[4],
        reference.as_ref(),
        &Options::default(),
        &|| false,
        |pcm| -> Result<()> {
            first.get_or_insert(start.elapsed());
            count += pcm.len();
            for s in pcm {
                writer
                    .write_sample(*s)
                    .map_err(|e| candle_core::Error::Msg(e.to_string()))?;
            }
            Ok(())
        },
    )?;
    device.synchronize()?;
    let elapsed = start.elapsed();
    writer.finalize()?;
    println!(
        "{}",
        serde_json::json!({"load_ms":load.as_millis(),"reference_ms":encoding.as_millis(),"first_pcm_ms":first.map(|v|v.as_millis()),"generate_ms":elapsed.as_millis(),"audio_seconds":count as f64/SAMPLE_RATE as f64,"rtf":elapsed.as_secs_f64()/(count as f64/SAMPLE_RATE as f64),"outcome":format!("{outcome:?}")})
    );
    Ok(())
}
