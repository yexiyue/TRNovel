# VoxCPM2 native Candle port

Model semantics: [OpenBMB/VoxCPM](https://github.com/OpenBMB/VoxCPM),
revision `f0c787f0937dc1c9a8f4f64d9a332d9c5da2e629`, Apache-2.0.
GGUF tensor mapping reference: [llama.cpp-omni](https://github.com/tc-mb/llama.cpp-omni),
revision `873056743b74e1a4ce5dcf7290e2298428e214db`, MIT.
This crate independently implements those model operations using workspace Candle
0.11.0 (crates.io source revision `31f35b147389700ed2a178ee66a91c3cc25cc80d`).
It does not link or execute the mapping reference's inference engine.

Weights remain the existing DennisHuang648/VoxCPM2-GGUF resource revision
`169f64d8b98bbaab1761e4ca3a83e6af653456cc`; see the production resource manifest
in `novel-tts-backends` for sizes and SHA-256. The production adapter uses this
Candle library after explicit user authorization to integrate before complete
qualification. Strict numerical failures and outstanding acceptance remain
reported; the old engine is retained only as a development benchmark.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| VoxCPM2-BaseLM-Q8_0.gguf | 1727309920 | 0113177abd11303503bf0b705e1613ec5f0a8508cc74a7dfd0f99312b962a962 |
| VoxCPM2-Acoustic-F16.gguf | 1825096352 | 5bde898488ad635ff55d24da53543768fa33d5e5cdc538ce190e5ef831038e85 |

The weights are derived from Apache-2.0 OpenBMB/VoxCPM2. Their existing default
directory is `~/.novel-tts/voxcpm/models/2b-q8_0/169f64d8b98bbaab1761e4ca3a83e6af653456cc/`.
The caller must verify the resource manifest before opening model files.

Original comparison weights: [openbmb/VoxCPM2](https://huggingface.co/openbmb/VoxCPM2/tree/32279effe8c19989596f05d353d1447f51d9e915),
revision `32279effe8c19989596f05d353d1447f51d9e915`, Apache-2.0.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| model.safetensors | 4580080592 | f7f964cfa9da23653baec6e6f7750719977ad944ed9f95fe52fe3a620506891d |
| audiovae.pth | 376951122 | 94b5d51e107e0507d4acc976cfdadb64edd6fd06d1f751dadbf2fd1594274bf1 |
| tokenizer.json | 3676772 | f8984687e4a92a3503d521396d454b7d68e9fdaab2a0288eb3536c7c1aa4bc20 |
| config.json | 4336 | 405f0dcd92f7feba6011ed4eac5c8d4f74cba9712f07fd5cfa3063bbdd95402c |

`tools/tts/download_voxcpm_official.py` resumes into the revision-scoped
`~/.novel-tts/voxcpm/models/2b-bf16/` directory and verifies LFS SHA-256 or
Git blob identities, recording SHA-256 for every downloaded file. It is a
development tool; no Python download or inference runs in the product.
Original BF16 and F16 comparison runs reuse these same assets. F16 is a
compute precision choice, not a separate upstream model or dequantized Q8.
