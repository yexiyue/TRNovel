# Repository Guidelines

## Development Workflow

Before any development task that writes code, changes configuration, or adds dependencies, use the `dev-workflow` skill. It loads project notes from `dev-notes/knowledge/` and guides whether new project knowledge should be added after the change.

Relevant project notes:

- `dev-notes/knowledge/tui-ratatui-kit.md`: ratatui-kit hooks, render timing pitfalls, route conventions, and keyboard handling.
- `dev-notes/knowledge/booksource.md`: book-source rule DSL, render-fetcher, anti-scraping behavior, Fanqie support, and `novel-tts`.
- `dev-notes/knowledge/toolchain.md`: Cargo workspace conventions, feature gates, pinned dependencies, CI, release, and `mod.rs` layout decisions.

Use existing OpenSpec entries under `openspec/changes/` before changing book-source behavior or other proposed features.

## Project Overview

TRNovel is a Rust 2024 terminal novel reader built on `ratatui` plus the custom React-like TUI framework crate `ratatui-kit`. It reads local `.txt` novels and network novels through Legado-style book sources, persists reading history and theme state, and supports Kokoro-based TTS playback. UI text and code comments are primarily Chinese.

The workspace has one root application crate and two library crates:

- `trnovel` in `src/`: main app; builds the `trnovel` and `trn` binaries.
- `crates/parse-book-source`: Legado-compatible book-source parsing, fetching, and rule evaluation.
- `crates/novel-tts`: Kokoro TTS wrapper and streaming audio pipeline.

Docs are an Astro/Starlight site in `docs/`. Integration tests live in `tests/`; optional large fixtures live in `test-novels/` and must be skipped cleanly when absent.

## Common Commands

```bash
# Build / run
cargo build
cargo run
cargo run --bin trn
cargo run -- -q
cargo run -- -l <PATH>
cargo run -- -n

# CI / pre-commit checks
cargo test --locked --all-features --workspace --lib --tests --examples
cargo clippy --all-targets --all-features --workspace -- -D warnings
cargo fmt --all --check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --all-features --workspace --examples

# Single-crate tests
cargo test -p parse-book-source <test_name>
cargo test -p novel-tts <test_name>

# Examples
cargo run -p parse-book-source --example json
cargo run -p novel-tts --example basic

# Book-source schema
cargo run -p parse-book-source --features schema --example gen_schema

# Docs
cd docs && pnpm install && pnpm dev
cd docs && pnpm build
```

`lefthook.yaml` runs tests, `clippy --fix --allow-dirty`, `cargo fmt`, and `cargo doc` on pre-commit. Release flow uses `./release.sh`, cargo-release, git-cliff, and cargo-dist via `.github/workflows/trnovel-release.yml`.

## Coding Style

Use Rust 2024 idioms. Formatting is controlled by `rustfmt.toml` with `tab_spaces = 4`. Use `snake_case` for modules, functions, and variables; use `PascalCase` for public types. Keep shared dependencies in `[workspace.dependencies]`.

Avoid broad refactors in parser, fetcher, UI state, and cache code unless the task requires it. These areas have cross-crate or runtime behavior that is easy to disturb.

The app crate under `src/` currently has no tests; most tests are in the sub-crates. Put crate-local unit tests beside the implementation and cross-cutting tests in `tests/`. Name tests after the behavior they lock down.

## Architecture Notes

### TUI and Routing

`ratatui-kit` components use `#[component] fn(props, hooks) -> impl Into<AnyElement>` and compose UI with the `element!` macro. Hooks drive local state, global context, async effects, keyboard events, and memoization.

Routes are declared with `routes!` in `src/app/mod.rs` and served by `RouterProvider`. Navigation uses `use_navigate()` with `push` or `push_with_state`; target pages read typed route state with `use_route_state::<T>()`. All pages render through `src/app/layout.rs`.

`App` initializes caches and theme/TTS state, then renders a nested `ContextProvider` chain. Do not reorder providers casually; descendant `use_context` lookups depend on the chain.

### Keyboard Handling

There is no central keybinding table. `Layout` handles only a few global keys; pages and components match `KeyCode` in their own `use_events` closures. When a `SearchInput` or other input is focused, pages must respect the `is_inputting` context so shortcuts are not handled twice.

### Novel Domain

The `Novel` trait in `src/novel/novel_core.rs` unifies local and network novels so generic reader UI can drive chapter navigation and content loading.

`LocalNovel` stores byte offsets for chapter pagination after detecting UTF-8 or GBK text. Those offsets become invalid if the source file changes between sessions.

`NetworkNovel` fetches chapters and content lazily through `BookSourceParser`.

### Persistence

App state lives under `~/.novel/`. `cargo run -- -c` deletes that directory without confirmation. Cache types generally use serde_json, a `save()` method, and `Drop` auto-save; save errors are swallowed. History is MRU-deduped and capped.

### Book Sources

`parse-book-source` parses Legado-style JSON with camelCase fields. Rules dispatch by prefix to CSS, JSONPath, or default Legado parsing. Rules support chaining, fallback, concat, regex replacement, `{{var}}` and `{{page}}` templating, and `@put:` / `@get:` variables.

Before changing book-source behavior, inspect the matching OpenSpec change when one exists, update schema/docs examples when model types change, and validate generated or imported book-source JSON with `trn doctor` before calling it working.

### TTS

`novel-tts` wraps the Chinese Kokoro v1.1 model. Large model files are downloaded to `~/.novel-tts/kokoro/` with HTTP Range resume. Streaming uses spawned async work and cancellation tokens; after changing public async APIs called from `tokio::spawn`, run `cargo build` in addition to tests to verify futures remain `Send` in the app context.

## Platform Notes

Rust 1.85 or newer is required for edition 2024. Linux builds require `libasound2-dev`, `libssl-dev`, and `pkg-config`. On Windows, keep `msvc-crt-static = false` in root `Cargo.toml` workspace dist metadata because ONNX Runtime ships with a dynamic CRT.

## Commit and PR Notes

Use Conventional Commit style with scopes, such as `feat(booksource): ...`, `fix(booksource): ...`, and `docs(openspec): ...`. Keep messages imperative and scoped to the changed subsystem.

PRs should include the problem, solution, related issue or OpenSpec change, test commands run, and screenshots or terminal captures for UI/docs changes. Mention schema, cache, migration, or persisted-state impacts explicitly.

@RTK.md
