# VoxCPM2 inference sources

Upstream: https://github.com/tc-mb/llama.cpp-omni
Revision: 873056743b74e1a4ce5dcf7290e2298428e214db

Included: ggml, llama, common and VoxCPM2 inference code with their licenses.
Excluded: model converters, Python, CLI, server, tests, examples, UI and other Omni models.
Local patches: expose the selected acoustic backend name to reject explicit GPU fallback;
emit a final callback only on predictor EOS, so frame limits remain incomplete;
accept cached reference features using the unchanged upstream continuation prefill.
CPU selection disables both KV and operation offloading in the BaseLM context.
The wrapper selects CPU/CUDA/Metal explicitly and uses the dynamic MSVC CRT.

Weights: DennisHuang648/VoxCPM2-GGUF revision 169f64d8b98bbaab1761e4ca3a83e6af653456cc,
Q8_0 BaseLM + F16 Acoustic, derived from Apache-2.0 OpenBMB/VoxCPM2.
This GGUF repository and inference engine are linked by the official OpenBMB README.
