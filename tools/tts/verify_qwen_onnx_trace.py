"""Compare real Rust prompt/prefill/chunk decoding with pinned Python helpers."""
import argparse
import json
from pathlib import Path
import sys

import numpy as np
import onnxruntime as ort
import soundfile as sf

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--upstream", required=True)
p.add_argument("--source", required=True)
p.add_argument("--export", required=True, type=Path)
p.add_argument("--trace", required=True, type=Path)
p.add_argument("--wav", required=True)
p.add_argument("--output", required=True, type=Path)
a = p.parse_args()
sys.path.insert(0, a.upstream)
from src import OnnxSessionRunner
original_options = OnnxSessionRunner.make_quiet_session_options


def options(*args, **kwargs):
    value = original_options(*args, **kwargs)
    value.intra_op_num_threads = 4
    return value


OnnxSessionRunner.make_quiet_session_options = options
from src.builders import CustomVoicePromptBuilder
from src.tokenizer_decode_aux import build_tokenizer_decode_aux_inputs

t = json.loads(a.trace.read_text())
h = t["hidden_size"]
builder = CustomVoicePromptBuilder(a.source, a.export / "onnx", dtype=np.float32)
prompt = builder.build(t["text"], t["voice"], language="chinese", instruct=t["style"], non_streaming_mode=t.get("non_streaming_mode", True))
rust_prompt = np.array(t["prompt"], np.float32).reshape(prompt.inputs_embeds.shape)
np.testing.assert_allclose(rust_prompt, prompt.inputs_embeds, atol=1e-5, rtol=1e-5)
np.testing.assert_allclose(np.array(t["pad"], np.float32).reshape(prompt.tts_pad_embed.shape), prompt.tts_pad_embed, atol=1e-5, rtol=1e-5)
if "trailing" in t:
    np.testing.assert_allclose(np.array(t["trailing"], np.float32).reshape(prompt.trailing_text_hidden.shape), prompt.trailing_text_hidden, atol=1e-5, rtol=1e-5)
config = builder.talker_config
so = ort.SessionOptions()
so.intra_op_num_threads = 4
session = ort.InferenceSession(str(a.export / "onnx/talker/talker_core.onnx"), so, providers=["CPUExecutionProvider"])
n = rust_prompt.shape[1]
mask = np.full((1, 1, n, n), np.finfo(np.float32).min, np.float32)
mask[0, 0][np.tril_indices(n)] = 0
feed = dict(inputs_embeds=prompt.inputs_embeds, attention_mask=mask, cache_position=np.arange(n, dtype=np.int64))
for layer in range(config["num_hidden_layers"]):
    for kind in ("key", "value"):
        feed[f"past_{kind}_{layer}"] = np.empty((1, config["num_key_value_heads"], 0, config["head_dim"]), np.float32)
logits = session.run(["logits"], feed)[0].reshape(-1)
np.testing.assert_allclose(logits, t["first_logits"], atol=1e-4, rtol=1e-4)
del session
decoder = ort.InferenceSession(str(a.export / "onnx/tokenizer/tokenizer12hz_decode_chunk.onnx"), so, providers=["CPUExecutionProvider"])
codes = np.array(t["codes"], np.int64).reshape(1, -1, 16)
chunks = []
emitted = 0
while emitted < codes.shape[1]:
    end = min(emitted + 10, codes.shape[1])
    start = max(0, emitted - 25)
    feed = dict(audio_codes=codes[:, start:end], context_frames=np.array(emitted - start, np.int64))
    feed.update(build_tokenizer_decode_aux_inputs(end - start, np.float32))
    pcm, lengths = decoder.run(["audio_values", "lengths"], feed)
    assert int(lengths.reshape(-1)[0]) == (end - emitted) * 1920
    chunks.append(pcm.reshape(-1))
    emitted = end
reference = np.concatenate(chunks)
actual, rate = sf.read(a.wav, dtype="float32")
assert rate == 24000 and len(actual) == len(reference)
np.testing.assert_allclose(actual, reference, atol=5e-5, rtol=5e-4)
result = dict(prompt_max_abs=float(np.max(np.abs(rust_prompt - prompt.inputs_embeds))),
              first_logits_max_abs=float(np.max(np.abs(logits - t["first_logits"]))),
              pcm_max_abs=float(np.max(np.abs(actual - reference))), frames=codes.shape[1], passed=True)
a.output.write_text(json.dumps(result, indent=2) + "\n")
sf.write(str(a.output.with_suffix(".wav")), reference, rate, subtype="FLOAT")
print(json.dumps(result))
