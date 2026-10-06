## Purpose

在独立听书程序中提供可选择的原生 Rust Qwen 语音合成后端，复用现有会话与播放职责，并明确报告模型资源、音色及实际执行设备，便于用户比较不同模型的朗读效果。

## ADDED Requirements

### Requirement: Optional Qwen backend
The worker SHALL expose Qwen only when compiled and SHALL expose nine preset voices with a valid default. The reader SHALL remain independent of Candle.

#### Scenario: Select Qwen
- **WHEN** a user selects the compiled qwen backend
- **THEN** the worker selects its default voice, stops the old session and requires explicit playback restart

### Requirement: Verified model resources
The backend SHALL download fixed revision model resources on demand and verify their sizes and SHA-256 hashes.

#### Scenario: Corrupt resources
- **WHEN** a downloaded file fails integrity verification
- **THEN** preparation fails with an error instead of loading that file

### Requirement: Honest generation completion
The adapter SHALL emit completion only after model EOS and valid PCM. Cancellation and frame exhaustion SHALL not commit the current segment checkpoint.

#### Scenario: Frame exhaustion
- **WHEN** the model reaches the configured frame limit without EOS
- **THEN** playback stops with an incomplete segment error and preserves the last reliable checkpoint

### Requirement: Backend-specific devices
The worker SHALL report CPU and compiled available Metal devices for Qwen, independently from ORT alignment providers. Automatic selection SHALL require measured efficiency improvements.

#### Scenario: Unsupported provider
- **WHEN** a user explicitly selects CoreML or CUDA for Qwen
- **THEN** the worker rejects that selection with a clear error

### Requirement: Preserve source text
The backend SHALL skip decoration lines, retain operators in prose and keep UTF-8 source coordinates while merging soft layout lines for synthesis.

#### Scenario: Soft Chinese line break
- **WHEN** Chinese prose wraps at a single newline
- **THEN** synthesis joins that layout boundary and playback events still refer to the original source range
