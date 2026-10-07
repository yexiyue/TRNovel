# Spec Delta

## Purpose

为中文小说听书评估 MOSS 的多种原生模型模式，提供有真实音频证据的模型选择，同时保留现有 Nano 用户配置和音色，确保失败、取消及资源隔离行为一致。

## ADDED Requirements

### Requirement: Verified MOSS model modes
The system SHALL expose a MOSS mode in the reader only after real synthesis, cancellation and subsequent generation succeed. Unverified modes MUST remain experimental and report concrete limitations.

#### Scenario: Candidate verification fails
- **WHEN** a candidate cannot produce verified valid audio or normal termination
- **THEN** the reader model catalog excludes it and the acceptance report identifies the failure

### Requirement: Preserve existing MOSS users
The system SHALL interpret an existing MOSS configuration without a model as Nano and preserve its stored voice and device. New model assets and voice prompts MUST be isolated by model identity and revision.

#### Scenario: Old configuration loads
- **WHEN** an existing Nano user starts the updated worker
- **THEN** the user retains Nano and the existing voice without downloading another model

### Requirement: Native cancellable inference
Published modes MUST run without Python, honor explicit devices, deliver bounded PCM and normal End, and stop delivering audio after cancellation. Truncated or failed synthesis MUST NOT complete a checkpoint.

#### Scenario: Cancel and reuse
- **WHEN** generation is cancelled and another request begins
- **THEN** old audio stops and the next request can complete normally

### Requirement: Acceptance evidence
Each evaluated mode SHALL record immutable resources, licenses, Chinese source text and WAV, generation time, memory and cancellation results. Successful termination MUST NOT be presented as proof of full spoken coverage.

#### Scenario: Listening finds omitted words
- **WHEN** a listener reports missing words despite normal termination
- **THEN** completeness remains unverified and the report records the feedback separately from performance
