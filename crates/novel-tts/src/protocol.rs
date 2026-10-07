use crate::{resources::Resources, runtime::Worker};
use std::{
    io::{BufReader, BufWriter, Write},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, oneshot};
use tts_core::{checkpoint::CheckpointStore, config::ConfigStore, session::SessionEvent};
use tts_protocol::{
    Command, ErrorInfo, Event, Message, PROTOCOL_VERSION, Request, RequestTracker, decode, encode,
    read_frame,
};

struct Output {
    instance: String,
    sequence: u64,
    messages: mpsc::Sender<Message>,
}
impl Output {
    async fn send(
        &mut self,
        request: Option<&Request>,
        session: Option<String>,
        event: Event,
    ) -> anyhow::Result<()> {
        self.sequence += 1;
        let message = Message {
            protocol_version: PROTOCOL_VERSION,
            instance_id: self.instance.clone(),
            session_id: session.or_else(|| request.and_then(|r| r.session_id.clone())),
            sequence: self.sequence,
            request_id: request.map(|r| r.request_id.clone()),
            event,
        };
        tokio::time::timeout(Duration::from_secs(5), self.messages.send(message)).await??;
        Ok(())
    }
    async fn error(
        &mut self,
        request: Option<&Request>,
        code: &str,
        stage: &str,
        message: impl ToString,
    ) -> anyhow::Result<()> {
        self.send(
            request,
            None,
            Event::Error(ErrorInfo {
                code: code.into(),
                stage: stage.into(),
                message: message.to_string(),
                retryable: code != "incompatible_version",
            }),
        )
        .await
    }
}

pub async fn run(
    store: ConfigStore,
    checkpoints: CheckpointStore,
    resources: Resources,
) -> anyhow::Result<()> {
    let (input_tx, mut input) = mpsc::channel(8);
    // A dedicated stdin thread can end with the process even if a broken parent
    // never closes its pipe; it is not a Tokio blocking task keeping the runtime alive.
    std::thread::Builder::new()
        .name("tts-stdin".into())
        .spawn(move || {
            let mut reader = BufReader::new(std::io::stdin());
            loop {
                let frame = read_frame(&mut reader);
                let end = !matches!(&frame, Ok(Some(_)));
                if input_tx.blocking_send(frame).is_err() || end {
                    break;
                }
            }
        })?;
    let (messages, mut outgoing) = mpsc::channel::<Message>(64);
    let (finished, writer) = oneshot::channel();
    // A blocked parent stdout must not keep Tokio's blocking pool alive on exit.
    std::thread::Builder::new()
        .name("tts-stdout".into())
        .spawn(move || {
            let result = (|| {
                let mut stdout = BufWriter::new(std::io::stdout());
                while let Some(message) = outgoing.blocking_recv() {
                    stdout.write_all(&encode(&message)?)?;
                    stdout.flush()?;
                }
                Ok::<_, anyhow::Error>(())
            })();
            let _ = finished.send(result);
        })?;
    let result = serve(store, checkpoints, resources, &mut input, messages).await;
    let written = tokio::time::timeout(Duration::from_secs(3), writer).await;
    match written {
        Ok(joined) => joined??,
        Err(_) => return Err(anyhow::anyhow!("protocol stdout did not drain")),
    }
    result
}

async fn serve(
    store: ConfigStore,
    checkpoints: CheckpointStore,
    resources: Resources,
    input: &mut mpsc::Receiver<Result<Option<Vec<u8>>, tts_protocol::ProtocolError>>,
    messages: mpsc::Sender<Message>,
) -> anyhow::Result<()> {
    let mut output = Output {
        instance: format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ),
        sequence: 0,
        messages,
    };
    let (events, mut session_events) = mpsc::channel::<SessionEvent>(64);
    let (progress, mut model_events) = mpsc::channel::<Event>(16);
    let mut tracker = RequestTracker::default();
    let mut handshake = false;
    let deadline = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(deadline);
    let mut worker = Worker::new(store, checkpoints, resources, events, progress);
    let result: anyhow::Result<()> = async {
        loop {
            let preparing = worker.is_preparing();
            tokio::select! {
                _ = &mut deadline, if !handshake => {
                    output.error(None, "handshake_timeout", "protocol", "hello required within five seconds").await?;
                    break;
                }
                event = session_events.recv() => {
                    if let Some(event) = event {
                        output.send(None, Some(event.session_id), event.event).await?;
                    }
                }
                event = model_events.recv(), if handshake => {
                    if let Some(event) = event {
                        output.send(None, None, event).await?;
                    }
                }
                response = worker.prepared(), if preparing => {
                    output.send(None, response.session, response.event).await?;
                }
                frame = input.recv() => {
                    let bytes = match frame {
                        Some(Ok(Some(bytes))) => bytes,
                        Some(Err(error)) => {
                            output.error(None, "invalid_message", "protocol", error).await?;
                            break;
                        }
                        _ => break,
                    };
                    let request: Request = match decode(&bytes) {
                        Ok(request) => request,
                        Err(error) => {
                            output.error(None, "invalid_message", "protocol", error).await?;
                            continue;
                        }
                    };
                    if let Err(error) = tracker.accept(&request) {
                        let incompatible = request.protocol_version != PROTOCOL_VERSION;
                        let code = if incompatible { "incompatible_version" } else { "invalid_request" };
                        output.error(Some(&request), code, "protocol", error).await?;
                        if incompatible { break; }
                        continue;
                    }
                    if !handshake && !matches!(request.command, Command::Hello) {
                        output.error(Some(&request), "hello_required", "protocol", "first command must be hello").await?;
                        continue;
                    }
                    match &request.command {
                        Command::Hello => {
                            handshake = true;
                            output.send(Some(&request), None, Event::Ready(worker.catalog()?)).await?;
                            #[cfg(any(feature="moss",feature="kokoro",feature="qwen",feature="voxcpm",feature="omnivoice",feature="zipvoice"))]
                            for component in ["tts","alignment"] {
                                output.send(None,None,worker.unprepared_device_status(component)?).await?;
                            }
                        }
                        Command::Shutdown => {
                            output.send(Some(&request), None, Event::Accepted).await?;
                            break;
                        }
                        command => {
                            let response = worker.command(&request).await;
                            if matches!(command, Command::CancelPrepare) || matches!(command, Command::UpdateConfig(patch) if patch.backend.is_some() || patch.model.is_some() || patch.tts_device.is_some() || patch.alignment_device.is_some() || patch.alignment_enabled.is_some()) {
                                while model_events.try_recv().is_ok() {}
                            }
                            let config_changed = matches!(&response.event, Event::ConfigChanged(_))
                                && matches!(command, Command::UpdateConfig(patch) if patch.backend.is_some() || patch.model.is_some() || patch.tts_device.is_some() || patch.alignment_device.is_some() || patch.alignment_enabled.is_some());
                            output.send(Some(&request), response.session, response.event).await?;
                            #[cfg(any(feature="moss",feature="kokoro",feature="qwen",feature="voxcpm",feature="omnivoice",feature="zipvoice"))]
                            if config_changed && !worker.is_preparing() && !worker.has_prepared_model() {
                                output.send(None,None,worker.unprepared_device_status("tts")?).await?;
                            }
                            #[cfg(not(any(feature="moss",feature="kokoro",feature="qwen",feature="voxcpm",feature="omnivoice",feature="zipvoice")))]
                            let _ = config_changed;
                        }
                    }
                }
            }
        }
        Ok(())
    }.await;
    worker.shutdown().await?;
    while let Ok(event) = session_events.try_recv() {
        output
            .send(None, Some(event.session_id), event.event)
            .await?;
    }
    result
}
