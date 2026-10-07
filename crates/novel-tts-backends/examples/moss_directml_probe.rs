//! Development-only DirectML evaluation; no product device or config is changed.
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    novel_tts_backends::moss::directml_probe::main()
}
#[cfg(not(windows))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("DirectML evaluation requires Windows");
}
