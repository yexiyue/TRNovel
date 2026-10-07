"""Development-only GPT2 reference fixtures from PyTorch eager attention."""

import torch, json
from safetensors.torch import save_file
from pathlib import Path

torch.manual_seed(19)
torch.set_num_threads(1)
d, h, inner, layers = 16, 4, 48, 2
weights = {}
for i in range(layers):
    p = f"h.{i}."
    for n in ["ln_1", "ln_2"]:
        weights[p + n + ".weight"] = torch.ones(d) + torch.randn(d) * 0.05
        weights[p + n + ".bias"] = torch.randn(d) * 0.05
    for n, shape in [
        ("attn.c_attn", (3 * d, d)),
        ("attn.c_proj", (d, d)),
        ("mlp.fc_in", (inner, d)),
        ("mlp.fc_out", (d, inner)),
    ]:
        weights[p + n + ".weight"] = torch.randn(*shape) * 0.1
        weights[p + n + ".bias"] = torch.randn(shape[0]) * 0.05
weights["ln_f.weight"] = torch.ones(d) + torch.randn(d) * 0.05
weights["ln_f.bias"] = torch.randn(d) * 0.05
x = torch.randn(1, 5, d)


def forward(x, pasts=None):
    offset = 0 if pasts is None else pasts[0][0].shape[2]
    positions = torch.arange(offset, offset + x.shape[1])
    inv = 10000 ** (-torch.arange(0, d // h, 2).float() / (d // h))
    cos = torch.outer(positions, inv).cos().repeat_interleave(2, -1)[None, None]
    sin = torch.outer(positions, inv).sin().repeat_interleave(2, -1)[None, None]

    def rope(q):
        even = q[..., ::2]
        odd = q[..., 1::2]
        return q * cos + torch.stack((-odd, even), -1).flatten(-2) * sin

    cache = []
    for i in range(layers):
        p = f"h.{i}."

        def ln(y, n):
            return torch.nn.functional.layer_norm(
                y, (d,), weights[p + n + ".weight"], weights[p + n + ".bias"], 1e-5
            )

        def linear(y, n):
            return torch.nn.functional.linear(
                y, weights[p + n + ".weight"], weights[p + n + ".bias"]
            )

        q, k, v = (
            linear(ln(x, "ln_1"), "attn.c_attn")
            .reshape(1, x.shape[1], 3, h, d // h)
            .permute(2, 0, 3, 1, 4)
        )
        q = rope(q)
        k = rope(k)
        if pasts is not None:
            k = torch.cat([pasts[i][0], k], 2)
            v = torch.cat([pasts[i][1], v], 2)
        cache.append((k, v))
        mask = torch.arange(k.shape[2])[None, :] > positions[:, None]
        scores = (q @ k.transpose(-1, -2)) / (d // h) ** 0.5
        attended = scores.masked_fill(mask, float("-inf")).softmax(-1) @ v
        x = x + linear(
            attended.transpose(1, 2).reshape(1, x.shape[1], d), "attn.c_proj"
        )
        x = x + linear(
            torch.nn.functional.gelu(
                linear(ln(x, "ln_2"), "mlp.fc_in"), approximate="tanh"
            ),
            "mlp.fc_out",
        )
    return torch.nn.functional.layer_norm(
        x, (d,), weights["ln_f.weight"], weights["ln_f.bias"], 1e-5
    ), cache


out, _ = forward(x)
a, cache = forward(x[:, :3])
b, _ = forward(x[:, 3:], cache)
assert torch.max(torch.abs(torch.cat([a, b], 1) - out)) < 1e-6
base = Path("crates/moss-tts/tests/fixtures")
save_file(weights, base / "nano-transformer.safetensors")
(base / "nano-transformer.json").write_text(
    json.dumps(
        {
            "config": {
                "vocab_size": 32,
                "n_embd": d,
                "n_head": h,
                "n_layer": layers,
                "n_inner": inner,
                "layer_norm_epsilon": 1e-5,
                "activation_function": "gelu_new",
                "rope_base": 10000,
            },
            "input": x.flatten().tolist(),
            "output": out.flatten().tolist(),
        },
        indent=2,
    )
    + "\n"
)
