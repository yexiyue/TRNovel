"""Development-only FP32 export of the pinned OmniVoice model."""
import argparse
import functools
import pathlib
import shutil
import sys

import torch
from onnx_manifest import require_revision, write_manifest

REVISION = "4ec8125833a6a2806ff2e07b31e73f106954aad0"
PYTHON_REVISION = "08be0b4ccbac3e13e374e86fbfead4b4cac343e2"

p = argparse.ArgumentParser()
p.add_argument("--upstream", type=pathlib.Path, required=True)
p.add_argument("--python-source", type=pathlib.Path, required=True)
p.add_argument("--source", type=pathlib.Path, required=True)
p.add_argument("--output", type=pathlib.Path, required=True)
p.add_argument("--source-revision", required=True)
p.add_argument("--manifest-only", action="store_true")
args = p.parse_args()
require_revision(args.upstream, REVISION)
require_revision(args.python_source, PYTHON_REVISION)
sys.path[:0] = [str(args.upstream.resolve()), str(args.python_source.resolve())]
torch.set_num_threads(4)
original = torch.onnx.export


@functools.wraps(original)
def legacy_export(*a, **kw):
    kw["dynamo"] = False
    return original(*a, **kw)


torch.onnx.export = legacy_export
import _common

_common.PROJECT_ROOT = args.python_source.resolve()
_common.PT_MODEL_DIR = args.source.resolve()
_common.AUDIO_TOKENIZER_DIR = args.source.resolve() / "audio_tokenizer"
_common.OUTPUT_DIR = args.output.resolve()
_common.THIS_DIR = args.output.resolve().parent
for prefix, folder in [("LM", "omnivoice_lm"), ("AT_ENC", "audio_tokenizer_encoder"), ("AT_DEC", "audio_tokenizer_decoder")]:
    directory = _common.OUTPUT_DIR / folder
    directory.mkdir(parents=True, exist_ok=True)
    setattr(_common, f"{prefix}_OUT_DIR", directory)
    setattr(_common, f"{prefix}_ONNX", directory / "model.onnx")
import export_lm
import export_audio_tokenizer

if not args.manifest_only:
    export_lm.export(argparse.Namespace(device="cpu", optimize=False))
    tok = _common.load_audio_tokenizer(torch.device("cpu"))
    export_audio_tokenizer.export_encoder(tok, torch.device("cpu"), False)
    export_audio_tokenizer.export_decoder(tok, torch.device("cpu"), False)
for name in ["config.json", "tokenizer.json"]:
    shutil.copyfile(args.source / name, args.output / name)
write_manifest(args.output, REVISION, args.source_revision,
               dict(script="export_omnivoice_onnx.py", python_revision=PYTHON_REVISION,
                    torch=torch.__version__, device="cpu", dynamo=False, optimize=False, opset=_common.ONNX_OPSET),
               ["config.json", "tokenizer.json", "omnivoice_lm/model.onnx",
                "audio_tokenizer_encoder/model.onnx", "audio_tokenizer_decoder/model.onnx"])
