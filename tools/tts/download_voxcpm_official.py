"""Development-only download of pinned original VoxCPM2 weights, with resume/hash checks."""
import hashlib
import json
import subprocess
import time
from pathlib import Path

REVISION = "32279effe8c19989596f05d353d1447f51d9e915"
ROOT = Path.home() / ".novel-tts/voxcpm/models/2b-bf16" / REVISION


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verified(path, entry):
    if not path.exists() or path.stat().st_size != entry["size"]:
        return False
    if "lfs" in entry:
        return digest(path) == entry["lfs"]["sha256"]
    data = path.read_bytes()
    return hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest() == entry["blobId"]


def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    url = f"https://huggingface.co/api/models/openbmb/VoxCPM2/revision/{REVISION}?blobs=true"
    metadata = json.loads(subprocess.check_output([
        "curl.exe", "--location", "--fail", "--silent", "--show-error", "--retry", "5",
        "--retry-all-errors", "--connect-timeout", "30", "--max-time", "120", url]))
    assert metadata["sha"] == REVISION
    manifest = {"repository": "openbmb/VoxCPM2", "revision": REVISION,
                "license": "Apache-2.0", "files": []}
    for entry in sorted(metadata["siblings"], key=lambda item: item["size"]):
        name = entry["rfilename"]
        if name == ".gitattributes":
            continue
        path = ROOT / name
        if verified(path, entry):
            print(f"verified {name}", flush=True)
            manifest["files"].append({"file": name, "size": entry["size"], "sha256": digest(path)})
            continue
        partial = path.with_suffix(path.suffix + ".partial")
        for attempt in range(15):
            try:
                if not verified(partial, entry):
                    if partial.exists() and partial.stat().st_size >= entry["size"]:
                        partial.unlink()
                    result = subprocess.run([
                        "curl.exe", "--location", "--fail", "--silent", "--show-error",
                        "--http1.1", "--connect-timeout", "30", "--max-time", "1800",
                        "--speed-time", "30", "--speed-limit", "1024", "--continue-at", "-",
                        "--output", str(partial),
                        f"https://huggingface.co/openbmb/VoxCPM2/resolve/{REVISION}/{name}"])
                    if not verified(partial, entry):
                        raise RuntimeError(f"incomplete or corrupt {name}, curl status {result.returncode}")
                actual = digest(partial)
                partial.replace(path)
                manifest["files"].append({"file": name, "size": entry["size"], "sha256": actual})
                print(f"verified {name} {actual}", flush=True)
                break
            except Exception as error:
                if attempt == 14:
                    raise
                print(f"retry {name}: {error}", flush=True)
                time.sleep(2)
    (ROOT / "source-manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(ROOT, flush=True)


if __name__ == "__main__":
    main()
