"""Compare real Rust CFG prefill and decoded codes with FP32 PyTorch."""
import argparse
import json
from pathlib import Path
import sys

import numpy as np
import torch

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--upstream", required=True)
p.add_argument("--python-source", required=True)
p.add_argument("--source", required=True, type=Path)
p.add_argument("--trace", required=True, type=Path)
p.add_argument("--output", required=True, type=Path)
a = p.parse_args()
sys.path[:0] = [a.upstream, a.python_source]
import _common
_common.PROJECT_ROOT = Path(a.python_source).resolve()
_common.PT_MODEL_DIR = a.source.resolve()
_common.AUDIO_TOKENIZER_DIR = a.source.resolve() / "audio_tokenizer"
from export_lm import OmniVoiceLMWrapper
from export_audio_tokenizer import AudioDecoderWrapper

torch.set_num_threads(4)
t = json.loads(a.trace.read_text())
c, n = t["channels"], t["seq"]
model = _common.load_omnivoice(torch.device("cpu"))
inputs = [torch.tensor(t["input_ids"], dtype=torch.long).reshape(2, c, n),
          torch.tensor(t["audio_mask"], dtype=torch.bool).reshape(2, n),
          torch.tensor(t["attention_mask"], dtype=torch.bool).reshape(2, 1, n, n),
          torch.arange(n).expand(2, -1)]
with torch.no_grad():
    logits = OmniVoiceLMWrapper(model)(*inputs).numpy().reshape(-1)
np.testing.assert_allclose(logits, t["first_logits"], atol=2e-3, rtol=5e-3)
lm_error = float(np.max(np.abs(logits - t["first_logits"])))
del model
tokenizer = _common.load_audio_tokenizer(torch.device("cpu"))
codes = torch.tensor(t["codes"], dtype=torch.long).reshape(1, c, t["target"])
with torch.no_grad():
    audio = AudioDecoderWrapper(tokenizer)(codes).numpy().reshape(-1)
np.testing.assert_allclose(audio, t["raw_audio"], atol=5e-5, rtol=5e-3)
result = dict(lm_max_abs=lm_error, pcm_max_abs=float(np.max(np.abs(audio - t["raw_audio"]))),
              frames=t["target"], samples=len(audio), passed=True)
a.output.write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result))
