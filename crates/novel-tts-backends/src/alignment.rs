//! Independent Qwen forced aligner; no TTS model state crosses its owner thread.
mod features;
pub mod resources;
mod timestamps;
use ort::{
    session::{RunOptions, Session},
    value::Tensor,
};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{mpsc, oneshot};
use tts_core::alignment::{Aligner, Alignment, AudioClip, SentenceTiming, SpeechText};

struct Request {
    text: SpeechText,
    audio: AudioClip,
    options: Arc<RunOptions>,
    reply: oneshot::Sender<Result<Vec<SentenceTiming>, String>>,
}
pub struct QwenAligner {
    requests: mpsc::Sender<Request>,
}
impl QwenAligner {
    pub async fn load(directory: PathBuf) -> anyhow::Result<Arc<Self>> {
        Self::load_on(directory, tts_protocol::Device::Cpu).await
    }
    pub async fn load_on(
        directory: PathBuf,
        device: tts_protocol::Device,
    ) -> anyhow::Result<Arc<Self>> {
        Self::load_with_recovery(directory, device, None).await
    }
    pub async fn load_with_recovery(
        directory: PathBuf,
        device: tts_protocol::Device,
        recovery: Option<mpsc::Sender<tts_protocol::Event>>,
    ) -> anyhow::Result<Arc<Self>> {
        crate::devices::validate(device)?;
        let (requests, mut receiver) = mpsc::channel::<Request>(1);
        let (ready, loaded) = oneshot::channel();
        std::thread::Builder::new()
            .name("qwen-alignment".into())
            .spawn(move || {
                let load = (|| -> anyhow::Result<_> {
                    Ok((
                        crate::devices::session(
                            &directory.join(if device == tts_protocol::Device::Cpu {
                                "model_q4.onnx"
                            } else {
                                "model.onnx"
                            }),
                            device,
                            &directory.join("coreml-cache"),
                        )?,
                        tokenizers::Tokenizer::from_file(directory.join("tokenizer.json"))
                            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
                    ))
                })();
                let (mut session, tokenizer) = match load {
                    Ok(value) => {
                        let _ = ready.send(Ok(()));
                        value
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error.to_string()));
                        return;
                    }
                };
                let mut current_device = device;
                while let Some(request) = receiver.blocking_recv() {
                    if request.reply.is_closed() {
                        continue;
                    }
                    let result = run(
                        &mut session,
                        &tokenizer,
                        &request.text,
                        &request.audio,
                        &request.options,
                    )
                    .map_err(|e| e.to_string());
                    if let Err(error) = &result
                        && !request.reply.is_closed()
                        && let Some(progress) = &recovery
                        && current_device != tts_protocol::Device::Cpu
                        && let Ok(cpu) = crate::devices::session(
                            &directory.join("model_q4.onnx"),
                            tts_protocol::Device::Cpu,
                            &directory.join("coreml-cache"),
                        )
                    {
                        session = cpu;
                        current_device = tts_protocol::Device::Cpu;
                        let _ = progress.blocking_send(crate::devices::status(
                            "alignment",
                            tts_protocol::Device::Cpu,
                            Some(format!(
                                "inference failed; using fragment highlight for this block: {error}"
                            )),
                        ));
                    }
                    let _ = request.reply.send(result);
                }
            })?;
        loaded.await?.map_err(|e| anyhow::anyhow!(e))?;
        Ok(Arc::new(Self { requests }))
    }
}
impl Aligner for QwenAligner {
    fn align<'a>(&'a self, text: &'a SpeechText, audio: &'a AudioClip) -> Alignment<'a> {
        Box::pin(async move {
            let (reply, result) = oneshot::channel();
            let options = Arc::new(RunOptions::new().map_err(|e| e.to_string())?);
            let _cancel = CancelRun(options.clone());
            self.requests
                .send(Request {
                    text: text.clone(),
                    audio: audio.clone(),
                    options,
                    reply,
                })
                .await
                .map_err(|_| "alignment thread exited".to_string())?;
            result
                .await
                .map_err(|_| "alignment thread exited".to_string())?
        })
    }
}
fn run(
    session: &mut Session,
    tokenizer: &tokenizers::Tokenizer,
    text: &SpeechText,
    audio: &AudioClip,
    options: &RunOptions,
) -> anyhow::Result<Vec<SentenceTiming>> {
    anyhow::ensure!(!text.units.is_empty(), "no alignable text");
    let mono = features::mono(audio)?;
    let (frames, mel) = features::log_mel(&mono)?;
    let prompt = prompt(text, frames);
    let encoded = tokenizer
        .encode(prompt, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let ids: Vec<i64> = encoded.get_ids().iter().map(|id| *id as i64).collect();
    let length = ids.len();
    let markers: Vec<usize> = ids
        .iter()
        .enumerate()
        .filter_map(|(i, id)| (*id == 151705).then_some(i))
        .collect();
    anyhow::ensure!(
        markers.len() == text.units.len() * 2,
        "timestamp token count mismatch"
    );
    let output = session.run_with_options(
        ort::inputs![
            "input_ids"=>Tensor::from_array(([1,length],ids))?,
            "attention_mask"=>Tensor::from_array(([1,length],vec![1i64;length]))?,
            "input_features"=>Tensor::from_array(([1,128,frames],mel))?,
            "feature_attention_mask"=>Tensor::from_array(([1,frames],vec![1i32;frames]))?,
        ],
        options,
    )?;
    let (shape, logits) = output["logits"].try_extract_tensor::<f32>()?;
    anyhow::ensure!(
        shape.as_ref() == [1, length as i64, 5000],
        "unexpected alignment logits shape"
    );
    let mut timestamps = Vec::new();
    for index in markers {
        let row = &logits[index * 5000..(index + 1) * 5000];
        anyhow::ensure!(
            row.iter().all(|value| value.is_finite()),
            "nonfinite timestamp logits"
        );
        timestamps.push(
            row.iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap()
                .0 as u64
                * 80,
        );
    }
    let timestamps = timestamps::repair(&timestamps);
    let total = audio.frames();
    text.sentences
        .iter()
        .map(|sentence| {
            let start = timestamps[sentence.units.start * 2] * audio.sample_rate as u64 / 1000;
            let end =
                timestamps[(sentence.units.end - 1) * 2 + 1] * audio.sample_rate as u64 / 1000;
            anyhow::ensure!(
                start <= end && end <= total,
                "alignment timestamp exceeds audio"
            );
            Ok(SentenceTiming {
                range: sentence.range,
                start_frame: start,
                end_frame: end,
            })
        })
        .collect()
}

fn prompt(text: &SpeechText, frames: usize) -> String {
    let mut prompt = format!(
        "<|audio_start|>{}<|audio_end|>",
        "<|audio_pad|>".repeat(features::audio_slots(frames))
    );
    for unit in &text.units {
        prompt.push_str(&unit.text);
        prompt.push_str("<timestamp><timestamp>");
    }
    prompt
}

#[cfg(all(test, feature = "moss"))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn native_chain_matches_official_tokens_and_timestamps() {
        let (Some(moss), Some(qwen)) = (
            std::env::var_os("TRNOVEL_MOSS_MODEL_DIR"),
            std::env::var_os("TRNOVEL_QWEN_MODEL_DIR"),
        ) else {
            return;
        };
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/qwen-official.json")).unwrap();
        let text = fixture["text"].as_str().unwrap();
        let speech = SpeechText::from_source(text, 0);
        let tokenizer =
            tokenizers::Tokenizer::from_file(PathBuf::from(&qwen).join("tokenizer.json")).unwrap();
        let ids = tokenizer.encode(prompt(&speech, 360), false).unwrap();
        assert_eq!(serde_json::to_value(ids.get_ids()).unwrap(), fixture["ids"]);
        let backend = crate::moss::MossBackend::load(moss.into()).await.unwrap();
        let mut stream = backend
            .stream_seeded(text, "Weiguo", Some(42))
            .await
            .unwrap();
        let mut blocks = Vec::new();
        while let Some(chunk) = stream.recv().await {
            match chunk.unwrap() {
                tts_core::backend::AudioChunk::Pcm(pcm) => blocks.push(Arc::new(pcm)),
                tts_core::backend::AudioChunk::End => break,
            }
        }
        let audio = AudioClip {
            blocks,
            sample_rate: 48000,
            channels: 2,
            retention: Vec::new(),
        };
        let aligner = QwenAligner::load(qwen.into()).await.unwrap();
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(1),
                aligner.align(&speech, &audio)
            )
            .await
            .is_err()
        );
        let timeline = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            aligner.align(&speech, &audio),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].range, speech.sentences[0].range);
        assert_eq!(timeline[0].start_frame, 0);
        assert_eq!(timeline[0].end_frame, 3280 * 48);
    }
}

/// Cancellation of an async caller also terminates its native ORT run.
struct CancelRun(Arc<RunOptions>);
impl Drop for CancelRun {
    fn drop(&mut self) {
        let _ = self.0.terminate();
    }
}
