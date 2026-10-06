use super::*;
use std::sync::OnceLock;

pub(crate) fn fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    static COMPILED: OnceLock<PathBuf> = OnceLock::new();
    let binary = COMPILED.get_or_init(|| {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tts-client-fixture");
        std::fs::create_dir_all(&directory).unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/tts/client/fixture.rs");
        let binary = directory.join(format!("worker{}", std::env::consts::EXE_SUFFIX));
        assert!(
            std::process::Command::new("rustc")
                .arg(source)
                .arg("--edition=2024")
                .arg("-o")
                .arg(&binary)
                .status()
                .unwrap()
                .success()
        );
        binary
    });
    let directory = tempfile::Builder::new()
        .prefix("tts path with spaces ")
        .tempdir()
        .unwrap();
    let program = directory
        .path()
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(binary, &program).unwrap();
    (directory, program)
}

#[tokio::test]
async fn pipes_correlate_reuse_and_bound_large_stderr_with_space_paths() {
    let (_directory, program) = fixture("logs");
    let (client, ready) = Client::connect(&program).await.unwrap();
    let first = client.command(None, Command::GetConfig).await.unwrap();
    let second = client.command(None, Command::GetStatus).await.unwrap();
    assert_eq!(first.instance_id, ready.instance_id);
    assert_eq!(first.instance_id, second.instance_id);
    assert!(second.sequence > first.sequence);
    assert!(client.logs().len() <= LOG_BYTES);
    assert!(!client.logs().is_empty());
    client.close().await;
    assert!(!client.is_connected());
    assert!(client.0.child.lock().unwrap().is_none());
}

#[tokio::test]
async fn incompatible_and_timed_out_workers_are_reaped() {
    for mode in ["incompatible", "timeout"] {
        let (_directory, program) = fixture(mode);
        assert!(Client::connect(&program).await.is_err());
    }
}

#[tokio::test]
async fn crash_requires_an_explicit_new_connection() {
    let (_directory, program) = fixture("crash");
    let (client, _) = Client::connect(&program).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while client.is_connected() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(client.command(None, Command::GetConfig).await.is_err());
    client.close().await;
}

#[test]
fn explicit_missing_path_fails_without_falling_back() {
    let (_directory, program) = fixture("novel-tts");
    let executable = program.with_file_name("trnovel");
    assert!(discover(Some(&program.with_file_name("missing")), &executable, None).is_err());
    assert_eq!(discover(None, &executable, None).unwrap(), program);
}

#[tokio::test]
async fn framing_rejects_unterminated_and_oversized_worker_output() {
    let mut reader = BufReader::new(&b"unterminated"[..]);
    assert!(read_line(&mut reader).await.is_err());
    let bytes = vec![b'x'; MAX_MESSAGE_BYTES + 3];
    let mut reader = BufReader::new(bytes.as_slice());
    assert!(read_line(&mut reader).await.is_err());
}
