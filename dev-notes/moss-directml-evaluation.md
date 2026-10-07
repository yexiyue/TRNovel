# MOSS Nano DirectML hardware evaluation

This is an opt-in Rust probe, not a reader device release. Both AMD discrete and
integrated DX12 hardware are eligible; select an adapter explicitly. The local
Ryzen 9700X integrated GPU does not represent RX-series discrete GPU performance.
Qwen/Vox/Omni/MOSS 1.7B remain Candle and are not accelerated by this probe.

## Reproduce

```text
cargo build --release -p novel-tts-backends --no-default-features --features directml-probe --example moss_directml_probe
target/release/examples/moss_directml_probe.exe list
target/release/examples/moss_directml_probe.exe MODEL_DIR TEXT_FILE OUTPUT_DIR cpu
target/release/examples/moss_directml_probe.exe MODEL_DIR TEXT_FILE OUTPUT_DIR dml-host DXGI_INDEX 5
target/release/examples/moss_directml_probe.exe MODEL_DIR TEXT_FILE OUTPUT_DIR dml-cache DXGI_INDEX 5
```

`MODEL_DIR` is the existing `~/.novel-tts/moss` directory, containing `tts/` and
`codec/`, not the parent model root. Text is UTF-8, one semantic segment per line.
The probe reads cached models and builtin Weiguo reference codes, with seed42.
Round0 is warmup; subsequent five rounds are measured. Run paths serially without
concurrent builds/inference. Save the executable hash, adapter LUID/name, driver
and ONNX identities when sharing results. Adapter indices can change: always list
again and check the saved identity. Software or incompatible devices reject.

For a separate profiled run, append `DXGI_INDEX 1 profile`. CPU accepts an ignored
index placeholder when specifying a round count, e.g. `cpu 0 1 profile`.
Summarize the resulting profiles with the development-only command:

```text
python tools/tts/directml_profiles.py OUTPUT_DIR
```

Keep profiled timing separate from normal timing. Provider event counts are not
operator-coverage percentages because DML graphs fuse nodes. CPU shape/control
nodes are reported; registering an EP does not prove GPU acceleration. The unused
voice encoder may have no execution events. The product has no Python dependency.

## Semantics and boundaries

The probe shares Nano normalization, tokenizer, voice codes, sessions and sampling.
Sessions are built and run on a dedicated owned thread; the controller consumes a
bounded PCM channel. DML disables memory patterns and parallel execution. Host mode
uses normal Run. Cache mode requests DirectML outputs for persistent global/codec
state and validates their actual ORT allocation identity; it does not keep every
intermediate tensor on GPU. Other outputs remain CPU-visible for sampling and PCM.

Every completed WAV requires finite, nonempty, nonzero PCM and normal EOS. Errors
and truncation fail the probe and are recorded in summary.json. Dropping the receiver
after first PCM must cancel, followed by successful generation. Adapter selection
errors do not load another GPU. Existing product defaults/config/protocol and Auto
remain unchanged pending a separate qualification change.

## Local evidence

Measured on 2026-10-07: Windows 11 build28000, AMD driver32.0.21042.62,
Ryzen7 9700X, DXGI index1, vendor1002/device13c0. D3D12 device creation passes;
DXGI reports 509358080 dedicated bytes and 8136970240 shared bytes. Shared memory
capacity is not bandwidth or guaranteed free model space. AMD discrete hardware,
full graph coverage, human listening and long playback remain unverified.

Release executable SHA-256:
`7a8d7c1b0a901bd1b775b7fb3bf7ef17b1eba31655a92f31bef6507385694104`.
The measured executable is preserved as
`target/tts-integration/directml-measured-probe.exe`. Model/tokenizer hashes are in
`target/tts-integration/directml-model-identities.json`; models were read from the
existing default cache without downloads. Adapter LUID is `00000000:00016b00`.
ORT reports Runtime1.28 release branch commit `da9b5e3`; the matching `DirectML.dll`
SHA-256 is `9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1`.

The five-line corpus covers a title, dialogue, date/money/time, polyphones and
English mixed with Chinese. This is a short corpus, not long-paragraph or playback
qualification. One warmup and five serial measured rounds used the same text,
Weiguo reference and seed42, without concurrent builds or inference.

The measured UTF-8 corpus was:

```text
第一章，山中的约定
他轻声问道：“明天还会见面吗？”她点头说：“会的。”
今天是2026年10月7日，车票售价128元，发车时间是下午三点。
行长走过银行门口，看见远处重重的山影。他说这项工作很重要。
她打开电脑，输入Hello world，然后按下Enter键。
```

| Path | Five RTF values | Mean RTF | Hot first PCM range | Cold load | Cancel / release |
| --- | --- | --- | --- | --- | --- |
| CPU | .4285 / .4340 / .4186 / .4180 / .4130 | **.4224** | 144–160 ms | 2441 ms | 15 / 77 ms |
| AMD DML host outputs | 1.2749 / 1.2715 / 1.2653 / 1.2698 / 1.2655 | **1.2694** | 479–488 ms | 3674 ms | 41 / 345 ms |
| AMD DML device cache | 1.1172 / 1.1082 / 1.1145 / 1.1138 / 1.1189 | **1.1145** | 439–457 ms | 2968 ms | 31 / 281 ms |

First PCM is the first segment of each hot round; cancel is after first PCM.
All paths produced finite, nonzero PCM with normal EOS, and cancellation followed
by a new complete request passed. Device-cache allocation assertions passed.
CPU produced 28.4 seconds per round, DML 32.4 seconds: the same seed does not imply
identical numerical trajectories, content or quality across execution providers.
The cache mode improved DML RTF by about 12.2%, but this iGPU remains slower than
CPU and does not meet real-time throughput. This conclusion is specific to the
9700X iGPU, not AMD discrete GPUs. Peak RSS/VRAM was not measured.

Raw summaries and WAVs are in `target/tts-integration/directml-cpu/`,
`directml-amd-host/` and `directml-amd-cache/`; comparison is
`target/tts-integration/directml-comparison.json`. Invalid adapter99 rejected
without switching hardware. Software-device rejection and an eligible synthetic
8GiB AMD discrete adapter are covered by the selection unit test.
The actual Microsoft software adapter3 also rejected in a CLI run. A missing model
path produced a nonzero exit and saved error summary under `directml-missing-model/`.

A separate short profiled run is saved in `directml-amd-profile/`. Native provider
events show MatMul on DML for prefill/global decode/local decode, and MatMul/Conv
on DML for audio decode. CPU events still include Gather/Slice/Concat and codec
arithmetic such as Mul/Div/Sqrt. GPU execution is real, but this is not complete
GPU coverage. The voice encoder is unused by builtin Weiguo and has no execution
events; arbitrary WAV reference encoding has not been validated. Profiler event
counts cannot be interpreted as operator-coverage percentages or isolated GPU
kernel time. Profiled RTF is excluded from the table above.

Development-only Whisper small/int8 CPU transcripts are saved under the CPU and
DML-cache `round-5/transcription.json`. Both contain possible recognition or speech
errors; date/money/English differences are more conspicuous for DML. ASR alone
cannot diagnose omitted words or a precision defect. Human listening and a
numerical comparison remain outstanding. These outputs do not qualify quality.

The product device catalogue and Auto remain unchanged. AMD discrete hardware,
reference encoding, long paragraphs and 30-minute playback need separate evidence
before exposing DirectML as a supported reader device.

## Build and regression checks

Windows workspace all-features lib/tests/examples: 537 passed, 7 ignored (excluding
a filtered subprocess duplicate). All-targets all-features Clippy with denied
warnings, all-features private rustdoc for workspace libraries/examples, fmt,
diff whitespace and strict OpenSpec validation passed. The optional probe release
build and empty-feature worker check passed. The reader's normal dependency tree
contains no ORT, Candle or inference backend library. No product device enum,
configuration or reader UI was changed. Linux/macOS builds were not run for this
Windows-only probe. The existing CUDA SDK CRT linker warning remains recorded in
toolchain notes; this work does not suppress it.

Logs are `target/tts-integration/directml-workspace-tests.log`,
`directml-clippy.log`, `directml-rustdoc.log`, `directml-empty-worker.log` and
`directml-final-build.log`.

The final source's release executable SHA-256 is
`6f0bd0a02be58f048eff11581bcdec46d7e064f24e9984f59a4ad25178cea265`.
After source cleanup, short CPU and AMD device-cache runs both passed EOS,
cancel/reuse again (`directml-final-cpu/` and `directml-final-amd/`). The table's
five-round measurements remain tied to the preserved earlier executable.
