#!/usr/bin/env python3
"""Build one release variant without unifying reader and native TTS features."""
import os
from pathlib import Path
import shutil
import subprocess
import sys

variant = sys.argv[1]
if variant not in ("basic", "listening"):
    raise SystemExit("variant must be basic or listening")
root = Path(__file__).resolve().parents[2]
target = os.environ.get("CARGO_DIST_TARGET")
if not target:
    raise SystemExit("CARGO_DIST_TARGET is required")
subprocess.run(["rustup", "target", "add", target], check=True)
build_root = root / "target" / "variants" / variant
environment = os.environ.copy()
environment["CARGO_TARGET_DIR"] = str(build_root)
if "windows-msvc" in target:
    environment["RUSTFLAGS"] = environment.get("RUSTFLAGS", "") + " -C target-feature=-crt-static"
command = ["cargo", "build", "--locked", "--profile", "dist", "--target", target, "--bins", "-p", "trnovel"]
if variant == "basic":
    command.append("--no-default-features")
subprocess.run(command, cwd=root, env=environment, check=True)
binaries = ["trnovel", "trn"]
if variant == "listening":
    # A separate package-specific build keeps native dependencies out of readers.
    subprocess.run(["cargo", "build", "--locked", "--profile", "dist", "--target", target, "-p", "novel-tts"], cwd=root, env=environment, check=True)
    binaries.append("novel-tts")
output = Path(os.environ.get("TRNOVEL_VARIANT_OUTPUT", Path.cwd()))
output.mkdir(parents=True, exist_ok=True)
for name in binaries:
    suffix = ".exe" if "windows" in target else ""
    shutil.copy2(build_root / target / "dist" / (name + suffix), output / (name + suffix))
