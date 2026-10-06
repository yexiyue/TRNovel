use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command as ProcessCommand, Stdio},
};
use tts_protocol::{Command, ConfigPatch, Event, Message, PROTOCOL_VERSION, Request, encode};

struct Worker {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    _directory: tempfile::TempDir,
}
impl Worker {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut child = ProcessCommand::new(env!("CARGO_BIN_EXE_novel-tts"))
            .args(["--protocol", "--config"])
            .arg(directory.path().join("config.json"))
            .arg("--model-dir")
            .arg(directory.path().join("models"))
            .arg("--checkpoint-dir")
            .arg(directory.path().join("positions"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            _directory: directory,
        }
    }
    fn send(&mut self, id: &str, command: Command) -> Message {
        let request = Request {
            protocol_version: PROTOCOL_VERSION,
            request_id: id.into(),
            session_id: None,
            command,
        };
        self.input
            .as_mut()
            .unwrap()
            .write_all(&encode(&request).unwrap())
            .unwrap();
        self.next()
    }
    fn next(&mut self) -> Message {
        let mut line = String::new();
        assert!(self.output.read_line(&mut line).unwrap() > 0);
        serde_json::from_str(&line).expect("each stdout line must be a protocol message")
    }
    fn wait(&mut self) {
        for _ in 0..100 {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("worker failed to exit after EOF/shutdown");
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn query_and_update_do_not_prepare_resources_and_shutdown_drains_json() {
    let mut worker = Worker::new();
    let ready = worker.send("hello", Command::Hello);
    assert!(matches!(ready.event, Event::Ready(_)));
    assert_eq!(ready.request_id.as_deref(), Some("hello"));
    let original = worker.send("config", Command::GetConfig);
    assert!(matches!(original.event, Event::Config(_)));
    let changed = worker.send(
        "update",
        Command::UpdateConfig(ConfigPatch {
            volume: Some(0.7),
            ..Default::default()
        }),
    );
    assert!(
        matches!(changed.event,Event::ConfigChanged(ref config) if config.volume == 0.7 && config.revision == 1)
    );
    assert_eq!(ready.instance_id, changed.instance_id);
    assert!(changed.sequence > original.sequence);
    assert!(!worker._directory.path().join("models").exists());
    let conflict = worker.send("conflict", Command::UpdateConfig(ConfigPatch::default()));
    assert!(matches!(conflict.event,Event::Error(ref error) if error.code == "revision_conflict"));
    assert!(matches!(
        worker.send("end", Command::Shutdown).event,
        Event::Accepted
    ));
    worker.wait();
}

#[test]
fn eof_duplicate_invalid_json_and_incompatible_version() {
    let mut worker = Worker::new();
    worker
        .input
        .as_mut()
        .unwrap()
        .write_all(b"not-json\n")
        .unwrap();
    assert!(matches!(worker.next().event, Event::Error(_)));
    assert!(matches!(
        worker.send("hello", Command::Hello).event,
        Event::Ready(_)
    ));
    assert!(matches!(
        worker.send("hello", Command::Hello).event,
        Event::Error(_)
    ));
    worker.input.take();
    worker.wait();
    let mut worker = Worker::new();
    let request = Request {
        protocol_version: 99,
        request_id: "future".into(),
        session_id: None,
        command: Command::Hello,
    };
    worker
        .input
        .as_mut()
        .unwrap()
        .write_all(&encode(&request).unwrap())
        .unwrap();
    assert!(
        matches!(worker.next().event,Event::Error(ref error) if error.code == "incompatible_version")
    );
    worker.wait();
}

#[test]
fn invalid_utf8_and_missing_files_exit_before_preparation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invalid.txt");
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    for path in [path, directory.path().join("missing.txt")] {
        let output = ProcessCommand::new(env!("CARGO_BIN_EXE_novel-tts"))
            .arg(path)
            .arg("--model-dir")
            .arg(directory.path().join("models"))
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read UTF-8 file"));
        assert!(output.stdout.is_empty());
    }
    assert!(!directory.path().join("models").exists());
}
