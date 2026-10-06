//! Lightweight, independently drained JSON Lines process transport.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command as ProcessCommand},
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use tts_protocol::{
    Command, Event, MAX_MESSAGE_BYTES, Message, PROTOCOL_VERSION, Request, decode, encode,
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const LOG_BYTES: usize = 8192;
type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Message>>>>;

#[derive(Debug, Clone)]
pub enum Received {
    Message(Message),
    Disconnected(String),
}

/// A validated worker connection. Clone handles; explicit close releases resources.
#[derive(Clone)]
pub struct Client(Arc<Connection>);
struct Connection {
    child: Arc<Mutex<Option<Child>>>,
    writer: mpsc::Sender<Vec<u8>>,
    pending: Pending,
    events: broadcast::Sender<Received>,
    broken: Arc<AtomicBool>,
    counter: AtomicU64,
    instance: String,
    logs: Arc<Mutex<Vec<u8>>>,
    readers: Vec<JoinHandle<()>>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        for task in &self.readers {
            task.abort();
        }
        if let Some(mut child) = self.child.lock().expect("child mutex").take() {
            let _ = child.start_kill();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = child.wait().await;
                });
            }
        }
    }
}

/// Explicit paths fail directly. Otherwise prefer the reader's directory over PATH.
pub fn discover(
    explicit: Option<&Path>,
    executable: &Path,
    search_path: Option<&std::ffi::OsStr>,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        anyhow::ensure!(is_program(path), "听书程序路径不可用: {}", path.display());
        return Ok(path.into());
    }
    let name = if cfg!(windows) {
        "novel-tts.exe"
    } else {
        "novel-tts"
    };
    if let Some(directory) = executable.parent() {
        let path = directory.join(name);
        if is_program(&path) {
            return Ok(path);
        }
    }
    if let Some(paths) = search_path {
        for directory in std::env::split_paths(paths) {
            let path = directory.join(name);
            if is_program(&path) {
                return Ok(path);
            }
        }
    }
    Err(anyhow::anyhow!(
        "未找到 novel-tts；安装听书版或通过 --tts-program 指定程序路径"
    ))
}
fn is_program(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

async fn read_line(
    reader: &mut BufReader<impl AsyncRead + Unpin>,
) -> anyhow::Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            anyhow::ensure!(frame.is_empty(), "unterminated protocol message");
            return Ok(None);
        }
        let length = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |index| index + 1);
        anyhow::ensure!(
            frame.len() + length <= MAX_MESSAGE_BYTES + 2,
            "worker message exceeds protocol limit"
        );
        frame.extend_from_slice(&buffer[..length]);
        reader.consume(length);
        if frame.ends_with(b"\n") {
            return Ok(Some(frame));
        }
    }
}

impl Client {
    /// Starts without a shell and validates hello before exposing the connection.
    pub async fn connect(program: &Path) -> anyhow::Result<(Self, Message)> {
        let mut child = ProcessCommand::new(program)
            .arg("--protocol")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        let mut stderr = child.stderr.take().expect("piped stderr");
        let logs = Arc::new(Mutex::new(Vec::new()));
        let log_copy = logs.clone();
        let logger = tokio::spawn(async move {
            let mut buffer = [0u8; 2048];
            while let Ok(count) = stderr.read(&mut buffer).await {
                if count == 0 {
                    break;
                }
                let mut logs = log_copy.lock().expect("log mutex");
                logs.extend_from_slice(&buffer[..count]);
                if logs.len() > LOG_BYTES {
                    let excess = logs.len() - LOG_BYTES;
                    logs.drain(..excess);
                }
            }
        });
        let hello = Request {
            protocol_version: PROTOCOL_VERSION,
            request_id: "hello".into(),
            session_id: None,
            command: Command::Hello,
        };
        let handshake = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            stdin.write_all(&encode(&hello)?).await?;
            stdin.flush().await?;
            let bytes = read_line(&mut stdout)
                .await?
                .ok_or_else(|| anyhow::anyhow!("worker exited before hello"))?;
            let message: Message = decode(&bytes)?;
            anyhow::ensure!(
                message.protocol_version == PROTOCOL_VERSION
                    && message.request_id.as_deref() == Some("hello")
                    && !message.instance_id.is_empty()
                    && matches!(message.event, Event::Ready(_)),
                "听书程序不兼容或握手失败: {:?}",
                message.event
            );
            Ok::<_, anyhow::Error>(message)
        })
        .await;
        let ready = match handshake {
            Ok(Ok(message)) => message,
            result => {
                logger.abort();
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(match result {
                    Ok(Err(error)) => error,
                    _ => anyhow::anyhow!("听书程序握手超时"),
                });
            }
        };
        let (events, _) = broadcast::channel(64);
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let broken = Arc::new(AtomicBool::new(false));
        let (writer, mut queue) = mpsc::channel::<Vec<u8>>(16);
        let failed = broken.clone();
        let notify = events.clone();
        let child = Arc::new(Mutex::new(Some(child)));
        let writer_child = child.clone();
        let writer_pending = pending.clone();
        let write_task = tokio::spawn(async move {
            while let Some(bytes) = queue.recv().await {
                let result = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
                    stdin.write_all(&bytes).await?;
                    stdin.flush().await
                })
                .await;
                if !matches!(result, Ok(Ok(()))) {
                    failed.store(true, Ordering::SeqCst);
                    let _ =
                        notify.send(Received::Disconnected("听书程序输入管道关闭或超时".into()));
                    writer_pending.lock().expect("pending mutex").clear();
                    reap(&writer_child).await;
                    break;
                }
            }
        });
        let child_to_reap = child.clone();
        let replies = pending.clone();
        let notify = events.clone();
        let failed = broken.clone();
        let instance = ready.instance_id.clone();
        let mut sequence = ready.sequence;
        let read_task = tokio::spawn(async move {
            let result: anyhow::Result<()> = async {
                while let Some(bytes) = read_line(&mut stdout).await? {
                    let message: Message = decode(&bytes)?;
                    anyhow::ensure!(
                        message.protocol_version == PROTOCOL_VERSION
                            && message.instance_id == instance
                            && message.sequence > sequence,
                        "worker protocol version, instance or sequence changed"
                    );
                    sequence = message.sequence;
                    if let Some(id) = &message.request_id
                        && let Some(reply) = replies.lock().expect("pending mutex").remove(id)
                    {
                        let _ = reply.send(message);
                        continue;
                    }
                    let _ = notify.send(Received::Message(message));
                }
                Ok(())
            }
            .await;
            failed.store(true, Ordering::SeqCst);
            replies.lock().expect("pending mutex").clear();
            let reason = result.err().map_or_else(
                || "听书程序已退出；可主动重试".into(),
                |error| error.to_string(),
            );
            let _ = notify.send(Received::Disconnected(reason));
            reap(&child_to_reap).await;
        });
        Ok((
            Self(Arc::new(Connection {
                child,
                writer,
                pending,
                events,
                broken,
                counter: AtomicU64::new(0),
                instance: ready.instance_id.clone(),
                logs,
                readers: vec![logger, write_task, read_task],
            })),
            ready,
        ))
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Received> {
        self.0.events.subscribe()
    }
    pub fn instance(&self) -> &str {
        &self.0.instance
    }
    pub fn logs(&self) -> String {
        String::from_utf8_lossy(&self.0.logs.lock().expect("log mutex")).into_owned()
    }
    pub fn is_connected(&self) -> bool {
        !self.0.broken.load(Ordering::SeqCst)
    }

    /// Correlated response; async progress is drained independently while waiting.
    pub async fn command(
        &self,
        session: Option<String>,
        command: Command,
    ) -> anyhow::Result<Message> {
        anyhow::ensure!(self.is_connected(), "听书程序已断开；请主动重试");
        let id = format!("reader-{}", self.0.counter.fetch_add(1, Ordering::SeqCst));
        let bytes = encode(&Request {
            protocol_version: PROTOCOL_VERSION,
            request_id: id.clone(),
            session_id: session,
            command,
        })?;
        let (reply, response) = oneshot::channel();
        {
            let mut pending = self.0.pending.lock().expect("pending mutex");
            anyhow::ensure!(pending.len() < 64, "too many pending worker commands");
            pending.insert(id.clone(), reply);
        }
        let _pending = PendingRequest {
            id,
            pending: self.0.pending.clone(),
        };
        let response = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            self.0.writer.send(bytes).await?;
            Ok::<_, anyhow::Error>(response.await?)
        })
        .await;
        let message = match response {
            Ok(Ok(message)) => message,
            result => {
                // A timed-out command may already have changed playback. Retire
                // its process so a retry cannot leave an untracked session alive.
                self.0.broken.store(true, Ordering::SeqCst);
                self.0.pending.lock().expect("pending mutex").clear();
                let reason = match result {
                    Ok(Err(error)) => error.to_string(),
                    _ => "听书程序响应超时；请主动重试".into(),
                };
                let _ = self.0.events.send(Received::Disconnected(reason.clone()));
                reap(&self.0.child).await;
                return Err(anyhow::anyhow!(reason));
            }
        };
        if let Event::Error(error) = &message.event {
            return Err(anyhow::anyhow!("{}: {}", error.code, error.message));
        }
        Ok(message)
    }
    /// Graceful shutdown, then forced kill and reap after a bounded wait.
    pub async fn close(&self) {
        let deadline = tokio::time::Instant::now() + SHUTDOWN_TIMEOUT;
        let _ = tokio::time::timeout_at(deadline, self.command(None, Command::Shutdown)).await;
        self.0.broken.store(true, Ordering::SeqCst);
        let child = self.0.child.lock().expect("child mutex").take();
        if let Some(mut child) = child {
            // This task owns the child even if the caller cancels shutdown.
            let reaper = tokio::spawn(async move {
                if !matches!(
                    tokio::time::timeout_at(deadline, child.wait()).await,
                    Ok(Ok(_))
                ) {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                }
            });
            let _ = reaper.await;
        }
        for task in &self.0.readers {
            task.abort();
        }
        self.0.pending.lock().expect("pending mutex").clear();
    }
}
async fn reap(shared: &Arc<Mutex<Option<tokio::process::Child>>>) {
    let child = shared.lock().expect("child mutex").take();
    if let Some(mut child) = child {
        let reaper = tokio::spawn(async move {
            let _ = child.start_kill();
            let _ = child.wait().await;
        });
        let _ = reaper.await;
    }
}

struct PendingRequest {
    id: String,
    pending: Pending,
}
impl Drop for PendingRequest {
    fn drop(&mut self) {
        self.pending.lock().expect("pending mutex").remove(&self.id);
    }
}

#[cfg(test)]
pub(crate) mod tests;
