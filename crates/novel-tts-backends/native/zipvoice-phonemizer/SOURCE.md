# ZipVoice English phonemizer

This standalone GPL-3.0-or-later executable uses the exact patched eSpeak
TextToPhonemesWithTerminator API used by upstream piper-phonemize. It accepts
UTF-8 through stdin and returns IPA clauses and terminators through stdout.
It does not link into the Rust worker or introduce another inference runtime.

eSpeak source: https://github.com/rhasspy/espeak-ng
Revision: 0f65aa301e0d6bae5e172cc74197d32a6182200f
License: espeak/COPYING (GPL-3.0-or-later).
Piper semantics: https://github.com/rhasspy/piper-phonemize
Revision: ba3cc06c5248215928821f1393b2b854a936991a (MIT wrapper).

Data files are from the official k2-fsa ZipVoice Distill INT8 model package:
https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-zipvoice-distill-int8-zh-en-emilia.tar.bz2
Every extracted data file is pinned in data-manifest.json. The combined
artifact espeak-data.bin uses count, name length, byte length, name and data
in little-endian order; it contains no model weights.

Distribute the helper's GPL license and matching source alongside releases.
The Chinese frontend uses upstream Jieba/Pypinyin/Cn2An dictionaries and
normalization, with separate source/asset licenses.
