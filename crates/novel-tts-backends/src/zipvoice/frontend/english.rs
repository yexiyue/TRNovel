//! Upstream eSpeak clause semantics, executed in a separate native helper.
use std::{
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};
use unicode_normalization::UnicodeNormalization;
const BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/zipvoice-phonemizer.bin"));
const DATA: &[u8] = include_bytes!("../../../native/zipvoice-phonemizer/espeak-data.bin");
const MANIFEST: &str = include_str!("../../../native/zipvoice-phonemizer/data-manifest.json");

pub fn prepare(directory: &Path) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(directory)?;
    std::fs::write(
        directory.join("LICENSE"),
        include_bytes!("../../../native/zipvoice-phonemizer/espeak/COPYING"),
    )?;
    std::fs::write(
        directory.join("SOURCE.md"),
        include_bytes!("../../../native/zipvoice-phonemizer/SOURCE.md"),
    )?;
    let binary = directory.join(if cfg!(windows) {
        "phonemizer.exe"
    } else {
        "phonemizer"
    });
    if !binary.exists() || std::fs::read(&binary)? != BINARY {
        let mut temp = tempfile::NamedTempFile::new_in(directory)?;
        temp.write_all(BINARY)?;
        temp.persist(&binary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    let resources: Vec<crate::resources::Resource> = serde_json::from_str(MANIFEST)?;
    let valid = resources.iter().all(|resource| {
        crate::resources::verify(
            &directory.join("espeak-ng-data").join(&resource.path),
            resource,
        )
        .is_ok()
    });
    if !valid {
        let mut input = std::io::Cursor::new(DATA);
        fn u32(input: &mut impl Read) -> std::io::Result<u32> {
            let mut bytes = [0; 4];
            input.read_exact(&mut bytes)?;
            Ok(u32::from_le_bytes(bytes))
        }
        fn u64(input: &mut impl Read) -> std::io::Result<u64> {
            let mut bytes = [0; 8];
            input.read_exact(&mut bytes)?;
            Ok(u64::from_le_bytes(bytes))
        }
        let count = u32(&mut input)?;
        for _ in 0..count {
            let name_len = u32(&mut input)? as usize;
            let data_len = u64(&mut input)? as usize;
            anyhow::ensure!(
                name_len <= 1024 && data_len <= DATA.len(),
                "invalid embedded phonemizer data"
            );
            let mut name = vec![0; name_len];
            input.read_exact(&mut name)?;
            let name = String::from_utf8(name)?;
            anyhow::ensure!(
                Path::new(&name)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_))),
                "invalid phonemizer data path"
            );
            let path = directory.join("espeak-ng-data").join(name);
            let mut data = vec![0; data_len];
            input.read_exact(&mut data)?;
            std::fs::create_dir_all(path.parent().expect("data parent"))?;
            let mut temp = tempfile::NamedTempFile::new_in(path.parent().expect("data parent"))?;
            temp.write_all(&data)?;
            temp.persist(path)?;
        }
        for resource in &resources {
            crate::resources::verify(
                &directory.join("espeak-ng-data").join(&resource.path),
                resource,
            )?;
        }
    }
    Ok(binary)
}

pub fn phonemes(binary: &Path, text: &str) -> anyhow::Result<Vec<String>> {
    anyhow::ensure!(
        text.len() <= 16384 && !text.contains('\0'),
        "invalid English reference text"
    );
    let mut command = Command::new(binary);
    command
        .current_dir(binary.parent().expect("helper directory"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn()?;
    let mut stdin = child.stdin.take().expect("piped helper input");
    // Drain output while sending input, so a long reference cannot fill both pipes.
    let bytes = text.as_bytes().to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&bytes));
    let output = child.wait_with_output()?;
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("phonemizer input thread exited"))??;
    anyhow::ensure!(
        output.status.success(),
        "English phonemizer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut tokens = Vec::new();
    for line in std::str::from_utf8(&output.stdout)?.lines() {
        let Some((terminator, phones)) = line.split_once('\t') else {
            anyhow::bail!("invalid phonemizer output");
        };
        let mut in_flag = false;
        for phone in phones.trim_end_matches('\r').nfd() {
            if phone == '(' {
                in_flag = true;
            } else if phone == ')' && in_flag {
                in_flag = false;
            } else if !in_flag {
                tokens.push(phone.to_string());
            }
        }
        match terminator.parse::<u32>()? & 0x000fffff {
            0x80028 => tokens.push(".".into()),
            0x82028 => tokens.push("?".into()),
            0x8302d => tokens.push("!".into()),
            0x41014 => tokens.extend([",".into(), " ".into()]),
            0x4001e => tokens.extend([":".into(), " ".into()]),
            0x4101e => tokens.extend([";".into(), " ".into()]),
            _ => {}
        }
    }
    Ok(tokens)
}
