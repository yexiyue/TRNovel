"""Development-only Nano codec fixtures from the pinned ONNX export."""

import json, numpy as np, onnxruntime as ort
from pathlib import Path

base = Path("crates/novel-tts-backends/src/moss/assets")
m = json.loads((base / "browser_poc_manifest.json").read_text())
meta = json.loads((base / "codec_browser_onnx_meta.json").read_text())
codes = next(
    v["prompt_audio_codes"] for v in m["builtin_voices"] if v["voice"] == "Weiguo"
)[:65]
s = ort.SessionOptions()
s.intra_op_num_threads = 4
session = ort.InferenceSession(
    str(Path.home() / ".novel-tts/moss/codec/moss_audio_tokenizer_decode_step.onnx"),
    s,
    providers=["CPUExecutionProvider"],
)
state = {}
mapping = {}
for spec in meta["streaming_decode"]["transformer_offsets"]:
    state[spec["input_name"]] = np.zeros(spec["shape"], np.int32)
    mapping[spec["input_name"]] = spec["output_name"]
for spec in meta["streaming_decode"]["attention_caches"]:
    for prefix, shape, dtype, fill in [
        ("offset", "offset_shape", np.int32, 0),
        ("cached_keys", "cache_shape", np.float32, 0),
        ("cached_values", "cache_shape", np.float32, 0),
        ("cached_positions", "positions_shape", np.int32, -1),
    ]:
        state[spec[prefix + "_input_name"]] = np.full(spec[shape], fill, dtype)
        mapping[spec[prefix + "_input_name"]] = spec[prefix + "_output_name"]
chunks = []
for i in range(0, len(codes), 3):
    frames = codes[i : i + 3]
    feeds = dict(
        state,
        audio_codes=np.array([frames], np.int32),
        audio_code_lengths=np.array([len(frames)], np.int32),
    )
    outputs = dict(
        zip([v.name for v in session.get_outputs()], session.run(None, feeds))
    )
    n = int(outputs["audio_lengths"][0])
    chunks.append(outputs["audio"][0, :, :n].T.reshape(-1))
    state = {name: outputs[output] for name, output in mapping.items()}
audio = np.concatenate(chunks)
np.save("target/tts-integration/nano-codec-onnx.npy", audio)
fixture = {
    "codes": codes,
    "sample_count": len(audio),
    "probes": [
        {"index": int(i), "value": float(audio[i])}
        for i in np.linspace(0, len(audio) - 1, 1024, dtype=np.int64)
    ],
}
Path("crates/moss-tts/tests/fixtures/nano-codec.json").write_text(
    json.dumps(fixture, indent=2) + "\n"
)
print(len(codes), len(audio))
