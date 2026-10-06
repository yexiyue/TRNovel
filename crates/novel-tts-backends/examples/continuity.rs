//! Export a reproducible same-voice comparison without an audio output device.
use novel_tts_backends::moss::MossBackend;
use std::{path::PathBuf, time::Instant};
use tts_core::{
    audio::BoundarySilence,
    backend::{AudioChunk, Backend, Pcm},
};
fn write(
    writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    pcm: Pcm,
) -> anyhow::Result<()> {
    for sample in pcm.samples {
        writer.write_sample(sample)?;
    }
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let model = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("model directory required"))?;
    let output = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("output directory required"))?,
    );
    let text=args.next().unwrap_or_else(||"=====\n夜色落在山间，星河缓缓流动。林舟打开远方寄来的信，看见一行熟悉的字。风吹过窗前的树叶，路灯照亮小巷，他决定明天清晨出发。".into());
    std::fs::create_dir_all(&output)?;
    let backend = MossBackend::load(model.into()).await?;
    for continuous in [false, true] {
        backend.select_voice("Weiguo");
        let start = Instant::now();
        let mut first = None;
        let mut blocks = 0;
        let mut source = text.as_str();
        let name = if continuous {
            "continuous.wav"
        } else {
            "sentences.wav"
        };
        let mut wav = hound::WavWriter::create(
            output.join(name),
            hound::WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )?;
        while !source.is_empty() {
            let Some(segment) = (if continuous {
                backend.next_segment(source).await?
            } else {
                let mut offset = 0;
                for line in source.split_inclusive('\n') {
                    if line.trim().is_empty() || tts_core::text::is_decoration_line(line) {
                        offset += line.len();
                    } else {
                        break;
                    }
                }
                let end = source[offset..]
                    .char_indices()
                    .find(|(_, c)| "。！？!?\n".contains(*c))
                    .map_or(source.len(), |(i, c)| offset + i + c.len_utf8());
                (offset < end).then(|| tts_core::text::TextSegment {
                    text: source[offset..end].into(),
                    start: offset,
                    end,
                })
            }) else {
                break;
            };
            let paragraph = backend.paragraph_end(&segment.text, &source[segment.end..]);
            let mut silence = BoundarySilence::default();
            let mut stream = backend
                .stream_seeded(&segment.text, "Weiguo", Some(42))
                .await?;
            let mut ended = false;
            let mut seconds = 0.0;
            while let Some(chunk) = stream.recv().await {
                match chunk? {
                    AudioChunk::Pcm(pcm) => {
                        seconds += pcm.duration_ms()? as f64 / 1000.0;
                        first.get_or_insert(start.elapsed());
                        if continuous {
                            if let Some(pcm) = silence.push(pcm)? {
                                write(&mut wav, pcm)?;
                            }
                        } else {
                            write(&mut wav, pcm)?;
                        }
                    }
                    AudioChunk::End => {
                        ended = true;
                        if continuous && let Some(pcm) = silence.finish(paragraph)? {
                            write(&mut wav, pcm)?;
                        }
                        break;
                    }
                }
            }
            anyhow::ensure!(ended, "incomplete synthesis");
            backend.observe_duration(&segment.text, "Weiguo", seconds);
            source = &source[segment.end..];
            blocks += 1;
        }
        wav.finalize()?;
        eprintln!(
            "{name}: blocks {blocks}, first {first:?}, total {:?}",
            start.elapsed()
        );
    }
    Ok(())
}
