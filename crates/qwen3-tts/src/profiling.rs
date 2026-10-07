//! Feature-gated profiling support via `tracing-chrome`.
//!
//! When the `profiling` feature is enabled, [`init`] sets up a Chrome trace
//! subscriber that writes to `trace.json` in the current directory. Open the
//! resulting file in `chrome://tracing` or [Perfetto](https://ui.perfetto.dev).
//!
//! When the feature is disabled, [`init`] returns `None` and has zero overhead.

/// Guard that flushes the trace file on drop.
///
/// Hold this in `main()` — the trace is written when the guard is dropped.
#[cfg(feature = "profiling")]
pub struct ProfilingGuard {
    _guard: tracing_chrome::FlushGuard,
}

/// Opaque guard type when profiling is disabled (zero-size, no-op drop).
#[cfg(not(feature = "profiling"))]
pub struct ProfilingGuard;

/// Initialize the Chrome tracing layer.
///
/// Returns `Some(guard)` when the `profiling` feature is active, `None` otherwise.
/// The caller must hold the guard until the end of the program.
///
/// # Example
///
/// ```rust,ignore
/// let _guard = qwen3_tts::profiling::init();
/// // ... run workload ...
/// // trace.json is written when _guard is dropped
/// ```
pub fn init() -> Option<ProfilingGuard> {
    #[cfg(feature = "profiling")]
    {
        use tracing_chrome::ChromeLayerBuilder;
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let (chrome_layer, guard) = ChromeLayerBuilder::new()
            .file("trace.json")
            .include_args(true)
            .build();

        tracing_subscriber::registry().with(chrome_layer).init();

        tracing::info!("Profiling enabled — trace will be written to trace.json");
        Some(ProfilingGuard { _guard: guard })
    }

    #[cfg(not(feature = "profiling"))]
    {
        None
    }
}
