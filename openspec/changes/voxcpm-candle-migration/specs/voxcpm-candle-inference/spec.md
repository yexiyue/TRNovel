# Spec Delta

## Purpose

Provide native VoxCPM2 speech generation with reusable voices and reliable streaming cancellation, preserving existing local resources and validating performance before deployment.

## ADDED Requirements

### Requirement: Explicit original-weight experiment
The reader SHALL expose voxcpm/2b-bf16 as a labelled CUDA experiment when CUDA is compiled, preserve the existing Q8 default, and isolate resources, voices, reference encodings and calibration by model identity. This option SHALL NOT imply numerical, listening or long-term qualification. All Candle backends SHALL share the pinned workspace stable version.

#### Scenario: Select the experiment from the reader
- **WHEN** the user switches from Q8 to original BF16
- **THEN** the old model is released, the reader shows the experimental status, and an incompatible CPU request is replaced by Auto before preparation

#### Scenario: Candle runtime is upgraded
- **WHEN** the unified Candle dependency changes
- **THEN** performance calibration and incompatible encoded references are invalidated without redownloading unchanged weights

### Requirement: Compatible model and voice identity
The system SHALL reuse the existing voxcpm/2b-q8_0 resources and stored reference voices without redownloading weights or changing existing user preferences. Incompatible encoded references MUST be rebuilt from their WAV.

#### Scenario: Existing voice is used after migration
- **WHEN** an existing reference voice is selected
- **THEN** its WAV and text remain usable and its encoding is validated or rebuilt

### Requirement: Cancellable native streaming
Generation SHALL produce 48kHz PCM incrementally without runtime Python or llama.cpp. Cancellation MUST stop audio delivery, and only valid PCM followed by normal EOS SHALL complete a segment.

#### Scenario: Cancel then generate again
- **WHEN** generation is cancelled and another request starts
- **THEN** no old audio is delivered and the next request can complete normally

#### Scenario: Frame limit
- **WHEN** generation reaches its limit before EOS
- **THEN** it reports truncation and does not complete the checkpoint

### Requirement: Performance qualification
Full qualification MUST pass RTX 5070 release testing: five fixed-corpus and long-paragraph runs each with RTF <= 0.8, hot first PCM <= 1.5 seconds, and 30-minute playback without failure, underruns or continuing memory growth. Missing platform and listening acceptance SHALL be reported explicitly.

#### Scenario: Candidate misses a gate
- **WHEN** the candidate fails correctness or performance qualification
- **THEN** the current production implementation remains unless the user explicitly authorizes staged integration; concrete blockers remain recorded

### Requirement: Numerical qualification
The implementation SHALL compare fixed inputs against the pinned official model at F32 atol=1e-5 and rtol=1e-4, including full/incremental Transformers, FSQ, fixed-noise CFM, reference encoding and full/chunked AudioVAE. Quantized and F16 errors SHALL be reported separately. Failed comparisons MUST NOT be hidden by relaxing the F32 threshold.

#### Scenario: Transformer comparison fails
- **WHEN** any required F32 output exceeds the stated tolerance
- **THEN** numerical qualification remains incomplete; production migration is gated unless the user explicitly accepts staged integration

### Requirement: Explicit staged rollout exception
The system MUST allow the user-authorized Candle production adapter switch before final qualification, while retaining strict numerical failure reports and pending playback, listening and platform acceptance items. This exception supersedes any pre-switch gating above and MUST NOT be reported as a numerical or full quality pass.

#### Scenario: User accepts current numerical differences
- **WHEN** the user explicitly requests integration with the measured differences
- **THEN** the worker uses Candle and rebuilds model-scoped reference caches from WAV
- **AND** all unmet acceptance gates remain recorded as unmet
