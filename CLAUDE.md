# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

TRNovel is a terminal novel reader (Rust, edition 2024) built on `ratatui` + a custom React-like TUI framework crate, `ratatui-kit`. It reads local `.txt` novels and network novels (via Legado-style "book sources"), tracks reading history, supports color themes, and offers TTS playback using the Kokoro model. UI text and code comments are in Chinese.

## Workspace layout

Cargo workspace with one root binary crate and two library members:

- **`trnovel`** (root, `src/`) — the app. Builds two identical binaries: `trnovel` (default) and `trn`.
- **`crates/parse-book-source`** — fetches/parses network novels from Legado-compatible book-source JSON.
- **`crates/novel-tts`** — Kokoro TTS engine wrapper + streaming audio pipeline.

## Commands

```bash
# Build / run
cargo build                       # debug; produces both trnovel and trn
cargo run                         # runs trnovel (default-run); opens the Home page
cargo run -- -q                   # quick: resume last reading position
cargo run -- -l <PATH>            # local novels from a directory
cargo run -- -n                   # network mode  | -H history  | -c clears ~/.novel

# Test / lint / docs — what CI and the pre-commit hook enforce
cargo test --locked --all-features --workspace --lib --tests --examples
cargo clippy --all-targets --all-features --workspace -- -D warnings   # -D warnings in CI
cargo fmt --all --check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --all-features --workspace --examples

# A single test (the app crate `src/` has NO tests — they live only in the two sub-crates)
cargo test -p parse-book-source <test_name>
cargo test -p novel-tts <test_name>

# Crate-scoped examples
cargo run -p parse-book-source --example json     # needs a book-source test.json
cargo run -p novel-tts --example basic            # downloads model + plays real audio

# Docs site (docs/ — Astro + Starlight, formatted with Biome)
cd docs && pnpm install && pnpm dev               # pnpm build / preview / astro check
```

`lefthook.yaml` runs test → `clippy --fix --allow-dirty` (auto-stages fixes) → `cargo fmt` → `cargo doc` on pre-commit. Releases go through `./release.sh` (cargo-release + git-cliff changelog, tag `<crate>-v<version>`) and cargo-dist (`.github/workflows/trnovel-release.yml`, triggered by `trnovel-v*` tags).

## Architecture

### UI framework: ratatui-kit (`src/app`, `src/pages`, `src/components`, `src/hooks`)

`ratatui-kit` is a React-like layer over ratatui. Understanding its model is the key to this codebase:

- **Components** are `#[component] fn(props, hooks) -> impl Into<AnyElement>`; UI is composed with the `element!` macro.
- **Hooks** drive everything: `use_state` (local `State<T>`; `.read()`/`.write()`/`.set()`), `use_context` (global state), `use_future`/`use_async_effect` (async side effects), `use_events` (keyboard), `use_memo`. The repo adds custom hooks in `src/hooks` — notably `UseThemeConfig` (`use_theme_token.rs`) and `UseInitState` (`use_init_state.rs`, async-load state that debounces the loading spinner by 200ms).
- **Routing** is declared with the `routes!` macro in `src/app/mod.rs` and served by `RouterProvider`. Navigation uses `use_navigate()` + `navigate.push(path)` / `push_with_state(path, T)`; the target page reads typed state via `use_route_state::<T>()`. All pages render inside `src/app/layout.rs` (the `Layout` outlet).

### App boot & global state (`src/app/mod.rs`)

`App` mounts `use_future` to load all caches (History, BookSourceCache, TTSConfig, theme.json) off the UI path, showing a `Loading` spinner only if init exceeds ~200ms, then renders a **nested `ContextProvider` chain**: ThemeConfig → History → BookSourceCache → TTSConfig → NovelTTS → is_inputting → `RouterProvider`. Pages reach this state via `use_context`. Reordering the providers risks breaking descendant lookups.

Routes: `/home`, `/select-history`, `/select-file` → `/local-novel`, `/book-source` → `/select-books` → `/book-detail` → `/network-novel`, `/theme-setting`. The CLI subcommand (`src/lib.rs`, clap derive) selects the initial flow.

### Keyboard handling — no central keymap

There is **no global keybinding table**. `Layout` handles a few app-wide keys; every page/component matches `KeyCode` itself inside its own `use_events` closure (vim-style `j/k/h/l`, `Tab`, `Enter`, etc.). The `is_inputting` context gates page shortcuts while a `SearchInput` is focused — pages must check it to avoid double-handling. The shortcut help overlay is `src/components/modal/shortcut_info_modal.rs`. (Note: issue #49 requests making these keys configurable — they are currently hardcoded across ~22 files; theme is already file-configurable but keybindings are not.)

### Novel domain (`src/novel`)

A single `Novel` trait (`novel_core.rs`) unifies both sources via `Deref<Target = NovelChapters<T>>`, so generic UI code (`ReadNovel<T>`) drives chapter navigation/content the same way for either:

- **`LocalNovel`** — `Arc<Mutex<File>>`; detects encoding (UTF-8, falling back to GBK), splits chapters by a `第…章` regex, and **stores byte offsets** to seek for pagination. Offsets become invalid if the file changes between sessions.
- **`NetworkNovel`** — `Arc<Mutex<BookSourceParser>>`; chapters/content are fetched lazily on demand from the book source.

### Persistence (`src/cache`, `src/utils.rs`)

All app state lives under **`~/.novel/`** (`utils::novel_catch_dir()`; `cargo run -- -c` deletes it entirely, no confirmation). Each cache type follows the same pattern: serde_json (de)serialization, a `save()` method, and a `Drop` impl that auto-saves on scope exit (errors are swallowed). Files: `theme.json`, `history.json` (max 100, MRU-deduped), `book_sources.json`, `tts_config.json`, and per-book snapshots in `local/{path_md5}.json` and `network/{url_md5}.json` (md5 keys via `utils::get_path_md5`/`get_md5_string`). `ReadNovel` saves history on drop.

### parse-book-source crate

Parses Legado-format book sources (`#[serde(rename_all = "camelCase")]`, e.g. `bookSourceUrl`). The public API is `BookSourceParser` (`search_books`, `explore_books`, `get_book_info`, `get_chapters`, `get_content`, `get_explores`). Extraction is a **rule DSL** dispatched by prefix to three analyzers: `@css:` (scraper CSS selectors, default), `@json:`/`$` (jsonpath-rust), and no-prefix Legado "Default" format (`default.rs` rewrites it to CSS). Rules chain with `@` (pipeline), `&&` (concat), `||` (fallback), `##` (regex replace), support `{{var}}`/`{{page}}` templating and `@put:`/`@get:` variable storage. `split_rule_resolve()` parses rules right-to-left. HTTP goes through a reqwest wrapper with cookies, headers, timeout, and optional token-bucket rate limiting. Legado compatibility is partial — not every rule works.

### novel-tts crate

Wraps `kokoro-tts` (Chinese v1.1 model). `NovelTTS::new(model, voices)` loads the ONNX model; `ChapterTTS::stream(voice, on_error)` spawns a tokio task that splits text via `preprocess_text` (recursive punctuation splitting into ~200-byte segments with byte ranges), synthesizes each, and pushes to a custom queue (`src/queue`, a `Source` implementor with silence padding) feeding a rodio `Sink`. A `position_rx` channel reports which segment is playing so the reader can highlight in sync. Model files (`kokoro-v1.1-zh.onnx`, `voices-v1.1-zh.bin`, large) auto-download from GitHub into **`~/.novel-tts/kokoro/`** with HTTP Range resume; cancellation is via `CancellationToken`.

### Errors

`src/errors.rs` defines a `thiserror` `Errors` enum (`#[from]` for io / serde_json / tokio lock / parse_book_source errors, plus a `Warning(String)` variant and anyhow passthrough); `Result<T>` aliases it. Errors surface to the UI as a `WarningModal`. Each crate has its own analogous error enum (`ParseError`, `NovelTTSError`).

## Platform / build notes

- **Edition 2024** — needs Rust ≥ 1.85 (stable; no nightly required).
- **Linux build deps**: `libasound2-dev` (rodio), `libssl-dev`, `pkg-config` — CI installs them via apt.
- **Windows**: `msvc-crt-static = false` in the root `Cargo.toml` `[workspace.metadata.dist]` is mandatory — ort/onnxruntime ship with a dynamic CRT and static linking causes unresolved-symbol (`__imp_tolower`) LNK errors.
