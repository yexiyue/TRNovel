# VoxCPM2 Candle

The computation library reads the current Q8_0 BaseLM and F16 Acoustic GGUF
without converting or downloading model weights. It shares Candle 0.11.0 with
the workspace and performs inference without Python, Burn or llama.cpp.
The production `voxcpm` adapter uses this library following explicit user approval
of staged integration. Strict numerical acceptance is still incomplete; failed
comparisons and outstanding playback/listening/platform gates remain reported.
The former engine is retained only as a development benchmark dependency.

The worker also exposes `2b-bf16` as an explicitly authorized CUDA experiment,
labelled as experimental in the reader. It reads pinned original Safetensors and
the original F32 AudioVAE from the existing revision-scoped cache. Q8 remains
the default; experimental voices, reference caches and calibration are separate.
Exposing this option does not mark strict numerical or listening gates passed.

The library implements MiniCPM4, ResidualLM, LocalEncoder, FSQ, LocalDiT,
fixed-step CFM and causal AudioVAE V2. CPU keeps the quantized backbone and
F32 acoustic weights; CUDA/Metal use F16 acoustic weights with F32 sampling,
normalization reductions and output validation. Decoder state contains only
causal receptive fields and transpose-convolution tails. KV growth is bounded
by the request's context plus its frame limit, with a hard 4096-token bound.

`Model::load_with_cancel`, `reference` and `generate` accept cancellation checks.
`generate` reports `Eos`, `Cancelled` or `Truncated`; an error remains an error.
PCM is mono 48kHz and is checked before each callback. A cancelled request does
not deliver further PCM. Caller-owned channels, thread ownership and checkpoint
completion remain adapter responsibilities. Only EOS with valid PCM can finish
a segment.

A reference without transcript uses the reference prefix; one with transcript
uses continuation. Voice design uses the same model with `(description)text`
to produce a short reference, then stores the WAV and its text for cloning.
Legacy `features.json` is never consumed. The adapter rebuilds
`candle-reference-v1.json` from the retained WAV and verifies implementation,
model/revision, weight manifest, reference digest, transcript and precision.
Only one encoded voice stays resident per inference thread.

The CFM solver retains one model-local timestep schedule and recomputes it when
the step count changes. Conditioning is projected once per latent patch.
Both caches preserve the batch-two CFG arithmetic and retain iteration/layer
cancellation checks; they never reuse diffusion-dependent Transformer KV.

## Original weight comparison

`Model::load_original(directory, device, dtype, cancel)` directly loads the
original Safetensors, aliases its tensor names without rewriting weights and
folds AudioVAE weight normalization in Rust. Networks support CUDA BF16/F16;
AudioVAE stays F32 as in the official implementation. CPU accepts F32.
This explicit comparison API does not change the production Q8 model choice.

Download verified pinned assets once with `tools/tts/download_voxcpm_official.py`.
Set `VOXCPM_BENCH_PRECISION=bf16` or `f16` and pass the original directory to
`voxcpm_candle_probe`. Its default `q8` continues to use the GGUF directory.
Each run uses the same corpus, reference and sampling, but original weights
also use the verified official tokenizer and F32 codec. Accordingly, this is
a comparison of complete inference paths, not of file containers alone.

## Reproducible validation

```text
cargo test -p voxcpm --lib
cargo build -p voxcpm --features cuda --example voxcpm_probe --release -j 2
target/release/examples/voxcpm_probe MODEL_DIRECTORY cuda OUTPUT.wav "Chinese text"

cargo build -p novel-tts-backends --no-default-features --features voxcpm-cuda,voxcpm/cuda --example voxcpm_candle_probe --release -j 2
target/release/examples/voxcpm_candle_probe MODEL_DIRECTORY cuda OUTPUT_DIRECTORY TEXT_FILE REFERENCE.wav TRANSCRIPT_FILE
```

The latter uses the production resampler and 180-byte segmentation. It performs
one warmup plus five rounds with seed 42, 10 CFM steps, CFG 2, temperature 1 and
limit 200. It exports segment WAVs, source text and per-round results, then tests
cancellation and the next request. `VOXCPM_BENCH_ROUNDS` adjusts development runs.
`VOXCPM_BENCH_PROFILE=1` enables synchronized module timing and must be kept off
for throughput measurements.

The matching preserved-engine hot benchmark is `voxcpm_native_benchmark`, built
with `cargo build -p voxcpm-sys --features cuda --example voxcpm_native_benchmark --release`. It accepts the same directory/device/output/
text/reference/transcript arguments and exports one warmup plus five rounds.
Both examples are development tools; the production backend has no engine selector.

The development-only `tools/tts/voxcpm_reference.py` imports modules from the
pinned official checkout and creates real-weight numerical tensors. Run ignored
tests with `VOXCPM_TEST_MODELS` and `VOXCPM_TEST_REFERENCE` pointing at local
directories, using `--ignored --nocapture --test-threads=1`. No model is downloaded
by tests. F32 assertions use atol=1e-5 and rtol=1e-4; Q8/F16 errors have separate
reports. See `SOURCE.md`, the fixture README and
`dev-notes/voxcpm-candle-acceptance.md` for identities and acceptance status.
Use `cargo test --release` for real-weight oracles: Debug's unoptimized matrix
arithmetic is much slower. The numerical thresholds are identical in both profiles.

```text
python tools/tts/voxcpm_reference.py --source OFFICIAL_CHECKOUT --gguf-python GGUF_PY_DIRECTORY --models MODEL_DIRECTORY --output ORACLE_DIRECTORY
python tools/tts/voxcpm_tokenizer_reference.py --source OFFICIAL_CHECKOUT --gguf MODEL_DIRECTORY/VoxCPM2-BaseLM-Q8_0.gguf --gguf-python GGUF_PY_DIRECTORY --fixtures crates/voxcpm/tests/fixtures/tokenizer.json
python tools/tts/voxcpm_transcribe.py OUTPUT_DIRECTORY/round-5
```

The oracle uses development Torch/NumPy/pydantic/einops, the tokenizer check also
uses SentencePiece, and transcription uses faster-whisper. These are optional
development dependencies. They are never installed or invoked by the runtime.
