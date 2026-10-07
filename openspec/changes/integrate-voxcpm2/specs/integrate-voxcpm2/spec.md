# Native VoxCPM2 backend

## ADDED Requirements

### Requirement: Validated native synthesis
The system SHALL provide native voxcpm2 backend with verified resources, bounded PCM delivery and explicit successful completion.

#### Scenario: Successful synthesis
- **WHEN** a supported model, voice and device are selected
- **THEN** the worker emits valid PCM and publishes completion only after normal model termination

#### Scenario: Cancelled synthesis
- **WHEN** the consumer cancels synthesis
- **THEN** the producer stops delivering audio and the incomplete segment does not advance the checkpoint

#### Scenario: Unsupported selection
- **WHEN** the model or explicit device is unavailable
- **THEN** preparation fails with an actionable error without silently substituting another model or device
