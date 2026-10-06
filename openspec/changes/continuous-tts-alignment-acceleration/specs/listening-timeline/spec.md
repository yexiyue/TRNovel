# Spec Delta

## Purpose

提供保持段落语境的连续播放和基于实际音频位置的句子时间线，使原文坐标、合成文本与可恢复进度在取消和变速时仍保持一致。

## ADDED Requirements

### Requirement: Continuous bounded synthesis

The system SHALL preserve original text mappings, synthesize duration-bounded contextual blocks, trim only boundary silence, and queue playback continuously.

#### Scenario: Long paragraph
- **WHEN** a paragraph exceeds the speech budget
- **THEN** complete sentence boundaries are preferred without losing source text

### Requirement: Asynchronous sentence alignment

The system SHALL align actual processed PCM asynchronously and advance sentence highlighting and checkpoints from actual playback positions.

#### Scenario: Late alignment
- **WHEN** alignment completes after playback has started
- **THEN** only the current position is updated without rewinding highlighting

### Requirement: Alignment failure

The system SHALL retain playback with explicit block highlighting when alignment fails or becomes unavailable.

#### Scenario: Cancelled session
- **WHEN** a session is replaced or stopped
- **THEN** old audio and alignment results cannot alter the new session
