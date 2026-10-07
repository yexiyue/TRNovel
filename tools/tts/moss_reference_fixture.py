"""Development-only upstream numerical fixtures; inference never invokes Python."""
import json
import sys
import importlib.util
from pathlib import Path
import torch
from safetensors.torch import save_file
from transformers import Qwen3Config
from transformers.models.qwen3 import Qwen3Model

root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / 'target/tts-integration/moss-upstream'))
sys.path.insert(0, str(root / 'target/tts-integration/moss-codec-upstream'))
from moss_tts_local.modeling_moss_tts import MossTTSLocalTransformer
codec_root=root/'target/tts-integration/moss-codec-upstream'
spec=importlib.util.spec_from_file_location('moss_codec_upstream',codec_root/'__init__.py',submodule_search_locations=[str(codec_root)])
package=importlib.util.module_from_spec(spec)
sys.modules[spec.name]=package
spec.loader.exec_module(package)
from moss_codec_upstream.modeling_moss_audio_tokenizer import MossAudioTokenizerProjectedTransformer

torch.set_num_threads(2)
torch.manual_seed(734)
folder = root / 'crates/moss-tts/tests/fixtures'
folder.mkdir(parents=True, exist_ok=True)
config = Qwen3Config(vocab_size=16, hidden_size=8, intermediate_size=12,
    num_hidden_layers=2, num_attention_heads=2, num_key_value_heads=1, head_dim=4,
    max_position_embeddings=128, rms_norm_eps=1e-6, rope_theta=10000, attention_bias=False)
config._attn_implementation = 'sdpa'
global_model = Qwen3Model(config).eval()
local_model = MossTTSLocalTransformer(config).eval()
codec_config = dict(module_type='Transformer', input_dimension=4, output_dimension=6,
    d_model=8, num_heads=2, num_layers=2, dim_feedforward=12, causal=True,
    context=8, norm='layer_norm', positional_embedding='rope', max_period=10000,
    gating='none', layer_scale=0.01, conv_layout=True)
codec = MossAudioTokenizerProjectedTransformer(**codec_config).eval()
x = torch.randn(1, 5, 8)
cx = torch.randn(1, 4, 5)
long_cx = torch.randn(1, 4, 21)
with torch.no_grad():
    global_y = global_model(inputs_embeds=x).last_hidden_state
    local_y = local_model(inputs_embeds=x).last_hidden_state
    full_codec = codec(cx, torch.tensor([5]))[0]
    with codec.streaming(1):
        chunk1 = codec(cx[:, :, :2], torch.tensor([2]))[0]
        chunk2 = codec(cx[:, :, 2:], torch.tensor([3]))[0]
    assert torch.allclose(full_codec, torch.cat([chunk1, chunk2], dim=2), atol=1e-6)
    with codec.streaming(1):
        wrapped = torch.cat([
            codec(long_cx[:, :, start:start+5], torch.tensor([min(5,21-start)]))[0]
            for start in range(0,21,5)
        ], dim=2)
state = {}
for prefix, model in [('global', global_model), ('local', local_model), ('codec', codec)]:
    state.update({f'{prefix}.{name}': value.contiguous() for name, value in model.state_dict().items()})
save_file(state, str(folder / 'upstream.safetensors'))
payload = dict(config={**config.to_dict(), 'rope_theta': 10000.}, codec_config=codec_config,
    input=x.flatten().tolist(), global_output=global_y.flatten().tolist(),
    local_output=local_y.flatten().tolist(), codec_input=cx.flatten().tolist(),
    codec_output=full_codec.flatten().tolist(),
    codec_wrap_input=long_cx.flatten().tolist(), codec_wrap_output=wrapped.flatten().tolist())
(folder / 'upstream.json').write_text(json.dumps(payload, indent=2), encoding='utf-8')
print('Upstream numerical fixtures written:', folder)
