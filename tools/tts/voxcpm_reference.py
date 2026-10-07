"""Development-only numerical oracle using the pinned official Python modules.

Runtime Rust inference never invokes this script. Install torch, numpy, gguf,
pydantic and einops in a development environment. Pass the pinned source checkout
and existing GGUF directory; outputs contain tensors, not redistributed weights.
"""
import argparse
import gc
import importlib
import json
import sys
import types
from pathlib import Path

import numpy as np
import torch


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--gguf-python", type=Path, required=True)
    p.add_argument("--models", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--only-cfm", action="store_true")
    p.add_argument("--original-models", type=Path,
                   help="Use original Safetensors and AudioVAE, instead of dequantized GGUF")
    args = p.parse_args()
    sys.path.insert(0, str(args.gguf_python))
    from gguf import GGUFReader
    from gguf.quants import dequantize

    # Import only model modules, without the official CLI/ASR/download package.
    package = types.ModuleType("voxcpm")
    package.__path__ = [str(args.source / "src" / "voxcpm")]
    sys.modules["voxcpm"] = package
    from voxcpm.modules.minicpm4 import MiniCPM4Config, MiniCPMModel
    torch.set_num_threads(2)
    torch.manual_seed(42)
    args.output.mkdir(parents=True, exist_ok=True)
    if args.original_models:
        from safetensors import safe_open
        converter_path = args.gguf_python.parent / "tools/omni/voxcpm2/convert_voxcpm2_to_gguf.py"
        spec = importlib.util.spec_from_file_location("voxcpm_converter", converter_path)
        converter = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(converter)
        aliases = {}
        for mapping in (converter.BASE_LM_GGUF_TENSOR_MAP, converter.ACOUSTIC_GGUF_PREFIX_MAP):
            for original_name, alias in mapping.items():
                for i in range(28) if "{i}" in original_name else [0]:
                    aliases[alias.format(i=i)] = original_name.format(i=i)
        original_tensors = safe_open(args.original_models / "model.safetensors", framework="pt", device="cpu")
        vae_state = torch.load(args.original_models / "audiovae.pth", map_location="cpu", weights_only=True)["state_dict"]
        folded = {}
        for key, value in vae_state.items():
            if key.endswith(".weight_v"):
                stem = key.removesuffix(".weight_v")
                folded["audio_vae." + stem] = torch._weight_norm(value, vae_state[stem + ".weight_g"], 0)
            elif not key.endswith(".weight_g"):
                folded["audio_vae." + key] = value
        config_data = json.loads((args.original_models / "config.json").read_text(encoding="utf-8"))
        folded["rope_factors_short.weight"] = torch.tensor(config_data["lm_config"]["rope_scaling"]["short_factor"])
        tensors = dict.fromkeys(list(aliases) + list(folded))
    else:
        readers = {name: GGUFReader(str(args.models / f"VoxCPM2-{name}.gguf"))
                   for name in ("BaseLM-Q8_0", "Acoustic-F16")}
        tensors = {t.name: t for r in readers.values() for t in r.tensors}

    def weight(name):
        if args.original_models:
            return folded[name].float() if name in folded else original_tensors.get_tensor(aliases[name]).float()
        t = tensors[name]
        return torch.from_numpy(np.array(dequantize(t.data, t.tensor_type), copy=True)
                                .reshape(tuple(reversed(t.shape)))).float()

    def dump(name, **values):
        data = {key: {"shape": list(v.shape), "data": v.detach().flatten().tolist()}
                for key, v in values.items()}
        (args.output / f"{name}.json").write_text(json.dumps(data), encoding="utf-8")
        print(name, flush=True)

    rope = weight("rope_factors_short.weight").tolist()

    def config(hidden, middle, layers, no_rope=False):
        return MiniCPM4Config(bos_token_id=1, eos_token_id=73440, hidden_size=hidden,
                             intermediate_size=middle, num_hidden_layers=layers,
                             num_attention_heads=16, num_key_value_heads=2,
                             kv_channels=128, max_position_embeddings=32768,
                             rms_norm_eps=1e-5, vocab_size=0, use_mup=False,
                             scale_emb=1, scale_depth=1, dim_model_base=hidden,
                             rope_theta=10000, no_rope=no_rope,
                             rope_scaling={"type": "longrope", "long_factor": rope,
                                           "short_factor": rope,
                                           "original_max_position_embeddings": 32768})

    def transformer(prefix, cfg, norm):
        # Construction on CPU retains the official RoPE caches; skip unused
        # random parameter initialization because every parameter is replaced.
        with torch.device("meta"):
            m = MiniCPMModel(cfg)
        m.to_empty(device="cpu")
        if m.rope_emb is not None:
            m.rope_emb.inv_freq = 1.0 / (10000 ** (torch.arange(0,128,2).float()/128))
            m.rope_emb._set_cos_sin_cache(32768, "cpu", torch.float32)
        mapping = {"input_layernorm": "attn_norm", "post_attention_layernorm": "ffn_norm",
                   "self_attn.q_proj": "attn_q", "self_attn.k_proj": "attn_k",
                   "self_attn.v_proj": "attn_v", "self_attn.o_proj": "attn_output",
                   "mlp.gate_proj": "ffn_gate", "mlp.up_proj": "ffn_up", "mlp.down_proj": "ffn_down"}
        state = {"norm.weight": weight(norm)}
        for i in range(cfg.num_hidden_layers):
            for dest, source in mapping.items():
                state[f"layers.{i}.{dest}.weight"] = weight(f"{prefix}blk.{i}.{source}.weight")
        m.load_state_dict(state)
        return m.eval()

    with torch.no_grad():
        if not args.only_cfm:
            for prefix, hidden, middle, count, norm, causal, name in (
                    ("",2048,6144,28,"output_norm.weight",True,"base"),
                    ("residual_lm.",2048,6144,8,"residual_lm.output_norm.weight",True,"residual"),
                    ("locenc.",1024,4096,12,"locenc.norm.weight",False,"local")):
                m = transformer(prefix, config(hidden,middle,count,name=="residual"),norm)
                x = torch.randn(1,3,hidden)*0.1
                y,_ = m(x,is_causal=causal)
                dump(name,x=x,y=y)
                del m;gc.collect()
    
            m = transformer("locenc.",config(1024,4096,12),"locenc.norm.weight")
            x = torch.randn(2,4,64)*0.1
            projected = torch.nn.functional.linear(x,weight("locenc.in_proj.weight"),weight("locenc.in_proj.bias"))
            cls = weight("locenc.cls_token.weight").reshape(1,1,1024).expand(2,1,1024)
            encoded,_=m(torch.cat([cls,projected],dim=1),is_causal=False)
            y=torch.nn.functional.linear(encoded[:,0,:],weight("projections.enc_to_lm_proj.weight"),weight("projections.enc_to_lm_proj.bias"))
            dump("encoder",x=x,y=y)
            del m;gc.collect()
    
            x=torch.randn(1,2048)*0.1
            h=torch.nn.functional.linear(x,weight("fsq.in_proj.weight"),weight("fsq.in_proj.bias"))
            y=torch.nn.functional.linear(torch.round(torch.tanh(h)*9)/9,weight("fsq.out_proj.weight"),weight("fsq.out_proj.bias"))
            dump("fsq",x=x,y=y)
    
            module=importlib.import_module("voxcpm.modules.audiovae.audio_vae_v2")
            with torch.device("meta"):
                vae=module.AudioVAE(module.AudioVAEConfig())
            # GGUF already folds weight normalization. Remove the hooks and replace
            # the plain tensors, including sample-rate conditioning embeddings.
            for sub in vae.modules():
                if hasattr(sub,"weight_g"):
                    torch.nn.utils.remove_weight_norm(sub)
            vae.to_empty(device="cpu")
            state={}
            for key in vae.state_dict():
                candidate="audio_vae."+key
                if candidate not in tensors and candidate.endswith(".weight"):
                    candidate=candidate[:-7]
                if candidate not in tensors and ".sr_cond_model." in candidate:
                    candidate=candidate.removesuffix(".weight")
                state[key]=weight(candidate)
            vae.load_state_dict(state);vae.eval()
            x=torch.randn(1,1,5120)*0.03
            dump("vae-encode",x=x,y=vae.encode(x,16000))
            x=torch.randn(1,64,8)*0.1
            dump("vae-decode",x=x,y=vae.decode(x))
            del vae;gc.collect()

        from voxcpm.modules.locdit.local_dit_v2 import VoxCPMLocDiT
        from voxcpm.modules.locdit.unified_cfm import UnifiedCFM, CfmConfig
        with torch.device("meta"):
            dit=VoxCPMLocDiT(config(1024,4096,12))
        dit.to_empty(device="cpu")
        dit.decoder=transformer("locdit.",config(1024,4096,12),"locdit.norm.weight")
        state=dit.state_dict()
        for key in state:
            if not key.startswith("decoder."):
                state[key]=weight("locdit."+key)
        dit.load_state_dict(state);dit.eval()
        x=torch.randn(1,64,4)*0.1
        mu=torch.randn(1,2048)*0.1
        cond=torch.randn(1,64,4)*0.1
        paired_mu=torch.cat([mu,torch.zeros_like(mu)])
        paired_x=x.repeat(2,1,1);paired_cond=cond.repeat(2,1,1)
        time=dit.time_mlp(dit.time_embeddings(torch.tensor([0.75,0.75])))+dit.delta_time_mlp(dit.time_embeddings(torch.zeros(2)))
        tokens=torch.cat([paired_mu.reshape(2,2,1024),time.unsqueeze(1),dit.cond_proj(paired_cond.transpose(1,2)),dit.in_proj(paired_x.transpose(1,2))],1)
        decoded,_=dit.decoder(tokens,is_causal=False)
        dump("dit-transformer",x=tokens,y=decoded)
        y=dit(paired_x,paired_mu,torch.tensor([0.75,0.75]),paired_cond,torch.zeros(2))
        dump("dit",x=paired_x,mu=paired_mu,cond=paired_cond,y=y)
        solver=UnifiedCFM(64,CfmConfig(),dit)
        grid=torch.linspace(1,0,11)
        grid=grid+torch.cos(torch.pi/2*grid)-1+grid
        y=solver.solve_euler(x,grid,mu,cond,cfg_value=2.0,use_cfg_zero_star=True)
        dump("cfm",x=x,mu=mu,cond=cond,y=y)



if __name__ == "__main__":
    main()
