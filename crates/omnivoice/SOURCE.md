# OmniVoice native inference

Source: https://github.com/FerrisMind/omnivoice-rs
Revision: 4c7b088294fd6f3a5fb9e3b1dfd3a77bc12f105b
Code license: Apache-2.0 (LICENSE).

Only the inference library and manifest assets are included. No CLI, server,
model downloader, private Candle fork, FlashAttention, Vulkan/WGPU or automatic
ASR dependency remains. All inference uses workspace Candle 0.9.2.
Reference transcripts are required; optional quantized embedding lookup uses
standard Candle dequantization. Debug/reference comparison helpers remain to
validate parity. Timing accumulation is disabled unless explicitly requested.

Weights: k2-fsa/OmniVoice c5fdb5ccb189668d56333f77ba2629f4cd7535f4.
The generator is Apache-2.0. The audio tokenizer has the separate BOSON HIGGS
AUDIO 2 COMMUNITY LICENSE AGREEMENT incorporating Llama 3 terms:
https://huggingface.co/k2-fsa/OmniVoice/blob/c5fdb5ccb189668d56333f77ba2629f4cd7535f4/audio_tokenizer/LICENSE
Do not describe the complete weight bundle as Apache-2.0.
