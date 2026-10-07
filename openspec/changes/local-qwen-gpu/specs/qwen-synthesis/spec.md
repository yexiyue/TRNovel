## ADDED Requirements

### Requirement: Qwen GPU execution
The worker SHALL report CPU and compiled available Candle CUDA or Metal devices for Qwen independently from ORT providers. Automatic selection SHALL require measured efficiency improvements. Explicit accelerator initialization SHALL fail instead of silently returning CPU.

#### Scenario: NVIDIA synthesis
- **WHEN** qwen-cuda is compiled on Windows/Linux and the CUDA device initializes
- **THEN** Qwen synthesis uses that Candle CUDA device and streams PCM through the existing session boundary

#### Scenario: Unsupported provider
- **WHEN** a user selects CoreML or an uncompiled/unavailable accelerator for Qwen
- **THEN** preparation fails with a clear error

### Requirement: Local inference ownership
The workspace SHALL contain the attributed MIT Qwen inference library and use a local path dependency. It SHALL read local model files; verified downloads, configuration, calibration and playback SHALL remain outside the inference library. The tokenizer SHALL NOT enable training-only C++ acceleration by default.

#### Scenario: Windows mixed inference build
- **WHEN** the worker builds Qwen alongside ORT CUDA
- **THEN** it links without requiring a user-supplied C++ CRT override
