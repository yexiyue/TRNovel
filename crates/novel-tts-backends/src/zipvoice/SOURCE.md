# ZipVoice Distill

Inference/frontend reference: https://github.com/k2-fsa/ZipVoice
Revision: 2f7326fbfe999a3ad179e3f1af82a424d4a62819; Apache-2.0, LICENSE.
Weights: https://huggingface.co/k2-fsa/ZipVoice
Revision: 4ed45fb6e7e9527b780bef9e097a04bf13fe4e6b; Apache-2.0.
INT8 and FP32 manifests record fixed URLs, byte sizes and SHA-256.

Vocos: https://huggingface.co/charactr/vocos-mel-24khz (MIT).
Official ONNX export: k2-fsa/sherpa-onnx release vocoder-models,
vocos_24khz.onnx; its exact size/hash are in both manifests.
The export returns magnitude/cosine/sine, requiring centered Hann ISTFT,
not a waveform. Rust mel extraction and ISTFT are tested against Torch.
The audio-golden.json fixture was generated using torch 2.14.1+cpu and
torchaudio 2.11.0+cpu; no Torch dependency exists in release inference.

Chinese dictionaries: jieba 0.42.1 (MIT), pypinyin 0.55.0 (MIT),
cn2an 0.5.24 (MIT), with licenses and hashes under frontend/assets.
tools/tts/generate_zipvoice_frontend.py generates complete word/character
pronunciations using upstream tone sandhi. Unknown HMM words use the same
character pronunciation and tone rules. Explicit pinyin uses the complete
syllable table. This is not a reduced pinyin approximation.
frontend/golden.json contains pinned Emilia cases covering dates, polyphones,
fractions, ordinals, English, mixed language and explicit pinyin.

English uses a separate eSpeak executable and the patched Piper clause API.
See ../../native/zipvoice-phonemizer/SOURCE.md for fixed source, GPL license,
data hashes and distribution obligations. No sherpa inference runtime,
Python, automatic ASR or network phonemizer is used in production.
