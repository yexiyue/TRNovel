> 已撤回（2026-10-07）：按用户决定移除 ZipVoice，CPU 默认仅保留 MOSS Nano。以下内容为历史方案，不再要求实现或验收。

# ZipVoice Distill CPU backend

## ADDED Requirements

### Requirement: Validated native synthesis
The system SHALL provide zipvoice distill cpu backend with verified resources, bounded PCM delivery and explicit successful completion.

#### Scenario: Successful synthesis
- **WHEN** a supported model, voice and device are selected
- **THEN** the worker emits valid PCM and publishes completion only after normal model termination

#### Scenario: Cancelled synthesis
- **WHEN** the consumer cancels synthesis
- **THEN** the producer stops delivering audio and the incomplete segment does not advance the checkpoint

#### Scenario: Unsupported selection
- **WHEN** the model or explicit device is unavailable
- **THEN** preparation fails with an actionable error without silently substituting another model or device
