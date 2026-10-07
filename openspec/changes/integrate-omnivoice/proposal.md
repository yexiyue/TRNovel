# Why

Adapt the pinned inference-only Rust implementation to workspace Candle without private forks, FlashAttention or runtime Python. Validate upstream parity and cancellable segmented generation.

# What Changes

- Implement Native OmniVoice backend.
- Reuse verified downloads, PCM/End, cancellation, model ownership and existing settings.
- Record hardware and listening acceptance separately from compilation.

# Capabilities

## New Capabilities

- `integrate-omnivoice`: Native OmniVoice backend.

## Modified Capabilities

None.

# Impact

Listening worker, backend adapters, lightweight protocol and reader settings; inference dependencies remain outside the reader.
