# Proposal

## Why

MOSS treats layout newlines as synthesis boundaries and may omit speech in long inputs. Always loading Qwen delays preparation and does not repair synthesis completeness.

## What Changes

- Default-off optional sentence alignment in protocol v3, CLI and TUI.
- Merge soft line breaks, preserve source ranges and true paragraph/title boundaries; MOSS target 8 seconds / ceiling 12 seconds.
- Structured backend generation diagnostics; stop on failure without skipping or automatic replay.
- Reproducible local novel/audio comparisons, feature checks and VHS.

## Capabilities

### New Capabilities

- `speech-continuity`: Layout-aware speech blocks and honest synthesis failure handling.
- `optional-alignment`: Explicit alignment preference and zero resource preparation when disabled.

### Modified Capabilities

None. This supersedes the in-flight continuous-tts-alignment-acceleration defaults without archiving its outstanding acceptance.

## Impact

Protocol/config, model-free core, MOSS backend, CLI/worker, reader settings and documentation. ORT and model revisions remain pinned.
