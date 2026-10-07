//! Exact upstream user message template and reference audio placeholders.
use crate::config::SpeechConfig;
use tokenizers::Tokenizer;

pub fn text_prompt(
    tokenizer: &Tokenizer,
    config: &SpeechConfig,
    text: &str,
    instruction: Option<&str>,
    reference: Option<&[Vec<u32>]>,
) -> anyhow::Result<Vec<Vec<u32>>> {
    anyhow::ensure!(!text.trim().is_empty(), "empty MOSS input");
    let reference_marker = if reference.is_some() {
        "[S1]:\n<|audio|>"
    } else {
        "None"
    };
    let user = format!(
        "<user_inst>\n- Reference(s):\n{reference_marker}\n- Instruction:\n{}\n- Tokens:\nNone\n- Quality:\nNone\n- Sound Event:\nNone\n- Ambient Sound:\nNone\n- Language:\nNone\n- Text:\n{text}\n</user_inst>",
        instruction.unwrap_or("None")
    );
    let user = format!("<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n");
    let mut rows = Vec::new();
    let mut encode = |text: &str| -> anyhow::Result<()> {
        for &token in tokenizer
            .encode(text, false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .get_ids()
        {
            let mut row = vec![config.audio_pad_code; config.n_vq + 1];
            row[0] = token;
            rows.push(row);
        }
        Ok(())
    };
    if let Some(reference) = reference {
        let (before, after) = user
            .split_once("<|audio|>")
            .expect("constructed placeholder");
        encode(before)?;
        let mut row = vec![config.audio_pad_code; config.n_vq + 1];
        row[0] = config.audio_start_token_id;
        rows.push(row);
        for frame in reference {
            anyhow::ensure!(
                frame.len() == config.n_vq && frame.iter().all(|id| *id < config.audio_pad_code),
                "incompatible MOSS reference codes"
            );
            let mut row = vec![config.audio_user_slot_token_id];
            row.extend_from_slice(frame);
            rows.push(row);
        }
        let mut row = vec![config.audio_pad_code; config.n_vq + 1];
        row[0] = config.audio_end_token_id;
        rows.push(row);
        for &token in tokenizer
            .encode(after, false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .get_ids()
        {
            let mut row = vec![config.audio_pad_code; config.n_vq + 1];
            row[0] = token;
            rows.push(row);
        }
    } else {
        encode(&user)?;
    }
    let mut row = vec![config.audio_pad_code; config.n_vq + 1];
    row[0] = config.audio_start_token_id;
    rows.push(row);
    Ok(rows)
}
