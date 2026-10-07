//! Native ONNX compatibility gate; does not open an audio device.
use ort::session::Session;
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("ONNX path required"))?;
    let provider = args.next().unwrap_or_else(|| "cpu".into());
    let builder = Session::builder()?
        .with_intra_threads(4)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .with_profiling("target/alignment-research/ort-profile")
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut builder = if provider == "coreml" {
        #[cfg(feature = "coreml")]
        {
            use ort::ep::{CoreML, coreml::ModelFormat};
            builder
                .with_execution_providers([CoreML::default()
                    .with_static_input_shapes(true)
                    .with_model_format(ModelFormat::NeuralNetwork)
                    .build()
                    .error_on_failure()])
                .map_err(|e| anyhow::anyhow!("{e}"))?
        }
        #[cfg(not(feature = "coreml"))]
        anyhow::bail!("coreml feature required");
    } else {
        anyhow::ensure!(provider == "cpu", "unsupported probe provider");
        builder
            .with_execution_providers([ort::ep::CPU::default().build().error_on_failure()])
            .map_err(|e| anyhow::anyhow!("{e}"))?
    };
    let start = std::time::Instant::now();
    let mut session = builder.commit_from_file(path)?;
    eprintln!(
        "loaded in {:?}; provider requested: {provider}",
        start.elapsed()
    );
    for input in session.inputs() {
        eprintln!("input {} {:?}", input.name(), input.dtype());
    }
    for output in session.outputs() {
        eprintln!("output {} {:?}", output.name(), output.dtype());
    }
    let input = args.next().map(std::fs::read_to_string).transpose()?;
    let input: Option<serde_json::Value> =
        input.map(|json| serde_json::from_str(&json)).transpose()?;
    let mut ids = vec![151669i64];
    ids.extend(std::iter::repeat_n(151676, 13));
    ids.extend([151670, 14990, 151705, 151705, 1879, 151705, 151705]);
    let (ids, features, mask) = if let Some(input) = &input {
        (
            serde_json::from_value(input["input_ids"]["data"].clone())?,
            serde_json::from_value(input["input_features"]["data"].clone())?,
            serde_json::from_value(input["feature_attention_mask"]["data"].clone())?,
        )
    } else {
        (ids, vec![0f32; 12800], vec![1i32; 100])
    };
    let length = ids.len();
    let frames = mask.len();
    let outputs = session.run(ort::inputs![
        "input_ids" => ort::value::Tensor::from_array(([1,length], ids))?,
        "attention_mask" => ort::value::Tensor::from_array(([1,length], vec![1i64;length]))?,
        "input_features" => ort::value::Tensor::from_array(([1,128,frames], features))?,
        "feature_attention_mask" => ort::value::Tensor::from_array(([1,frames], mask))?,
    ])?;
    let (shape, logits) = outputs["logits"].try_extract_tensor::<f32>()?;
    anyhow::ensure!(
        logits.iter().all(|value| value.is_finite()),
        "nonfinite logits"
    );
    if let Some(input) = input {
        let actual: Vec<usize> = logits
            .as_chunks::<5000>()
            .0
            .iter()
            .map(|row| {
                row.iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .unwrap()
                    .0
            })
            .collect();
        let expected: Vec<usize> = serde_json::from_value(input["expected"]["argmax"].clone())?;
        anyhow::ensure!(actual == expected, "reference argmax mismatch");
        eprintln!("all timestamp logits argmax match Python ORT reference");
    }
    eprintln!("inference shape {shape:?}, {} finite logits", logits.len());
    drop(outputs);
    eprintln!("profile {}", session.end_profiling()?);
    Ok(())
}
