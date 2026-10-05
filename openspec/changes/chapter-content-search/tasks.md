## Implementation
- [x] Add ephemeral chapter search state and exclusive one-row draft input.
- [x] Add configurable reader actions, dynamic help and result hints.
- [x] Share plain-text wrapping, range styling and match navigation.
- [x] Clear on chapter/mode changes; preserve ongoing TTS playback.
- [x] Document behavior and keybindings; add regression tests.

## Validation
- [x] Workspace tests, Clippy, formatting and rustdoc (existing CRLF-only schema assertion excluded after confirming the full-command failure).
- [x] Live local reading and offline Fetcher-backed NetworkNovel reading; capture input, matches and no-result terminal screens.
