# Qwen GPU model tiers and reusable voices

## ADDED Requirements

### Requirement: Validated native synthesis
The system SHALL provide qwen gpu model tiers and reusable voices with verified resources, bounded PCM delivery and explicit successful completion.

#### Scenario: Successful synthesis
- **WHEN** a supported model, voice and device are selected
- **THEN** the worker emits valid PCM and publishes completion only after normal model termination

#### Scenario: Cancelled synthesis
- **WHEN** the consumer cancels synthesis
- **THEN** the producer stops delivering audio and the incomplete segment does not advance the checkpoint

#### Scenario: Unsupported selection
- **WHEN** the model or explicit device is unavailable
- **THEN** preparation fails with an actionable error without silently substituting another model or device

### Requirement: Model-aware defaults and immutable voice identity
The system SHALL select defaults only for absent configuration, persist them on first activation, and preserve existing backend, voice, model and device preferences. Old Qwen configuration without a model SHALL resolve to 0.6B-CustomVoice.

#### Scenario: New GPU user
- **WHEN** the user first prepares listening with CUDA or Metal available and Qwen compiled
- **THEN** 1.7B-CustomVoice and that device are saved, and only that model is downloaded

#### Scenario: CPU or cropped build
- **WHEN** no supported GPU is available
- **THEN** the compiled default order is CPU MOSS, Kokoro, Qwen 0.6B; missing stable backends require explicit selection

#### Scenario: Model change
- **WHEN** a user changes a prepared model
- **THEN** the old session and model are released before loading the new model, and voices and capabilities follow the selected model

#### Scenario: Designed reference voice
- **WHEN** VoiceDesign creates a reference from a description
- **THEN** the saved voice includes its reference transcript and identity and subsequent narration uses Base's reusable clone prompt

#### Scenario: Incorrect reference or incompatible style
- **WHEN** the reference is invalid or a requested voice/style is incompatible
- **THEN** the operation returns a clear error and does not silently use a different voice or model
