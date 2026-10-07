# Why

Integrate model selection, Qwen 1.7B CustomVoice, Base cloning and VoiceDesign. Preserve existing user configurations and validate real CUDA generation before enabling a new default.

# What Changes

- Implement Qwen GPU model tiers and reusable voices.
- Reuse verified downloads, PCM/End, cancellation, model ownership and existing settings.
- Record hardware and listening acceptance separately from compilation.

# Capabilities

## New Capabilities

- `qwen-model-tiers`: Qwen GPU model tiers and reusable voices.

## Modified Capabilities

None.

# Impact

Listening worker, backend adapters, lightweight protocol and reader settings; inference dependencies remain outside the reader.
