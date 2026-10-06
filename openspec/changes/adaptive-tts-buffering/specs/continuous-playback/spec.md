## ADDED Requirements

### Requirement: Playback buffers before starting and after underruns
The core SHALL use bounded, speed-aware buffering and SHALL preserve explicit user pause and actual-playback checkpoints.

#### Scenario: Initial audio arrives
- **WHEN** generated audio is below the startup target and synthesis is unfinished
- **THEN** playback remains paused and reports buffering without advancing source progress

#### Scenario: Queue underrun
- **WHEN** playback consumes all queued audio before synthesis finishes
- **THEN** the core pauses and increases its recovery target instead of playing each arriving chunk immediately

#### Scenario: User pause and short final audio
- **WHEN** synthesis finishes with less audio than the target
- **THEN** remaining audio plays unless the user explicitly paused

### Requirement: Buffering and generation have observable diagnostics
The worker SHALL report session-isolated buffer duration, target and underruns. Qwen SHALL support optional per-chunk inference diagnostics without changing generation semantics.

#### Scenario: Buffer status arrives
- **WHEN** a buffer status belongs to an old session
- **THEN** the reader ignores it
