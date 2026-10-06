# Design

## Context

Current MOSS ends every input line independently and normal EOS provides no proof of spoken coverage. Alignment preparation is unconditional.

## Goals / Non-Goals

Goals: layout-aware source-preserving blocks, default-off alignment, observable generation termination and no silent continuation after failure.
Non-Goals: ASR, automatic replay, cross-request KV reuse, synthetic sentence timestamps or changing Kokoro segmentation.

## Decisions

- Add alignment_enabled=false in protocol v3 config/patch. CLI --alignment[=true|false], TUI switch; worker stops/unloads on changes. Disabled preference bypasses every alignment resource and calibration call.
- Reuse built-in TOC heading patterns as lightweight shared constants; recognize short standalone headings and avoid sentence-ending prose. Single newlines are soft; blank lines, headings and separators hard. Normalization joins Chinese soft breaks without spaces and preserves Latin word spacing.
- MOSS target 8 seconds/cap 12, 50 normalized tokens/60 CJK chars/375 frames. Prefer sentence/clause/character boundaries and retain closing quotes with preceding punctuation where budgets permit. Source slices remain unmodified.
- Backend determines paragraph endings separately from newline spelling. Core keeps bounded streaming PCM and played-only checkpoints.
- Generation reports typed EOS/frame-limit/cancel/inference-failure diagnostics via an opt-in backend observer. No diagnostics enter model-free core. Actual source and playback events are collected by an offline/worker probe.
- Normal EOS remains a model assertion, not speech coverage verification. Short duration is diagnostic only. No retry or ASR expansion.

## Risks / Trade-offs

Smaller blocks can add prosody resets; merging soft lines and fixed-input listening comparisons quantify the compromise. True blank-line-separated paragraphs remain independent. Missing speech despite EOS requires reference/audio evidence; no heuristic may claim verification.

## Migration

Existing configs without alignment_enabled disable alignment; retain resources and protocol major v3. Reader and worker shipped together. Existing GPU acceptance gaps remain tracked separately.
