# Why

Integrate the pinned llama.cpp-omni VoxCPM2 implementation through a narrow cancellable C ABI. Validate real Windows CUDA inference before publishing capabilities.

# What Changes

- Implement Native VoxCPM2 backend.
- Reuse verified downloads, PCM/End, cancellation, model ownership and existing settings.
- Record hardware and listening acceptance separately from compilation.

# Capabilities

## New Capabilities

- `integrate-voxcpm2`: Native VoxCPM2 backend.

## Modified Capabilities

None.

# Impact

Listening worker, backend adapters, lightweight protocol and reader settings; inference dependencies remain outside the reader.
