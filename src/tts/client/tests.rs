use super::*;
use std::sync::OnceLock;

pub(crate) fn fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    static COMPILED: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    let (_, binary) = COMPILED.get_or_init(|| {
        // Compile on the same filesystem as the per-test temporary links.
        let directory = tempfile::tempdir().unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/tts/client/fixture.rs");
        let binary = directory
            .path()
            .join(format!("worker{}", std::env::consts::EXE_SUFFIX));
        assert!(
            std::process::Command::new("rustc")
                .env(
                    "FIXTURE_PROTOCOL_VERSION",
                    tts_protocol::PROTOCOL_VERSION.to_string()
                )
                .arg(source)
                .arg("--edition=2024")
                .arg("-o")
                .arg(&binary)
                .status()
                .unwrap()
                .success()
        );
        (directory, binary)
    });
    let directory = tempfile::Builder::new()
        .prefix("tts path with spaces ")
        .tempdir()
        .unwrap();
    let program = directory
        .path()
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    // Keep the compiled inode read-only while parallel Unix tests spawn it.
    // Copying executables can race fork/exec with a writable descriptor from
    // another test and intermittently fail with ETXTBSY.
    #[cfg(unix)]
    std::fs::hard_link(binary, &program).unwrap();
    #[cfg(not(unix))]
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
fn discover_obeys_cli_config_path_precedence_and_reports_missing() {
    let (directory, program) = fixture("talechime");
    let (_other, configured) = fixture("configured");
    let path = std::env::join_paths([directory.path()]).unwrap();
    let missing = directory.path().join("missing");
    assert_eq!(
        discover(Some(&program), Some(&configured), Some(&path)).unwrap(),
        program
    );
    assert_eq!(
        discover(None, Some(&configured), Some(&path)).unwrap(),
        configured
    );
    assert_eq!(discover(None, None, Some(&path)).unwrap(), program);
    assert!(discover(Some(&missing), Some(&configured), Some(&path)).is_err());
    assert!(discover(None, Some(&missing), Some(&path)).is_err());
    let (legacy_directory, _legacy_program) = fixture("novel-tts");
    let legacy_path = std::env::join_paths([legacy_directory.path()]).unwrap();
    assert!(discover(None, None, Some(&legacy_path)).is_err());
    assert!(
        discover(None, None, None)
            .unwrap_err()
            .to_string()
            .contains("https://github.com/yexiyue/talechime")
    );
}

#[tokio::test]
async fn framing_rejects_unterminated_and_oversized_worker_output() {
    let mut reader = BufReader::new(&b"unterminated"[..]);
    assert!(read_line(&mut reader).await.is_err());
    let bytes = vec![b'x'; MAX_MESSAGE_BYTES + 3];
    let mut reader = BufReader::new(bytes.as_slice());
    assert!(read_line(&mut reader).await.is_err());
}
