//! Record the existing Kokoro behavior before changing the listening core.
//! Run with an output directory containing the pinned model and voices files.
use kokoro_tts::{KokoroTts, Voice, g2p, get_token_ids};
use novel_tts_core as tts_core;
use std::{fs::File, io::Write, path::PathBuf};
use tts_core::{
    backend::{Backend, KokoroBackend},
    text::preprocess_text,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected baseline directory")?,
    );
    let tts = KokoroTts::new(
        directory.join("kokoro-v1.1-zh.onnx"),
        directory.join("voices-v1.1-zh.bin"),
    )
    .await?;
    let adapter = KokoroBackend::load(
        &directory.join("kokoro-v1.1-zh.onnx"),
        &directory.join("voices-v1.1-zh.bin"),
    )
    .await?;
    let text = include_str!("../../novel-tts-protocol/tests/fixtures/narration.txt");
    let mut report = File::create(directory.join("segments.tsv"))?;
    writeln!(
        report,
        "kokoro-tts=0.3.1 ort=2.0.0-rc.10 voice=Zf001 model_speed=1 playback_speed=1 playback_volume=1"
    )?;
    writeln!(
        report,
        "index\tstart\tend\tsamples\tsynthesis_ms\ttext\tphonemes\ttokens"
    )?;
    for (index, segment) in preprocess_text(text, 200).iter().enumerate() {
        let phonemes = g2p(&segment.text, true)?;
        let tokens = get_token_ids(&phonemes, true);
        let (audio, elapsed) = tts.synth(&segment.text, Voice::Zf001(1)).await?;
        let migrated = adapter.synthesize(&segment.text, "Zf001").await?;
        assert_eq!(
            audio.len(),
            migrated.samples.len(),
            "sample count changed in segment {index}"
        );
        let difference = audio
            .iter()
            .zip(&migrated.samples)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(
            difference <= 0.0001,
            "PCM changed in segment {index}: {difference}"
        );
        eprintln!(
            "segment {index}: {} samples, adapter max PCM difference {difference}",
            audio.len()
        );
        writeln!(
            report,
            "{index}\t{}\t{}\t{}\t{}\t{}\t{}\t{:?}",
            segment.start,
            segment.end,
            audio.len(),
            elapsed.as_millis(),
            segment.text,
            phonemes,
            tokens
        )?;
        write_wav(
            &mut File::create(directory.join(format!("segment-{index}.wav")))?,
            &audio,
        )?;
    }
    eprintln!(
        "Baseline written to {} (24 kHz mono float PCM)",
        directory.display()
    );
    Ok(())
}

fn write_wav(writer: &mut impl Write, samples: &[f32]) -> std::io::Result<()> {
    let length = u32::try_from(samples.len() * 4).map_err(std::io::Error::other)?;
    writer.write_all(b"RIFF")?;
    writer.write_all(&(36 + length).to_le_bytes())?;
    writer.write_all(b"WAVEfmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&3u16.to_le_bytes())?; // IEEE float PCM.
    writer.write_all(&1u16.to_le_bytes())?;
    writer.write_all(&24000u32.to_le_bytes())?;
    writer.write_all(&96000u32.to_le_bytes())?;
    writer.write_all(&4u16.to_le_bytes())?;
    writer.write_all(&32u16.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&length.to_le_bytes())?;
    for sample in samples {
        writer.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}
