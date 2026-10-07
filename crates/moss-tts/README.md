# MOSS Candle computation

Experimental Rust implementation of the official MOSS Local, Realtime and Delay
model schedulers and MOSS Audio Tokenizer. This crate uses the workspace's pinned
Candle version and platform features; it does not manage downloads, playback,
configuration or a server. No Python is invoked by inference.

The worker exposes Local and Realtime as optional GPU trial models after real
generation/cancellation checks. VoiceGenerator creates reusable references. The
existing Nano ONNX adapter remains intact. Successful termination does not prove
spoken coverage. See `dev-notes/moss-candle-acceptance.md` for remaining acceptance.

## Provenance

- TTS equations and scheduling: OpenMOSS/MOSS-TTS commit
  `934d6826b084c46a0d033402174d5f8ac4ed2519`, Apache-2.0.
- Audio codec equations: OpenMOSS/MOSS-Audio-Tokenizer commit
  `8c50ac4c5d7287d2ed6ea20a08c90ca439887d23`, Apache-2.0.
- `assets/*.json` records immutable official weight URLs, sizes and SHA-256.
  Official model cards identify the MOSS weights as Apache-2.0.
- `tools/tts/moss_reference_fixture.py` creates numerical fixtures from official
  Python modules for development only. Generated fixtures require no Python to test.

## Experimental probe

```text
cargo run -p moss-tts --release --features cuda --example local_probe -- MODEL_DIR CODEC_DIR OUTPUT.wav "你好，欢迎收听今天的故事。" cuda
```

The probe writes a WAV and RVQ token JSON without opening an audio device.
Model and codec directories must contain verified official safetensors and configs.
The codec preserves decoder state between PCM chunks and resets before an utterance.

Set `MOSS_MODE` to `local-1.7b`, `realtime-1.7b` or `voice-design-1.7b`.
`MOSS_REFERENCE` accepts a mono 24kHz WAV; `MOSS_INSTRUCTION` supplies a design
description. CUDA model weights use BF16, codec weights F16; `MOSS_DTYPE` overrides
model precision for development comparisons. VoiceGenerator F16 overflowed on the
real checkpoint and must not be the CUDA default.

Development reference scripts require the pinned upstream checkouts in
`target/tts-integration/moss-upstream` and `moss-codec-upstream`, PyTorch and
Transformers 5.0.0. Their SDPA implementation supplies the causal behavior required
by the upstream position-free Local attention. Python is not a runtime dependency.
