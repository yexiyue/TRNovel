# Spec Delta

## Purpose

提供独立听书程序的可扩展模型后端，支持原生 CPU 流式合成和参考音色管理，使阅读器通过能力协议选择后端而无需加载模型依赖。

## ADDED Requirements

### Requirement: Compile-time backend composition

The system SHALL isolate concrete models in novel-tts-backends, default to MOSS, and retain Kokoro as an optional compile-time feature. The reader SHALL depend only on the lightweight listening protocol.

#### Scenario: MOSS-only distribution
- **WHEN** novel-tts is built with its default features
- **THEN** MOSS is available without Python or Kokoro dependencies

#### Scenario: Existing configuration selects an unavailable backend
- **WHEN** a persisted configuration names a backend absent from the binary
- **THEN** preparation reports an actionable error without silently changing the configuration

### Requirement: Native streaming sessions

The system SHALL synthesize MOSS with CPU ONNX, stream bounded PCM blocks, retain original UTF-8 source ranges, and advance durable checkpoints only after actual playback completes a text segment.

#### Scenario: Incomplete synthesis
- **WHEN** synthesis fails or disconnects before explicit completion
- **THEN** the session fails and the unfinished text remains eligible for retry

#### Scenario: Continuous Chinese narration
- **WHEN** a chapter contains multiple sentences or long Chinese clauses
- **THEN** segmentation prefers sentence boundaries and limits both token count and spoken CJK characters while preserving original source ranges

#### Scenario: Cancellation during generation
- **WHEN** playback is stopped or replaced
- **THEN** old audio and events cannot affect the replacement session and generation observes cancellation between inference steps

### Requirement: Immutable model resources

The system SHALL download pinned model revisions on demand, report bounded progress, support cancellation and resumption, and verify every required file size and SHA-256 before loading.

#### Scenario: Corrupt cache
- **WHEN** a cached resource fails verification
- **THEN** preparation fails with an actionable error and the next attempt can replace the corrupt resource

### Requirement: Reference voice management

The CLI SHALL support voices import/list/remove, accept non-silent mono or stereo WAV references, and atomically cache reusable encoded voices with their model revision. Built-in voices SHALL remain immutable.

#### Scenario: Reusing an imported voice
- **WHEN** a user selects an imported voice
- **THEN** generation loads cached reference codes without decoding and encoding the source WAV again

#### Scenario: Invalid import
- **WHEN** the input is invalid, silent, outside the supported duration, or the ID already exists
- **THEN** import fails without replacing an existing voice

### Requirement: Capability-driven reader selection

Protocol v2 SHALL expose compiled backends and voice names; the TUI SHALL allow backend and voice selection without inference-specific data types.

#### Scenario: Switching a prepared backend
- **WHEN** the user selects another backend
- **THEN** the old session and model are released, the continuation checkpoint is retained, and playback requires explicit preparation and start
