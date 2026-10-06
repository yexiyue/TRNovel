//! Fixed-seed offline synthesis evidence. Output stays in a user-selected directory.
use novel_tts_backends::moss::MossBackend;
use std::{path::PathBuf, time::Instant};
use tts_core::{
    audio::BoundarySilence,
    backend::{AudioChunk, Backend},
    text::TextSegment,
};
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let model = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("model directory required"))?,
    );
    let text = std::fs::read_to_string(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("input file required"))?,
    )?;
    let output = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("output directory required"))?,
    );
    let mode = args.next().unwrap_or_else(|| "context".into());
    anyhow::ensure!(
        ["context", "unsegmented", "lines"].contains(&mode.as_str()),
        "mode must be context, unsegmented or lines"
    );
    std::fs::create_dir_all(&output)?;
    let loaded = Instant::now();
    let backend = MossBackend::load(model).await?;
    let load_ms = loaded.elapsed().as_secs_f64() * 1000.0;
    backend.select_voice("Weiguo");
    let began = Instant::now();
    let mut offset = 0;
    let mut rows = Vec::new();
    while offset < text.len() {
        let piece = if mode == "lines" {
            let line = text[offset..].split_inclusive('\n').next().unwrap_or("");
            if line.trim().is_empty() || tts_core::text::is_decoration_line(line) {
                offset += line.len();
                continue;
            }
            Some(TextSegment {
                text: line.into(),
                start: 0,
                end: line.len(),
            })
        } else if mode == "unsegmented" {
            Some(TextSegment {
                text: text[offset..].into(),
                start: 0,
                end: text.len() - offset,
            })
        } else {
            backend.next_segment(&text[offset..]).await?
        };
        let Some(piece) = piece else {
            break;
        };
        let start = offset + piece.start;
        let end = offset + piece.end;
        let segment_start = Instant::now();
        let (mut audio, report) = backend
            .stream_diagnosed(&piece.text, "Weiguo", Some(42))
            .await?;
        let mut wav = hound::WavWriter::create(
            output.join(format!("block-{:03}.wav", rows.len())),
            hound::WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )?;
        let mut boundary = BoundarySilence::default();
        let mut first_ms = None;
        let mut completed = false;
        let mut failure = None;
        while let Some(chunk) = audio.recv().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    failure = Some(error.to_string());
                    break;
                }
            };
            let processed = match chunk {
                AudioChunk::Pcm(pcm) => boundary.push(pcm)?,
                AudioChunk::End => {
                    completed = true;
                    boundary.finish(backend.paragraph_end(&piece.text, &text[end..]))?
                }
            };
            if let Some(pcm) = processed {
                first_ms.get_or_insert(segment_start.elapsed().as_secs_f64() * 1000.0);
                for sample in pcm.samples {
                    wav.write_sample(sample)?;
                }
            }
            if completed {
                break;
            }
        }
        wav.finalize()?;
        let report = report.await?;
        backend.observe_duration(&piece.text, "Weiguo", report.audio_seconds);
        rows.push(serde_json::json!({"range":{"start":start,"end":end}, "source":&text[start..end],
            "generation":report,"first_processed_pcm_ms":first_ms,"total_ms":segment_start.elapsed().as_secs_f64()*1000.0,
            "synthesis_completed":completed,"failure":failure}));
        std::fs::write(
            output.join("report.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
            "seed":42,"voice":"Weiguo","mode":mode,"load_ms":load_ms,"total_ms":began.elapsed().as_secs_f64()*1000.0,"blocks":rows}))?,
        )?;
        anyhow::ensure!(completed, "generation failed; see report.json");
        offset = end;
    }
    eprintln!("{} blocks; evidence {}", rows.len(), output.display());
    Ok(())
}
