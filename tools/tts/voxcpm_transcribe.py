"""Development-only segment transcription; differences require listening review."""
import argparse
import difflib
import json
import re
from pathlib import Path

from faster_whisper import WhisperModel


def normalize(text):
    return re.sub(r"[\W_]+", "", text).lower()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--model", default="small")
    args = parser.parse_args()
    model = WhisperModel(args.model, device="cpu", compute_type="int8", cpu_threads=2)
    index = json.loads((args.directory / "index.json").read_text(encoding="utf-8"))
    results = []
    for item in index:
        segments, _ = model.transcribe(str(args.directory / item["file"]), language="zh", beam_size=5)
        transcript = "".join(segment.text for segment in segments)
        source, recognized = normalize(item["text"]), normalize(transcript)
        differences = [
            {"operation": operation, "source": source[a:b], "transcript": recognized[c:d]}
            for operation, a, b, c, d in difflib.SequenceMatcher(None, source, recognized).get_opcodes()
            if operation != "equal"
        ]
        results.append({**item, "transcript": transcript, "differences": differences})
        print(f'{item["file"]}: {transcript}', flush=True)
    (args.directory / "transcription.json").write_text(
        json.dumps({"model": args.model, "normalization": "punctuation removed; script/number differences retained", "segments": results}, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
