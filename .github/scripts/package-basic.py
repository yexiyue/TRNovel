#!/usr/bin/env python3
"""Archive the basic reader, verifying that listening binaries are absent."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import zipfile

root = Path(__file__).resolve().parents[2]
target = os.environ["CARGO_DIST_TARGET"]
suffix = ".exe" if "windows" in target else ""
source = root / "target" / "basic-stage" / target
output = root / "target" / "basic-distrib"
output.mkdir(parents=True, exist_ok=True)
name = f"trnovel-basic-{target}"
with tempfile.TemporaryDirectory() as temporary:
    stage = Path(temporary) / name
    stage.mkdir()
    for binary in ("trnovel", "trn"):
        shutil.copy2(source / (binary + suffix), stage / (binary + suffix))
    shutil.copy2(root / "LICENSE", stage / "LICENSE")
    shutil.copy2(root / "README.md", stage / "README.md")
    assert not list(stage.glob("novel-tts*"))
    if "windows" in target:
        archive = output / f"{name}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zipped:
            for path in stage.iterdir():
                zipped.write(path, f"{name}/{path.name}")
    else:
        archive = output / f"{name}.tar.xz"
        with tarfile.open(archive, "w:xz") as packed:
            packed.add(stage, arcname=name)
    checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(f"{checksum}  {archive.name}\n")
print(archive)
