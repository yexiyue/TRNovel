"""Export pinned local Qwen CustomVoice weights for the Rust experimental adapter.

Requires the pinned upstream checkout passed in --upstream. Python is only a
development/export dependency; the installed worker uses ORT directly.
"""
import argparse
import functools
import inspect
import pathlib
import shutil
import sys

import torch
from onnx_manifest import require_revision, write_manifest

REVISION = "5d1687f02855ee403792ed145fabc936f62b60f8"

parser = argparse.ArgumentParser()
parser.add_argument("--upstream", type=pathlib.Path, required=True)
parser.add_argument("--source", type=pathlib.Path, required=True)
parser.add_argument("--output", type=pathlib.Path, required=True)
parser.add_argument("--components", default="all")
parser.add_argument("--source-revision", required=True)
parser.add_argument("--manifest-only", action="store_true")
parser.add_argument("--greedy-control", action="store_true", help="Export only a separate greedy residual graph for matched Candle comparisons")
args = parser.parse_args()
require_revision(args.upstream, REVISION)
sys.path.insert(0, str(args.upstream.resolve()))
torch.set_num_threads(4)
original_export = torch.onnx.export


@functools.wraps(original_export)
def legacy_export(*a, **kw):
    # New torch defaults to dynamo; upstream wrappers target the legacy tracer.
    kw["dynamo"] = False
    if "dynamic_shapes" in kw:
        kw.pop("dynamic_shapes")
        kw["dynamic_axes"] = {"token_ids": {1: "seq_len"}, "ref_code": {1: "num_frames"}}
    return original_export(*a, **kw)


torch.onnx.export = legacy_export
# Torch 2.5 shape inference serializes large FP32 initializers twice and hits
# protobuf's 2GB cap before external-data serialization. ORT infers shapes when
# loading the completed graph; do not swallow other inference failures.
original_inference = torch._C._jit_pass_onnx_graph_shape_type_inference


def infer_shapes(*a, **kw):
    try:
        return original_inference(*a, **kw)
    except RuntimeError as error:
        if "serialized model is larger than the 2GiB" not in str(error):
            raise


torch._C._jit_pass_onnx_graph_shape_type_inference = infer_shapes
import export_onnx
import export.talker_core_export as talker_export

# Upstream's batch-one optimization assumes attention width == hidden size,
# which is false for 0.6B (16 heads * 128 != 1024). Preserve the o_proj width.
function = inspect.getsource(talker_export._batch1_shape_optimized_attention_forward)
function = function.replace("hidden_size = int(self.config.hidden_size)", "hidden_size = int(self.o_proj.in_features)")
exec(function, talker_export.__dict__)

args.output.mkdir(parents=True, exist_ok=True)
sys.argv = ["export_onnx", "--model-path", str(args.source), "--output-dir", str(args.output / ("onnx-greedy" if args.greedy_control else "onnx")),
            "--dtype", "fp32", "--device", "cpu", "--components",
            "sub_talker_sample" if args.greedy_control else args.components, "--verify"]
if args.greedy_control:
    sys.argv.append("--no-decode-residual-do-sample")
# Tokenizer encoder parity uses a supplied real WAV, not upstream's missing asset.
wav = pathlib.Path("target/tts-integration/onnx-reference.wav")
if wav.exists():
    sys.argv.extend(["--audio-path", str(wav.resolve())])
if not args.manifest_only:
    export_onnx.main()
from transformers import AutoTokenizer

AutoTokenizer.from_pretrained(args.source, local_files_only=True, fix_mistral_regex=True).backend_tokenizer.save(str(args.output / "tokenizer.json"))
for file in ["config.json", "generation_config.json"]:
    shutil.copyfile(args.source / file, args.output / file)
write_manifest(args.output, REVISION, args.source_revision,
               dict(script="export_qwen_onnx.py", torch=torch.__version__, device="cpu", dynamo=False,
                    attention_width_fix=True, tokenizer_fix_mistral_regex=True,
                    residual_sampling=True, residual_top_k=50, residual_temperature=0.9,
                    greedy_control=(args.output / "onnx-greedy/decode/sub_talker_sample.onnx").is_file()),
               ["tokenizer.json", "config.json", "generation_config.json",
                "onnx/text_project/text_project.onnx", "onnx/codec_embed/codec_embed.onnx",
                "onnx/talker/talker_core.onnx", "onnx/decode/sub_talker_sample.onnx",
                "onnx/tokenizer/tokenizer12hz_decode_chunk.onnx"])
