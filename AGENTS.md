# Repository Guidelines

## Development Workflow

Before changing code, configuration or dependencies, read `.claude/skills/dev-workflow/SKILL.md`. Load the relevant project notes and update them when new project-specific knowledge is learned:

- `dev-notes/knowledge/toolchain.md`: workspace, dependencies, platform builds, CI and releases.
- `dev-notes/knowledge/tui-ratatui-kit.md`: TUI components, hooks, themes, routing and keyboard handling.
- `dev-notes/knowledge/booksource.md`: v2 book sources, rule evaluation, browser fetching and TTS.

Consult applicable proposals in `openspec/changes/` before changing behavior. Historical design records are not a substitute for current source code.

## Project Layout

TRNovel is a Rust 2024 terminal novel reader. The root application builds `trnovel` and `trn`; the workspace library is `crates/parse-book-source`. TTS is the independently installed Talechime executable, connected via crates.io `talechime-protocol 0.1.0` and JSON Lines v5. No Git submodule is required. Documentation uses Astro/Starlight in `docs/`.

Book sources use structured `trnovel-booksource/v2` JSON and `parse_book_source::Engine`; they do not directly accept Legado book-source JSON. Keep source types, JSON Schema and documentation examples in sync.

The TUI uses ratatui-kit components and hooks. Routes and root providers live in `src/app/`. Process-wide appearance, reader preferences, TTS handles and keybindings live in `src/state.rs` as atoms; caches that rely on `Drop::save` remain owned by the app. Configurable keyboard actions live in `src/keymap/` and load `~/.trnovel/keybindings.toml`.

Paths are centralized in `src/paths.rs`. `~/.trnovel/config.toml` stores appearance/reader/browser/tts sections through the transactional ConfigStore; data lives in `data/`, rebuildable resources in `cache/`. Talechime owns `~/.talechime/`. Never read, migrate or delete legacy homes automatically. `clear` preserves preferences, book sources and login state.

## Commands

```bash
cargo run
cargo run -- -l <PATH>
cargo run -- -n
cargo run -- -q
cargo run -- -H

cargo test --locked --all-features --workspace --lib --tests --examples
cargo clippy --all-targets --all-features --workspace -- -D warnings
cargo fmt --all --check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --all-features --workspace --examples

cargo run -p parse-book-source --features schema --example gen_schema

cd docs
pnpm install
pnpm dev
pnpm build
```

Tests exist in both the application and library crates, plus `tests/`. Optional large fixtures in `test-novels/` must be skipped cleanly when absent. For public async engine APIs called by `tokio::spawn`, also build the application to verify `Send` in the consumer context.

## Coding and Build Conventions

Use Rust 2024 idioms and `rustfmt.toml`. Modules use `foo.rs`, with a same-named directory only when child modules exist, functions and variables use snake_case, and types use PascalCase. Keep shared dependencies in `[workspace.dependencies]`. Avoid unrelated parser, UI-state or cache refactors.

Rust 1.89 or newer is required. Linux reader builds need OpenSSL development libraries and pkg-config. Preserve the pinned ort version and `msvc-crt-static = false`; see toolchain notes before changing native dependencies.

`lefthook.yaml` runs tests, Clippy, formatting and rustdoc before commits. Honor any explicit user instruction to validate through CI instead of running locally.

## Releases and Contributions

`.github/scripts/release.sh` uses cargo-release and git-cliff. Application tags `trnovel-v*` trigger cargo-dist; crate tags also trigger crates.io publication. Change cargo-dist metadata and regenerate `.github/workflows/trnovel-release.yml` rather than editing the generated workflow. npm uses OIDC Trusted Publishing; Homebrew uses its tap token. Both applications use cargo-dist 0.32.0 and one native Cargo dist-workspace.toml each. Official targets are Apple Silicon macOS, x86_64 Linux GNU and x86_64 Windows MSVC. TRNovel never bundles a worker or audio inference libraries; basic, accelerated and ARM64 musl distribution are retired.

Use Conventional Commit messages. PRs should explain the behavior, scope and validation, with persisted-format or schema impacts when relevant. Include screenshots for visible UI changes.
