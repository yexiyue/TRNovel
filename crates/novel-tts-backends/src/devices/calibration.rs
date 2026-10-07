//! Reproducible full-pipeline calibration; model construction stays outside timing.
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tts_core::{
    alignment::{Aligner, AudioClip, SpeechText},
    backend::{AudioChunk, Backend},
};
use tts_protocol::Device;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Measurements {
    pub total_ms: f64,
    pub first_ms: f64,
}
impl Measurements {
    pub fn improves(self, baseline: Self) -> bool {
        self.total_ms <= baseline.total_ms * 0.85 && self.first_ms <= baseline.first_ms * 1.10
    }
}
#[derive(Serialize, Deserialize)]
pub struct Record {
    pub key: String,
    pub device: Device,
    pub reason: String,
}
pub fn key(component: &str, revision: &str) -> String {
    let hardware = if cfg!(target_os = "macos") {
        std::process::Command::new("sysctl")
            .args([
                "-n",
                "hw.model",
                "hw.cpufamily",
                "hw.memsize",
                "hw.physicalcpu",
                "hw.logicalcpu",
            ])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "Get-CimInstance Win32_Processor | Select-Object Name,ProcessorId,NumberOfCores; Get-CimInstance Win32_ComputerSystem | Select-Object Model,TotalPhysicalMemory"])
            .output().ok().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
    } else {
        // Clock frequency in /proc/cpuinfo changes continuously; it is not identity.
        let cpu = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        cpu.lines()
            .filter(|line| {
                [
                    "vendor_id",
                    "model name",
                    "cpu family",
                    "model\t",
                    "stepping",
                    "CPU implementer",
                    "CPU architecture",
                    "CPU variant",
                    "CPU part",
                    "CPU revision",
                    "Hardware",
                    "Revision",
                ]
                .iter()
                .any(|key| line.starts_with(key))
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let gpu = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=uuid,name,driver_version",
            "--format=csv,noheader",
        ])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    format!(
        "{}:{}:{}:{}:{}:{}:{:?}:{}",
        component,
        revision,
        std::env::consts::OS,
        std::env::consts::ARCH,
        hardware,
        runtime_info(),
        super::available(),
        gpu
    )
}
fn runtime_info() -> String {
    let versions: Vec<String> = vec![
        #[cfg(any(
            feature = "moss",
            feature = "alignment",
            feature = "kokoro",
            feature = "coreml",
            feature = "ort-cuda",
            feature = "zipvoice"
        ))]
        format!("{:?}", ort::info()),
        #[cfg(feature = "qwen")]
        "candle-0.9.2:qwen-local-v1".into(),
    ];
    versions.join(";")
}
fn path(root: &Path, component: &str, key: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    root.join(format!(
        "calibration-{component}-{:x}.json",
        Sha256::digest(key.as_bytes())
    ))
}
pub fn cached(root: &Path, component: &str, key: &str) -> Option<Record> {
    cached_for(root, component, key, &super::available())
}
pub fn cached_for(root: &Path, component: &str, key: &str, available: &[Device]) -> Option<Record> {
    let record: Record =
        serde_json::from_slice(&std::fs::read(path(root, component, key)).ok()?).ok()?;
    (record.key == key && available.contains(&record.device)).then_some(record)
}
pub fn save(root: &Path, component: &str, record: &Record) -> anyhow::Result<()> {
    std::fs::create_dir_all(root)?;
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer(&mut temporary, record)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path(root, component, &record.key))?;
    Ok(())
}
fn mean(values: &[Measurements]) -> Measurements {
    Measurements {
        total_ms: values.iter().map(|v| v.total_ms).sum::<f64>() / values.len() as f64,
        first_ms: values.iter().map(|v| v.first_ms).sum::<f64>() / values.len() as f64,
    }
}
pub async fn synthesis(backend: &dyn Backend, voice: &str) -> anyhow::Result<Measurements> {
    let mut measurements = Vec::new();
    for i in 0..8 {
        let start = Instant::now();
        let mut stream = backend
            .stream("你好，欢迎使用听书功能。今天我们一起阅读一个故事。", voice)
            .await?;
        let mut first = None;
        let mut ended = false;
        let mut silence = tts_core::audio::BoundarySilence::default();
        while let Some(chunk) = stream.recv().await {
            match chunk? {
                AudioChunk::Pcm(pcm) => {
                    if silence.push(pcm)?.is_some() {
                        first.get_or_insert(start.elapsed());
                    }
                }
                AudioChunk::End => {
                    let _ = silence.finish(true)?;
                    ended = true;
                    break;
                }
            }
        }
        anyhow::ensure!(ended && first.is_some(), "calibration synthesis failed");
        if i >= 3 {
            measurements.push(Measurements {
                total_ms: start.elapsed().as_secs_f64() * 1000.0,
                first_ms: first.unwrap().as_secs_f64() * 1000.0,
            });
        }
    }
    Ok(mean(&measurements))
}
pub async fn alignment(
    aligner: &dyn Aligner,
    text: &SpeechText,
    audio: &AudioClip,
) -> anyhow::Result<Measurements> {
    let mut measurements = Vec::new();
    for i in 0..8 {
        let start = Instant::now();
        tokio::time::timeout(Duration::from_secs(30), aligner.align(text, audio))
            .await?
            .map_err(|e| anyhow::anyhow!(e))?;
        if i >= 3 {
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            measurements.push(Measurements {
                total_ms: ms,
                first_ms: ms,
            });
        }
    }
    Ok(mean(&measurements))
}
pub async fn concurrent(
    backend: &dyn Backend,
    voice: &str,
    aligner: &dyn Aligner,
    text: &SpeechText,
    audio: &AudioClip,
) -> anyhow::Result<Measurements> {
    let mut measurements = Vec::new();
    for i in 0..8 {
        let started = Instant::now();
        let synthesis = async {
            let mut stream = backend
                .stream("你好，欢迎使用听书功能。今天我们一起阅读一个故事。", voice)
                .await?;
            let mut first = None;
            let mut ended = false;
            let mut silence = tts_core::audio::BoundarySilence::default();
            while let Some(chunk) = stream.recv().await {
                match chunk? {
                    AudioChunk::Pcm(pcm) => {
                        if silence.push(pcm)?.is_some() {
                            first.get_or_insert(started.elapsed());
                        }
                    }
                    AudioChunk::End => {
                        let _ = silence.finish(true)?;
                        ended = true;
                        break;
                    }
                }
            }
            anyhow::ensure!(ended, "incomplete concurrent calibration");
            first.ok_or_else(|| anyhow::anyhow!("no concurrent calibration audio"))
        };
        let alignment = async {
            tokio::time::timeout(Duration::from_secs(30), aligner.align(text, audio))
                .await?
                .map_err(|e| anyhow::anyhow!(e))
        };
        let (first, aligned) = tokio::join!(synthesis, alignment);
        let first = first?;
        aligned?;
        if i >= 3 {
            measurements.push(Measurements {
                total_ms: started.elapsed().as_secs_f64() * 1000.0,
                first_ms: first.as_secs_f64() * 1000.0,
            });
        }
    }
    Ok(mean(&measurements))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_cache_records_do_not_replace_each_other() {
        let root = tempfile::tempdir().unwrap();
        for key in ["moss-model", "qwen-model"] {
            save(
                root.path(),
                "tts",
                &Record {
                    key: key.into(),
                    device: Device::Cpu,
                    reason: key.into(),
                },
            )
            .unwrap();
        }
        for key in ["moss-model", "qwen-model"] {
            assert_eq!(cached(root.path(), "tts", key).unwrap().reason, key);
        }
    }
    #[test]
    fn speedup_and_first_audio_must_both_pass() {
        let baseline = Measurements {
            total_ms: 1000.0,
            first_ms: 100.0,
        };
        assert!(
            Measurements {
                total_ms: 850.0,
                first_ms: 110.0
            }
            .improves(baseline)
        );
        assert!(
            !Measurements {
                total_ms: 851.0,
                first_ms: 100.0
            }
            .improves(baseline)
        );
        assert!(
            !Measurements {
                total_ms: 700.0,
                first_ms: 111.0
            }
            .improves(baseline)
        );
    }
}
