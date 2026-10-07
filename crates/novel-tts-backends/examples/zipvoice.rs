use novel_tts_backends::zipvoice::{frontend::Frontend, model::Model};
use std::{path::PathBuf, time::Instant};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let directory = PathBuf::from(&args[1]);
    if args[2] == "frontend" {
        let frontend = Frontend::load(&directory)?;
        for text in args.iter().skip(3) {
            println!(
                "{}",
                serde_json::json!({"text":text,"phones":frontend.phonemes(text)?})
            );
        }
        return Ok(());
    }
    let reference = novel_tts_backends::reference::load(std::path::Path::new(&args[3]), 24000)?;
    let started = Instant::now();
    let mut model = Model::load(&directory)?;
    let loaded = started.elapsed().as_millis();
    let started = Instant::now();
    let samples = model
        .generate(&args[5], &reference, &args[4], || false)?
        .expect("not cancelled");
    let elapsed = started.elapsed().as_millis();
    let mut wav = hound::WavWriter::create(
        &args[2],
        hound::WavSpec {
            channels: 1,
            sample_rate: 24000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    for sample in &samples {
        wav.write_sample(*sample)?;
    }
    wav.finalize()?;
    println!(
        "{}",
        serde_json::json!({"load_ms":loaded,"generate_ms":elapsed,"audio_seconds":samples.len() as f64 / 24000.0})
    );
    let cancelled = std::cell::Cell::new(0);
    let result = model.generate(&args[5], &reference, &args[4], || {
        cancelled.set(cancelled.get() + 1);
        cancelled.get() > 3
    })?;
    anyhow::ensure!(result.is_none(), "expected iteration cancellation");
    anyhow::ensure!(
        model
            .generate("你好，取消后再试一次。", &reference, &args[4], || false)?
            .is_some(),
        "generation after cancellation failed"
    );
    println!("cancelled within flow iterations; subsequent request succeeded");
    Ok(())
}
