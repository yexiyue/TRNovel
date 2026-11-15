//! Reader preference saves stay off the UI thread and are ordered with the final flush.
use super::{DebounceOptions, UseDebounceEffect};
use crate::{ReaderDisplayConfig, state::READER_DISPLAY};
use ratatui_kit::{Hooks, UseOnDrop, UseState};
use std::sync::{Arc, Mutex};

// Shared across page mounts: a late job from the previous page must not race
// the current page's final flush. Read the atom only after acquiring this gate.
static SAVE_GATE: Mutex<()> = Mutex::new(());
struct ReaderPreferences {
    saved: Mutex<ReaderDisplayConfig>,
}
impl ReaderPreferences {
    fn save_latest(
        &self,
        read_current: impl FnOnce() -> ReaderDisplayConfig,
        save: impl FnOnce(&ReaderDisplayConfig) -> crate::Result<()>,
    ) -> crate::Result<()> {
        let _gate = SAVE_GATE.lock().unwrap_or_else(|e| e.into_inner());
        let current = read_current();
        let mut saved = self.saved.lock().unwrap_or_else(|e| e.into_inner());
        if *saved != current {
            save(&current)?;
            *saved = current;
        }
        Ok(())
    }
    fn flush(&self) {
        let _ = self.save_latest(|| READER_DISPLAY.get(), ReaderDisplayConfig::save);
    }
}
pub fn use_reader_preferences(hooks: &mut Hooks, current: ReaderDisplayConfig) {
    let saver = hooks
        .use_state(|| {
            Arc::new(ReaderPreferences {
                saved: Mutex::new(current),
            })
        })
        .read()
        .clone();
    hooks.use_debounce_effect(
        {
            let saver = saver.clone();
            move || {
                let saver = saver.clone();
                tokio::task::spawn_blocking(move || saver.flush());
            }
        },
        current,
        DebounceOptions::default(),
    );
    // Wait for an in-flight save; queued jobs re-read the latest atom under the gate.
    hooks.use_on_drop(move || saver.flush());
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_worker_reads_latest_value_and_failed_saves_remain_retryable() {
        let initial = ReaderDisplayConfig::default();
        let latest = ReaderDisplayConfig {
            page_overlap: initial.page_overlap + 1,
            ..initial
        };
        let saver = ReaderPreferences {
            saved: Mutex::new(initial),
        };
        assert!(
            saver
                .save_latest(
                    || latest,
                    |_| Err(crate::errors::Errors::from(anyhow::anyhow!(
                        "disk unavailable"
                    )))
                )
                .is_err()
        );
        let mut writes = vec![];
        saver
            .save_latest(
                || latest,
                |value| {
                    writes.push(*value);
                    Ok(())
                },
            )
            .unwrap();
        // A queued callback captures the source, never its stale configuration.
        saver
            .save_latest(
                || latest,
                |value| {
                    writes.push(*value);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(writes, vec![latest]);
    }
    #[test]
    fn final_flush_orders_after_in_flight_write_and_before_late_jobs() {
        use std::{sync::mpsc, thread};
        let initial = ReaderDisplayConfig::default();
        let previous = ReaderDisplayConfig {
            page_overlap: 1,
            ..initial
        };
        let latest = ReaderDisplayConfig {
            page_overlap: 2,
            ..initial
        };
        let source = Arc::new(Mutex::new(previous));
        let disk = Arc::new(Mutex::new(initial));
        let saver = Arc::new(ReaderPreferences {
            saved: Mutex::new(initial),
        });
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = thread::spawn({
            let saver = saver.clone();
            let source = source.clone();
            let disk = disk.clone();
            move || {
                saver
                    .save_latest(
                        || *source.lock().unwrap(),
                        |value| {
                            started_tx.send(()).unwrap();
                            release_rx.recv().unwrap();
                            *disk.lock().unwrap() = *value;
                            Ok(())
                        },
                    )
                    .unwrap();
            }
        });
        started_rx.recv().unwrap();
        *source.lock().unwrap() = latest;
        let final_flush = thread::spawn({
            let saver = saver.clone();
            let source = source.clone();
            let disk = disk.clone();
            move || {
                saver
                    .save_latest(
                        || *source.lock().unwrap(),
                        |value| {
                            *disk.lock().unwrap() = *value;
                            Ok(())
                        },
                    )
                    .unwrap();
            }
        });
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        final_flush.join().unwrap();
        // A job queued before unmount but scheduled afterwards must re-read latest.
        saver
            .save_latest(
                || *source.lock().unwrap(),
                |value| {
                    *disk.lock().unwrap() = *value;
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(*disk.lock().unwrap(), latest);
    }
}
