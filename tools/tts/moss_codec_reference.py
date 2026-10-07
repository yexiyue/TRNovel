"""Compare a real Candle WAV against the pinned official codec (development only)."""
import importlib.util
import json
import sys
import time
from pathlib import Path

import numpy as np
import soundfile as sf
import torch
from transformers.utils import logging

root = Path(__file__).resolve().parents[2]
source = root / "target/tts-integration/moss-codec-upstream"
spec = importlib.util.spec_from_file_location(
    "moss_codec_upstream", source / "__init__.py", submodule_search_locations=[str(source)]
)
package = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = package
spec.loader.exec_module(package)
from moss_codec_upstream.modeling_moss_audio_tokenizer import MossAudioTokenizerModel

torch.set_num_threads(4)
logging.disable_progress_bar()
directory, wav_path = Path(sys.argv[1]), Path(sys.argv[2])
frames = json.loads(wav_path.with_suffix(".tokens.json").read_text())
start = time.monotonic()
model = MossAudioTokenizerModel.from_pretrained(directory, local_files_only=True).eval()
print("Official codec loaded", time.monotonic() - start, flush=True)
codes = torch.tensor(frames).T.unsqueeze(1)
with torch.inference_mode():
    audio = model.decode(codes, chunk_duration=0.8).audio.flatten().numpy()
actual, rate = sf.read(wav_path, dtype="float32")
assert rate == 24000 and actual.shape == audio.shape
error = actual - audio
metrics = {
    "samples": len(audio),
    "max_error": float(np.max(np.abs(error))),
    "rmse": float(np.sqrt(np.mean(error ** 2))),
    "correlation": float(np.corrcoef(actual, audio)[0, 1]),
    "seconds": time.monotonic() - start,
}
output = wav_path.with_name(wav_path.stem + "-official.wav")
sf.write(output, audio, rate, subtype="FLOAT")
wav_path.with_suffix(".codec-comparison.json").write_text(json.dumps(metrics, indent=2))
print(json.dumps(metrics), flush=True)
if len(sys.argv) > 3:
    reference, rate = sf.read(sys.argv[3], dtype="float32")
    assert rate == 24000 and reference.ndim == 1
    actual_codes = np.array(json.loads(wav_path.with_suffix(".reference-tokens.json").read_text()))
    with torch.inference_mode():
        official_codes = model.encode(
            torch.from_numpy(reference).reshape(1, 1, -1),
            num_quantizers=actual_codes.shape[1],
        ).audio_codes[:, 0, :].T.numpy()
    assert actual_codes.shape == official_codes.shape
    agreement = (actual_codes == official_codes).mean(axis=0).tolist()
    result = {"reference_frames": len(actual_codes), "agreement_by_codebook": agreement}
    wav_path.with_suffix(".encoder-comparison.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result), flush=True)
