use crate::resources::Resources;
use crossterm::{
    event::{Event as TerminalEvent, EventStream, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use std::{io::IsTerminal, path::PathBuf, rc::Rc};
use tokio::sync::mpsc;
use tts_core::{
    AudioPlayer,
    checkpoint::CheckpointStore,
    config::ConfigStore,
    session::{SessionEvent, SessionManager},
};
use tts_protocol::{EndReason, Event, SourceId, StartRequest, text_hash};

struct TerminalGuard;
impl TerminalGuard {
    fn enter() -> std::io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

pub async fn run(
    file: PathBuf,
    store: ConfigStore,
    checkpoints: CheckpointStore,
    resources: Resources,
    restore_checkpoint: bool,
) -> anyhow::Result<()> {
    // Read and validate before model download or opening the output device.
    let text = tokio::fs::read_to_string(&file)
        .await
        .map_err(|error| anyhow::anyhow!("cannot read UTF-8 file {}: {error}", file.display()))?;
    let canonical = tokio::fs::canonicalize(&file).await?;
    let config = store.load()?;
    tts_core::config::validate(&config, &tts_core::backend::KokoroBackend::capabilities())?;
    let (progress, mut preparation) = mpsc::channel(16);
    let mut task = tokio::task::spawn_local(resources.prepare(progress));
    let backend = loop {
        tokio::select! {
            result = &mut task => break result??,
            event = preparation.recv() => if let Some(Event::ModelProgress {resource,downloaded,total}) = event {
                eprintln!("{resource}: {downloaded}/{total} bytes");
            },
            signal = tokio::signal::ctrl_c() => { signal?; task.abort(); let _ = task.await; return Ok(()); }
        }
    };
    let (tx, mut events) = mpsc::channel::<SessionEvent>(64);
    let mut manager = SessionManager::new(backend, Rc::new(AudioPlayer::open()?), checkpoints, tx);
    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    let _terminal = if interactive {
        Some(TerminalGuard::enter()?)
    } else {
        None
    };
    let mut keys = if interactive {
        Some(EventStream::new())
    } else {
        None
    };
    let mut paused = false;
    let id = "cli-file";
    manager
        .start(
            id.into(),
            StartRequest {
                source: SourceId {
                    namespace: "cli-file".into(),
                    book: canonical.to_string_lossy().into(),
                    chapter: "file".into(),
                },
                text_hash: text_hash(&text),
                text,
                resume_byte: if restore_checkpoint { None } else { Some(0) },
                restore_checkpoint,
            },
            &config,
        )
        .await?;
    if interactive {
        eprintln!("space: pause/resume; s: stop; q: exit\r");
    }
    let result: anyhow::Result<()> = async {
        loop {
            tokio::select! {
                event = events.recv() => {
                    let Some(event) = event else {break};
                    match event.event {
                        Event::SessionState{state} => eprintln!("{state:?}\r"),
                        Event::Error(error) => eprintln!("{}: {}\r",error.stage,error.message),
                        Event::SegmentStarted{range,..} => eprintln!("bytes {}..{}\r",range.start,range.end),
                        Event::SessionEnded{reason,..} => {
                            if reason == EndReason::Failed {return Err(anyhow::anyhow!("listening failed; run again to retry the unfinished segment"));}
                            break;
                        },
                        _ => {}
                    }
                }
                key = async {keys.as_mut().expect("guarded").next().await}, if interactive => {
                    if let Some(Ok(TerminalEvent::Key(key))) = key && key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break,
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                            KeyCode::Char('s') => {manager.stop().await?; break;},
                            KeyCode::Char(' ') => {
                                if paused {manager.resume(id).await?;} else {manager.pause(id).await?;}
                                paused = !paused;
                            }
                            _ => {}
                        }
                    }
                }
                signal = tokio::signal::ctrl_c() => {signal?; break;}
            }
        }
        Ok(())
    }.await;
    manager.stop().await?;
    result
}
