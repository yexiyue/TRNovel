//! Opt-in native evaluation, isolated from the reader's device catalogue.
use super::{
    diagnostics::{GenerationEnd, GenerationStats},
    runtime::{Placement, Runtime},
};
use ort::{ep, session::Session};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Instant,
};
use tts_core::backend::{AudioChunk, BackendError};
use windows::Win32::Graphics::{
    Direct3D::D3D_FEATURE_LEVEL_11_0,
    Direct3D12::{D3D12CreateDevice, ID3D12Device},
    Dxgi::{
        CreateDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
        IDXGIFactory1,
    },
};

#[derive(Debug, Serialize, Deserialize)]
struct Adapter {
    index: u32,
    name: String,
    vendor_id: u32,
    device_id: u32,
    luid: String,
    dedicated_bytes: usize,
    shared_bytes: usize,
    software: bool,
    d3d12: bool,
}
fn adapters() -> anyhow::Result<Vec<Adapter>> {
    // SAFETY: DXGI owns the enumerated COM objects; no borrowed native pointers escape.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1()? };
    let mut rows = Vec::new();
    for index in 0.. {
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(error.into()),
        };
        let mut desc = DXGI_ADAPTER_DESC1::default();
        unsafe { adapter.GetDesc1(&mut desc)? };
        let mut device: Option<ID3D12Device> = None;
        let d3d12 =
            unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device) }.is_ok();
        rows.push(Adapter {
            index,
            name: String::from_utf16_lossy(&desc.Description)
                .trim_end_matches('\0')
                .into(),
            vendor_id: desc.VendorId,
            device_id: desc.DeviceId,
            luid: format!(
                "{:08x}:{:08x}",
                desc.AdapterLuid.HighPart, desc.AdapterLuid.LowPart
            ),
            dedicated_bytes: desc.DedicatedVideoMemory,
            shared_bytes: desc.SharedSystemMemory,
            software: desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0,
            d3d12,
        });
    }
    Ok(rows)
}
fn selected(adapters: &[Adapter], index: u32) -> anyhow::Result<&Adapter> {
    let adapter = adapters
        .iter()
        .find(|a| a.index == index)
        .ok_or_else(|| anyhow::anyhow!("DXGI adapter {index} does not exist"))?;
    anyhow::ensure!(
        !adapter.software && adapter.d3d12,
        "adapter {index} is not compatible D3D12 hardware"
    );
    Ok(adapter)
}

enum Work {
    Generate(
        Vec<i32>,
        tokio::sync::mpsc::Sender<Result<AudioChunk, BackendError>>,
        mpsc::Sender<anyhow::Result<GenerationEnd>>,
    ),
    Profiles(mpsc::Sender<anyhow::Result<Vec<String>>>),
}
type Completion = mpsc::Receiver<anyhow::Result<GenerationEnd>>;
struct Engine {
    requests: Option<mpsc::Sender<Work>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Engine {
    fn load(
        directory: PathBuf,
        mode: &str,
        adapter: Option<u32>,
        profile: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let (requests, jobs) = mpsc::channel();
        let (ready, loaded) = mpsc::channel();
        let mode = mode.to_owned();
        let thread = thread::Builder::new()
            .name("moss-directml-probe".into())
            .spawn(move || {
                let placement = if mode == "dml-cache" {
                    Placement::DirectML
                } else {
                    Placement::Host
                };
                let result = Runtime::load_sessions(&directory, placement, |path| {
                    let mut builder = Session::builder()?
                        .with_intra_threads(4)
                        .map_err(|e| anyhow::anyhow!("{e}"))?
                        .with_parallel_execution(false)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    if mode == "cpu" {
                        builder = builder
                            .with_execution_providers([ep::CPU::default()
                                .build()
                                .error_on_failure()])
                            .map_err(|e| anyhow::anyhow!("{e}"))?;
                    } else {
                        builder = builder
                            .with_memory_pattern(false)
                            .map_err(|e| anyhow::anyhow!("{e}"))?
                            .with_execution_providers([ep::DirectML::default()
                                .with_device_id(adapter.expect("validated adapter") as i32)
                                .build()
                                .error_on_failure()])
                            .map_err(|e| anyhow::anyhow!("{e}"))?;
                    }
                    if let Some(profile) = &profile {
                        builder = builder
                            .with_profiling(
                                profile.join(path.file_stem().expect("model file name")),
                            )
                            .map_err(|e| anyhow::anyhow!("{e}"))?;
                    }
                    builder
                        .commit_from_file(path)
                        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
                });
                let mut runtime = match result {
                    Ok(runtime) => {
                        if ready.send(Ok(())).is_err() {
                            return;
                        }
                        runtime
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                while let Ok(job) = jobs.recv() {
                    match job {
                        Work::Generate(tokens, tx, reply) => {
                            let result = if tx.is_closed() {
                                Ok(GenerationEnd::Cancelled)
                            } else {
                                runtime.generate(
                                    tokens,
                                    "Weiguo",
                                    Some(42),
                                    &tx,
                                    &mut GenerationStats::default(),
                                )
                            };
                            let _ = reply.send(result);
                        }
                        Work::Profiles(reply) => {
                            let _ = reply.send(runtime.finish_profiles());
                        }
                    }
                }
            })?;
        // Own the initialization thread before waiting for its result.
        let engine = Self {
            requests: Some(requests),
            thread: Some(thread),
        };
        loaded.recv()??;
        Ok(engine)
    }
    fn generate(
        &self,
        tokens: Vec<i32>,
    ) -> anyhow::Result<(tts_core::backend::AudioStream, Completion)> {
        let (audio, rx) = tokio::sync::mpsc::channel(1);
        let (reply, result) = mpsc::channel();
        self.requests
            .as_ref()
            .expect("loaded engine")
            .send(Work::Generate(tokens, audio, reply))?;
        Ok((rx, result))
    }
    fn profiles(&self) -> anyhow::Result<Vec<String>> {
        let (reply, result) = mpsc::channel();
        self.requests
            .as_ref()
            .expect("loaded engine")
            .send(Work::Profiles(reply))?;
        result.recv()?
    }
}
fn synthesize(
    engine: &Engine,
    tokens: Vec<i32>,
    output: &Path,
) -> anyhow::Result<serde_json::Value> {
    let began = Instant::now();
    let (mut audio, report) = engine.generate(tokens)?;
    let mut writer = None;
    let mut format = None;
    let mut samples = 0usize;
    let mut first = None;
    let mut nonzero = false;
    let mut ended = false;
    while let Some(chunk) = audio.blocking_recv() {
        match chunk? {
            AudioChunk::Pcm(pcm) => {
                pcm.duration_ms()?;
                let current = (pcm.sample_rate, pcm.channels);
                anyhow::ensure!(format.is_none_or(|f| f == current), "PCM format changed");
                format = Some(current);
                first.get_or_insert(began.elapsed().as_secs_f64() * 1000.0);
                if writer.is_none() {
                    writer = Some(hound::WavWriter::create(
                        output,
                        hound::WavSpec {
                            channels: pcm.channels,
                            sample_rate: pcm.sample_rate,
                            bits_per_sample: 32,
                            sample_format: hound::SampleFormat::Float,
                        },
                    )?);
                }
                samples += pcm.samples.len();
                for sample in pcm.samples {
                    nonzero |= sample != 0.0;
                    writer.as_mut().unwrap().write_sample(sample)?;
                }
            }
            AudioChunk::End => {
                ended = true;
                break;
            }
        }
    }
    drop(audio);
    let end = report.recv()??;
    if let Some(writer) = writer {
        writer.finalize()?;
    }
    anyhow::ensure!(
        ended && end == GenerationEnd::Eos && samples > 0 && nonzero,
        "invalid/unfinished audio: {end:?}"
    );
    let elapsed = began.elapsed().as_secs_f64();
    let (rate, channels) = format.unwrap();
    let seconds = samples as f64 / f64::from(rate) / f64::from(channels);
    Ok(
        serde_json::json!({"first_pcm_ms":first,"generate_ms":elapsed*1000.0,"audio_seconds":seconds,"rtf":elapsed/seconds,"end":end}),
    )
}

/// Run the standalone command. All output is development evidence, not a device qualification.
pub fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "list" {
        println!("{}", serde_json::to_string_pretty(&adapters()?)?);
        return Ok(());
    }
    anyhow::ensure!(
        args.len() >= 4,
        "usage: moss_directml_probe list | MODEL_DIR TEXT_FILE OUTPUT_DIR cpu|dml-host|dml-cache [DXGI_INDEX] [ROUNDS=5] [profile]"
    );
    let mode = &args[3];
    anyhow::ensure!(
        ["cpu", "dml-host", "dml-cache"].contains(&mode.as_str()),
        "invalid mode"
    );
    let index = args.get(4).map(|s| s.parse::<u32>()).transpose()?;
    let rounds = args
        .get(5)
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(5);
    anyhow::ensure!((1..=10).contains(&rounds), "rounds must be 1..10");
    let output = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&output)?;
    let adapters = adapters()?;
    std::fs::write(
        output.join("adapters.json"),
        serde_json::to_vec_pretty(&adapters)?,
    )?;
    let adapter = if mode == "cpu" {
        None
    } else {
        Some(selected(
            &adapters,
            index.ok_or_else(|| anyhow::anyhow!("explicit DXGI index required"))?,
        )?)
    };
    let mut summary = serde_json::json!({"mode":mode,"adapter":adapter,"ort":format!("{:?}",ort::info()),"seed":42,"voice":"Weiguo","rounds":[],"profiled":args.get(6).is_some_and(|s|s=="profile")});
    let result = evaluate(&args, rounds, &mut summary);
    if let Err(error) = &result {
        summary["error"] = error.to_string().into();
    }
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    result
}
fn evaluate(args: &[String], rounds: usize, summary: &mut serde_json::Value) -> anyhow::Result<()> {
    let output = Path::new(&args[2]);
    let text = std::fs::read_to_string(&args[1])?;
    let tokenizer = sentencepiece_rs::SentencePieceProcessor::open(
        Path::new(&args[0]).join("tts/tokenizer.model"),
    )?;
    let segments: Vec<_> = text
        .lines()
        .map(super::text::normalize)
        .filter(|s| !s.is_empty())
        .map(|s| {
            let tokens = tokenizer
                .encode_to_ids(&s)?
                .into_iter()
                .map(|i| i as i32)
                .collect::<Vec<_>>();
            Ok((s, tokens))
        })
        .collect::<anyhow::Result<_>>()?;
    anyhow::ensure!(!segments.is_empty(), "empty corpus");
    let profile = summary["profiled"]
        .as_bool()
        .unwrap()
        .then(|| output.join("profiles"));
    if let Some(path) = &profile {
        std::fs::create_dir_all(path)?;
    }
    let began = Instant::now();
    let engine = Engine::load(
        PathBuf::from(&args[0]),
        &args[3],
        args.get(4).map(|s| s.parse()).transpose()?,
        profile.clone(),
    )?;
    summary["cold_load_ms"] = (began.elapsed().as_secs_f64() * 1000.0).into();
    for round in 0..=rounds {
        let directory = output.join(format!("round-{round}"));
        std::fs::create_dir_all(&directory)?;
        let mut rows = Vec::new();
        for (index, (text, tokens)) in segments.iter().enumerate() {
            let path = directory.join(format!("{index:03}.wav"));
            let mut row = synthesize(&engine, tokens.clone(), &path)?;
            row["text"] = text.clone().into();
            row["file"] = path.to_string_lossy().into_owned().into();
            rows.push(row);
        }
        let ms: f64 = rows
            .iter()
            .map(|r| r["generate_ms"].as_f64().unwrap())
            .sum();
        let seconds: f64 = rows
            .iter()
            .map(|r| r["audio_seconds"].as_f64().unwrap())
            .sum();
        let row = serde_json::json!({"round":round,"warmup":round==0,"rtf":ms/1000.0/seconds,"generate_ms":ms,"audio_seconds":seconds,"segments":rows});
        println!("round {round}: RTF {:.4}", ms / 1000.0 / seconds);
        summary["rounds"].as_array_mut().unwrap().push(row);
        std::fs::write(
            output.join("summary.json"),
            serde_json::to_vec_pretty(summary)?,
        )?;
    }
    // Cancel after the first delivered PCM, then require a fresh request to reach EOS.
    let (mut audio, report) = engine.generate(segments[0].1.clone())?;
    anyhow::ensure!(
        matches!(audio.blocking_recv().transpose()?, Some(AudioChunk::Pcm(_))),
        "no PCM before cancellation"
    );
    let began = Instant::now();
    drop(audio);
    anyhow::ensure!(
        report.recv()?? == GenerationEnd::Cancelled,
        "request did not cancel"
    );
    summary["cancel_ms"] = (began.elapsed().as_secs_f64() * 1000.0).into();
    summary["after_cancel"] = synthesize(
        &engine,
        segments[0].1.clone(),
        &output.join("after-cancel.wav"),
    )?;
    if profile.is_some() {
        summary["profiles"] = serde_json::to_value(engine.profiles()?)?;
    }
    let began = Instant::now();
    drop(engine);
    summary["release_ms"] = (began.elapsed().as_secs_f64() * 1000.0).into();
    summary["regressions"] = "passed".into();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_requires_exact_compatible_adapter() {
        let hardware = Adapter {
            index: 3,
            name: "AMD discrete".into(),
            vendor_id: 0x1002,
            device_id: 0,
            luid: "identity".into(),
            dedicated_bytes: 8 << 30,
            shared_bytes: 0,
            software: false,
            d3d12: true,
        };
        let mut rows = vec![hardware];
        assert_eq!(selected(&rows, 3).unwrap().name, "AMD discrete");
        assert!(selected(&rows, 0).is_err());
        rows[0].software = true;
        assert!(selected(&rows, 3).is_err());
        rows[0].software = false;
        rows[0].d3d12 = false;
        assert!(selected(&rows, 3).is_err());
    }
}
