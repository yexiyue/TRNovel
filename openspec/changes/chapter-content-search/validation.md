# Validation

- `cargo check --locked --all-features --workspace`: passed.
- `cargo build --locked --bin trnovel`: passed.
- `cargo clippy --all-targets --all-features --workspace -- -D warnings`: passed.
- `cargo fmt --all --check`: passed.
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --all-features --workspace --examples`: passed.
- Full prescribed workspace tests failed at the pre-existing `source::schema_sync::schema_is_in_sync` assertion on Windows. Generated LF text and checked-in CRLF text are identical after newline normalization. No schema or source types changed.
- Re-running the prescribed workspace test command with `-- --skip schema_is_in_sync`: 196 passed, 1 excluded. Includes six new tests for input, literal matches, navigation, wrapping, styling and configurable bindings.
- Local TUI: Chinese input; q/b/g stay in the draft; Enter submission; n/N navigation; Escape clearing; chapter change; no-result status. Captures replay the actual 80x24 terminal output: `target/issue77-input.png`, `target/issue77-hits.png`, `target/issue77-missing.png`.
- Network TUI: temporary runner under `target/` mounts the real `ReadNovel<NetworkNovel>` with an offline Fetcher and no persisted history. Searching 70 repeated matches, backward wrap to 70/70, canceling a q/b/g draft and forward wrap to 1/70 kept the fetch counter at 1. Changing chapters cleared search and increased it to 2. No live third-party source was used.
- TTS search-style precedence is covered by unit tests; audio playback was not exercised (models were not loaded).
- Completed simplify review of touched code, including input-layer guards, lock lifetimes, cached layout/matches, case-sensitive key hints and reading-mode help placement.

Default activation was subsequently changed from `/` to `s` at user request; the existing terminal captures predate that key change. Effective help/footer hints derive from the keymap.

For the `s` key follow-up, all five keymap tests passed using an independent rustc test harness against the current source; formatting and diff checks passed. Cargo tests/Clippy could not rerun because another Cargo check held the build lock.

## Release verification (2026-10-05)

- Re-ran the complete prescribed workspace test command after converting only the local schema file's CRLF checkout to LF; all 197 tests passed, with none excluded. Git's normalized schema content is unchanged.
- Clippy with warnings denied, formatting check and rustdoc with warnings denied passed against the final `s` binding.
- Rechecked touched search code using simplify criteria; no further code changes were needed.
- Planning documents remain design proposals; this release does not implement new TTS backends or role analysis.
