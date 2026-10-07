//! Preserved-engine hot benchmark for the Candle migration; no product settings.
use std::{path::Path, time::Instant};
use voxcpm_sys::{Device, Model, Outcome, Reference};

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 7,
        "MODEL_DIRECTORY cpu|cuda OUTPUT_DIRECTORY TEXT_FILE REFERENCE.wav TRANSCRIPT_FILE"
    );
    let directory = Path::new(&args[1]);
    let device = match args[2].as_str() {
        "cpu" => Device::Cpu,
        "cuda" => Device::Cuda,
        _ => anyhow::bail!("unsupported benchmark device"),
    };
    let started = Instant::now();
    let mut model = Model::load(
        &directory.join("VoxCPM2-BaseLM-Q8_0.gguf"),
        &directory.join("VoxCPM2-Acoustic-F16.gguf"),
        device,
    )
    .map_err(anyhow::Error::msg)?;
    let load = started.elapsed();
    let started = Instant::now();
    let samples = novel_tts_backends::reference::load(Path::new(&args[5]), 16000)?;
    let transcript = std::fs::read_to_string(&args[6])?;
    let encoded = model
        .encode_reference(&samples, 16000)
        .map_err(anyhow::Error::msg)?;
    let reference_ms = started.elapsed().as_millis();
    let text = std::fs::read_to_string(&args[4])?;
    let segments = tts_core::text::preprocess_text(&text, 180);
    let output = Path::new(&args[3]);
    std::fs::create_dir_all(output)?;
    let mut results = Vec::new();
    for round in 0..=5 {
        let directory = output.join(format!("round-{round}"));
        std::fs::create_dir_all(&directory)?;
        let started = Instant::now();
        let mut first = None;
        let mut count = 0;
        let mut index = Vec::new();
        for (i, segment) in segments.iter().enumerate() {
            let mut pcm = Vec::new();
            let outcome = model
                .generate(
                    &segment.text,
                    Some(Reference {
                        samples: &encoded,
                        sample_rate: 16000,
                        transcript: &transcript,
                        encoded: true,
                    }),
                    200,
                    |samples| {
                        first.get_or_insert(started.elapsed());
                        pcm.extend_from_slice(samples);
                        true
                    },
                )
                .map_err(anyhow::Error::msg)?;
            anyhow::ensure!(
                outcome == Outcome::Complete
                    && !pcm.is_empty()
                    && pcm.iter().all(|v| v.is_finite()),
                "invalid native generation: {outcome:?}"
            );
            count += pcm.len();
            let filename = format!("{:03}.wav", i + 1);
            let mut writer = hound::WavWriter::create(
                directory.join(&filename),
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 48000,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                },
            )?;
            for sample in pcm {
                writer.write_sample(sample)?;
            }
            writer.finalize()?;
            index.push(serde_json::json!({"file":filename,"text":segment.text}));
        }
        let elapsed = started.elapsed();
        let audio_seconds = count as f64 / 48000.0;
        let result = serde_json::json!({"round":round,"warmup":round==0,"first_pcm_ms":first.map(|t|t.as_millis()),"generate_ms":elapsed.as_millis(),"audio_seconds":audio_seconds,"rtf":elapsed.as_secs_f64()/audio_seconds,"segments":segments.len()});
        println!("{result}");
        results.push(result);
        std::fs::write(
            directory.join("index.json"),
            serde_json::to_vec_pretty(&index)?,
        )?;
    }
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"load_ms":load.as_millis(),"reference_ms":reference_ms,"rounds":results}),
        )?,
    )?;
    Ok(())
}
