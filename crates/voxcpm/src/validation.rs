//! Opt-in tests against tensors emitted by tools/tts/voxcpm_reference.py.
use super::{acoustic::Acoustic, codec::Codec, transformer::Transformer, weights::Weights};
use candle_core::{DType, Device, Tensor};
use std::path::{Path, PathBuf};

fn fixture(directory: &Path, name: &str) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(directory.join(format!("{name}.json"))).unwrap()).unwrap()
}
fn tensor(f: &serde_json::Value, name: &str) -> Tensor {
    let shape: Vec<usize> = serde_json::from_value(f[name]["shape"].clone()).unwrap();
    let data: Vec<f32> = serde_json::from_value(f[name]["data"].clone()).unwrap();
    Tensor::from_vec(data, shape, &Device::Cpu).unwrap()
}
static FAILURES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn close(name: &str, a: &Tensor, b: &Tensor) {
    assert_eq!(a.dims(), b.dims(), "{name} shape");
    let a = a.flatten_all().unwrap().to_vec1::<f32>().unwrap();
    let b = b.flatten_all().unwrap().to_vec1::<f32>().unwrap();
    assert!(
        a.iter().chain(&b).all(|v| v.is_finite()),
        "{name}: nonfinite numerical output"
    );
    let max = a
        .iter()
        .zip(&b)
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    let failures = a
        .iter()
        .zip(&b)
        .filter(|(a, b)| (**a - **b).abs() > 1e-5 + 1e-4 * b.abs())
        .count();
    println!(
        "{name}: max_abs={max}, outside_tolerance={failures}/{}",
        a.len()
    );
    FAILURES.fetch_add(failures, std::sync::atomic::Ordering::Relaxed);
}

fn report_error(name: &str, a: &Tensor, b: &Tensor) {
    assert_eq!(a.dims(), b.dims());
    let a = a
        .to_dtype(DType::F32)
        .unwrap()
        .flatten_all()
        .unwrap()
        .to_vec1::<f32>()
        .unwrap();
    let b = b
        .to_dtype(DType::F32)
        .unwrap()
        .flatten_all()
        .unwrap()
        .to_vec1::<f32>()
        .unwrap();
    assert!(a.iter().chain(&b).all(|v| v.is_finite()));
    let max = a
        .iter()
        .zip(&b)
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    let rms = (a
        .iter()
        .zip(&b)
        .map(|(a, b)| ((*a - *b) as f64).powi(2))
        .sum::<f64>()
        / a.len() as f64)
        .sqrt();
    println!(
        "{}",
        serde_json::json!({"path":name,"max_abs":max,"rms":rms,"elements":a.len()})
    );
}

#[test]
#[ignore = "requires existing GGUF and numerical tensors"]
fn cpu_quantized_error_report() {
    let models =
        PathBuf::from(std::env::var("VOXCPM_TEST_MODELS").expect("set VOXCPM_TEST_MODELS"));
    let reference =
        PathBuf::from(std::env::var("VOXCPM_TEST_REFERENCE").expect("set VOXCPM_TEST_REFERENCE"));
    let mut w = Weights::open(
        &models.join("VoxCPM2-BaseLM-Q8_0.gguf"),
        &Device::Cpu,
        DType::F32,
    )
    .unwrap();
    let rope = w.tensor("rope_factors_short.weight").unwrap();
    let model = Transformer::load(&mut w, "", 28, "output_norm.weight", Some(rope), true).unwrap();
    let f = fixture(&reference, "base");
    report_error(
        "cpu-q8-base",
        &model.forward(&tensor(&f, "x"), None, &|| false).unwrap(),
        &tensor(&f, "y"),
    );
}

#[cfg(all(feature = "cuda", any(target_os = "windows", target_os = "linux")))]
#[test]
#[ignore = "requires local weights, numerical oracle and CUDA GPU"]
fn cuda_quantized_and_f16_error_report() {
    let models =
        PathBuf::from(std::env::var("VOXCPM_TEST_MODELS").expect("set VOXCPM_TEST_MODELS"));
    let reference =
        PathBuf::from(std::env::var("VOXCPM_TEST_REFERENCE").expect("set VOXCPM_TEST_REFERENCE"));
    let device = Device::new_cuda(0).unwrap();
    let mut w = Weights::open(
        &models.join("VoxCPM2-BaseLM-Q8_0.gguf"),
        &device,
        DType::F32,
    )
    .unwrap();
    let rope = w.tensor("rope_factors_short.weight").unwrap();
    let model = Transformer::load(
        &mut w,
        "",
        28,
        "output_norm.weight",
        Some(rope.clone()),
        true,
    )
    .unwrap();
    let f = fixture(&reference, "base");
    report_error(
        "cuda-q8-base",
        &model
            .forward(&tensor(&f, "x").to_device(&device).unwrap(), None, &|| {
                false
            })
            .unwrap(),
        &tensor(&f, "y"),
    );
    drop(model);
    drop(w);
    let mut w = Weights::open(
        &models.join("VoxCPM2-Acoustic-F16.gguf"),
        &device,
        DType::F16,
    )
    .unwrap();
    let mut a = Acoustic::load(&mut w, &rope).unwrap();
    let f = fixture(&reference, "cfm");
    let on_device = |key| {
        tensor(&f, key)
            .to_device(&device)
            .unwrap()
            .to_dtype(DType::F16)
            .unwrap()
    };
    let y = a
        .sample(
            &on_device("mu"),
            &on_device("cond"),
            &tensor(&f, "x").to_device(&device).unwrap(),
            10,
            2.0,
            &|| false,
        )
        .unwrap();
    report_error("cuda-f16-cfm", &y, &tensor(&f, "y"));
    drop(a);
    let mut codec = Codec::load(&mut w).unwrap();
    let f = fixture(&reference, "vae-decode");
    let x = tensor(&f, "x")
        .to_device(&device)
        .unwrap()
        .to_dtype(DType::F16)
        .unwrap();
    let full = codec.decode(&x, &|| false).unwrap();
    report_error("cuda-f16-vae", &full, &tensor(&f, "y"));
    codec.reset();
    let parts: Vec<_> = [(0, 1), (1, 3), (4, 4)]
        .into_iter()
        .map(|(s, n)| {
            codec
                .decode(&x.narrow(2, s, n).unwrap(), &|| false)
                .unwrap()
        })
        .collect();
    report_error(
        "cuda-f16-vae-chunked",
        &Tensor::cat(&parts, 2).unwrap(),
        &full,
    );
}
#[test]
#[ignore = "requires local GGUF weights and the pinned official numerical oracle"]
fn official_f32_and_streaming_oracle() {
    FAILURES.store(0, std::sync::atomic::Ordering::Relaxed);
    let models =
        PathBuf::from(std::env::var("VOXCPM_TEST_MODELS").expect("set VOXCPM_TEST_MODELS"));
    let reference =
        PathBuf::from(std::env::var("VOXCPM_TEST_REFERENCE").expect("set VOXCPM_TEST_REFERENCE"));
    let original = std::env::var_os("VOXCPM_ORACLE_ORIGINAL").is_some();
    let mut base = if original {
        Weights::native(&models.join("model.safetensors"), &Device::Cpu, DType::F32)
    } else {
        Weights::open(
            &models.join("VoxCPM2-BaseLM-Q8_0.gguf"),
            &Device::Cpu,
            DType::F32,
        )
    }
    .unwrap();
    base.quantized = false;
    let rope = if original {
        let config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(models.join("config.json")).unwrap()).unwrap();
        let factors: Vec<f32> =
            serde_json::from_value(config["lm_config"]["rope_scaling"]["short_factor"].clone())
                .unwrap();
        Tensor::from_vec(factors, 64, &Device::Cpu).unwrap()
    } else {
        base.tensor("rope_factors_short.weight").unwrap()
    };
    let model = Transformer::load(
        &mut base,
        "",
        28,
        "output_norm.weight",
        Some(rope.clone()),
        true,
    )
    .unwrap();
    let f = fixture(&reference, "base");
    let x = tensor(&f, "x");
    let y = model.forward(&x, None, &|| false).unwrap();
    close(
        if original {
            "base-f32-original"
        } else {
            "base-f32-dequantized"
        },
        &y,
        &tensor(&f, "y"),
    );
    let mut cache = model.cache(3);
    let parts: Vec<_> = (0..3)
        .map(|i| {
            model
                .forward(&x.narrow(1, i, 1).unwrap(), Some(&mut cache), &|| false)
                .unwrap()
        })
        .collect();
    close("base-incremental", &Tensor::cat(&parts, 1).unwrap(), &y);
    drop(model);
    drop(base);
    let mut w = if original {
        Weights::native(&models.join("model.safetensors"), &Device::Cpu, DType::F32)
    } else {
        Weights::open(
            &models.join("VoxCPM2-Acoustic-F16.gguf"),
            &Device::Cpu,
            DType::F32,
        )
    }
    .unwrap();
    for (name, prefix, count, norm, causal, rope) in [
        (
            "residual",
            "residual_lm.",
            8,
            "residual_lm.output_norm.weight",
            true,
            None,
        ),
        (
            "local",
            "locenc.",
            12,
            "locenc.norm.weight",
            false,
            Some(rope.clone()),
        ),
    ] {
        let model = Transformer::load(&mut w, prefix, count, norm, rope, causal).unwrap();
        let f = fixture(&reference, name);
        close(
            name,
            &model.forward(&tensor(&f, "x"), None, &|| false).unwrap(),
            &tensor(&f, "y"),
        );
    }
    let f = fixture(&reference, "dit-transformer");
    let dit = Transformer::load(
        &mut w,
        "locdit.",
        12,
        "locdit.norm.weight",
        Some(rope.clone()),
        false,
    )
    .unwrap();
    close(
        "dit-transformer",
        &dit.forward(&tensor(&f, "x"), None, &|| false).unwrap(),
        &tensor(&f, "y"),
    );
    drop(dit);
    let mut a = Acoustic::load(&mut w, &rope).unwrap();
    let f = fixture(&reference, "encoder");
    close(
        "encoder",
        &a.encode(&tensor(&f, "x"), &|| false).unwrap(),
        &tensor(&f, "y"),
    );
    let f = fixture(&reference, "fsq");
    close("fsq", &a.fsq(&tensor(&f, "x")).unwrap(), &tensor(&f, "y"));
    let f = fixture(&reference, "dit");
    let tokens = a
        .velocity_tokens(
            &tensor(&f, "x"),
            &tensor(&f, "mu"),
            &tensor(&f, "cond"),
            0.75,
        )
        .unwrap();
    close(
        "dit-input",
        &tokens,
        &tensor(&fixture(&reference, "dit-transformer"), "x"),
    );
    close(
        "dit",
        &a.velocity(
            &tensor(&f, "x"),
            &tensor(&f, "mu"),
            &tensor(&f, "cond"),
            0.75,
            &|| false,
        )
        .unwrap(),
        &tensor(&f, "y"),
    );
    let f = fixture(&reference, "cfm");
    close(
        "cfm",
        &a.sample(
            &tensor(&f, "mu"),
            &tensor(&f, "cond"),
            &tensor(&f, "x"),
            10,
            2.0,
            &|| false,
        )
        .unwrap(),
        &tensor(&f, "y"),
    );
    drop(a);
    if original {
        w = Weights::native(&models.join("audiovae.pth"), &Device::Cpu, DType::F32).unwrap();
    }
    let mut codec = Codec::load(&mut w).unwrap();
    let f = fixture(&reference, "vae-encode");
    close(
        "vae-encode",
        &codec.encode(&tensor(&f, "x"), &|| false).unwrap(),
        &tensor(&f, "y"),
    );
    let f = fixture(&reference, "vae-decode");
    let x = tensor(&f, "x");
    codec.reset();
    let full = codec.decode(&x, &|| false).unwrap();
    close("vae-decode", &full, &tensor(&f, "y"));
    codec.reset();
    let parts: Vec<_> = [(0, 1), (1, 3), (4, 4)]
        .into_iter()
        .map(|(start, len)| {
            codec
                .decode(&x.narrow(2, start, len).unwrap(), &|| false)
                .unwrap()
        })
        .collect();
    close("vae-chunked", &Tensor::cat(&parts, 2).unwrap(), &full);
    codec.reset();
    close("vae-reset", &codec.decode(&x, &|| false).unwrap(), &full);
    assert_eq!(
        FAILURES.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "official numerical tolerance exceeded"
    );
}

#[test]
#[ignore = "requires local acoustic weights and the pinned fixed-noise oracle"]
fn cached_cfm_preserves_fixed_noise_and_step_changes() {
    let models = PathBuf::from(std::env::var("VOXCPM_TEST_MODELS").unwrap());
    let reference = PathBuf::from(std::env::var("VOXCPM_TEST_REFERENCE").unwrap());
    let mut base = Weights::open(
        &models.join("VoxCPM2-BaseLM-Q8_0.gguf"),
        &Device::Cpu,
        DType::F32,
    )
    .unwrap();
    let rope = base.tensor("rope_factors_short.weight").unwrap();
    drop(base);
    let mut w = Weights::open(
        &models.join("VoxCPM2-Acoustic-F16.gguf"),
        &Device::Cpu,
        DType::F32,
    )
    .unwrap();
    let mut a = Acoustic::load(&mut w, &rope).unwrap();
    let f = fixture(&reference, "cfm");
    let mu = tensor(&f, "mu");
    let cond = tensor(&f, "cond");
    let noise = tensor(&f, "x");
    let expected = tensor(&f, "y")
        .flatten_all()
        .unwrap()
        .to_vec1::<f32>()
        .unwrap();
    for steps in [10, 4, 10] {
        let y = a.sample(&mu, &cond, &noise, steps, 2.0, &|| false).unwrap();
        if steps == 10 {
            let actual = y.flatten_all().unwrap().to_vec1::<f32>().unwrap();
            assert!(
                actual
                    .iter()
                    .zip(&expected)
                    .all(|(a, b)| a.is_finite() && (a - b).abs() <= 1e-5 + 1e-4 * b.abs())
            );
        }
    }
    assert!(a.sample(&mu, &cond, &noise, 8, 2.0, &|| true).is_err());
    let resumed = a.sample(&mu, &cond, &noise, 10, 2.0, &|| false).unwrap();
    assert!(
        resumed
            .flatten_all()
            .unwrap()
            .to_vec1::<f32>()
            .unwrap()
            .iter()
            .zip(&expected)
            .all(|(a, b)| a.is_finite() && (a - b).abs() <= 1e-5 + 1e-4 * b.abs())
    );
}
