#!/usr/bin/env python3
"""Build one release variant without unifying reader and native TTS features."""
import os
from pathlib import Path
import shutil
import subprocess
import sys

variant = sys.argv[1]
if variant not in ("basic", "listening", "listening-coreml", "listening-cuda"):
    raise SystemExit("variant must be basic, listening, listening-coreml or listening-cuda")
root = Path(__file__).resolve().parents[2]
tts_root = root / "vendor" / "talechime"
target = os.environ.get("CARGO_DIST_TARGET")
if not target:
    raise SystemExit("CARGO_DIST_TARGET is required")
subprocess.run(["rustup", "target", "add", target], check=True)
build_root = root / "target" / "variants" / variant
environment = os.environ.copy()
environment["CARGO_TARGET_DIR"] = str(build_root)
if "windows-msvc" in target:
    environment["RUSTFLAGS"] = environment.get("RUSTFLAGS", "") + " -C target-feature=-crt-static"
if variant == "listening-cuda" and "linux" in target:
    environment["RUSTFLAGS"] = environment.get("RUSTFLAGS", "") + " -C link-arg=-Wl,-rpath,$ORIGIN"
command = ["cargo", "build", "--locked", "--profile", "dist", "--target", target, "--bins", "-p", "trnovel"]
if variant == "basic":
    command.append("--no-default-features")
subprocess.run(command, cwd=root, env=environment, check=True)
binaries = ["trnovel", "trn"]
if variant.startswith("listening"):
    # A separate package-specific build keeps native dependencies out of readers.
    tts_command = ["cargo", "build", "--locked", "--profile", "dist", "--target", target, "-p", "talechime", "--manifest-path", str(tts_root / "Cargo.toml")]
    if variant == "listening-coreml":
        if target != "aarch64-apple-darwin":
            raise SystemExit("CoreML distribution requires Apple Silicon")
        tts_command.extend(["--features", "coreml,metal"])
    elif variant == "listening-cuda":
        if target not in ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"):
            raise SystemExit("CUDA distribution requires x86_64 Linux or Windows")
        tts_command.extend(["--features", "ort-cuda,qwen-cuda"])
    optional = environment.get("TRNOVEL_TTS_FEATURES", "")
    if optional:
        tts_command.extend(["--features", optional])
    subprocess.run(tts_command, cwd=root, env=environment, check=True)
    binaries.append("novel-tts")
output = Path(os.environ.get("TRNOVEL_VARIANT_OUTPUT", Path.cwd()))
output.mkdir(parents=True, exist_ok=True)
for name in binaries:
    suffix = ".exe" if "windows" in target else ""
    shutil.copy2(build_root / target / "dist" / (name + suffix), output / (name + suffix))

# ORT is statically linked where supported; CUDA provider libraries stay beside the executable.
if variant.startswith("listening"):
    for library in (build_root / target / "dist").iterdir():
        if library.is_file() and (library.name.endswith((".dll", ".dylib")) or ".so" in library.name):
            shutil.copy2(library, output / library.name)

if variant == "listening-cuda" and not any("onnxruntime_providers_cuda" in path.name for path in output.iterdir()):
    raise SystemExit("CUDA distribution is missing its native ORT provider library; refusing to package a CPU fallback")

if variant.startswith("listening"):
    notices = output / "tts-notices"
    notices.mkdir(exist_ok=True)
    sources = [tts_root / "crates" / "qwen3-tts", tts_root / "crates" / "talechime-backends" / "src" / "moss" / "assets"]
    optional_features = {feature.strip() for feature in environment.get("TRNOVEL_TTS_FEATURES", "").split(",")}
    if any(feature.startswith("moss-candle") for feature in optional_features):
        sources.append(tts_root / "crates" / "moss-tts")
    if any(feature.startswith("omnivoice") for feature in optional_features):
        sources.append(tts_root / "crates" / "omnivoice")
    if any(feature.startswith("voxcpm") for feature in optional_features):
        sources.append(tts_root / "crates" / "voxcpm")
    for source in sources:
        for path in source.rglob("*"):
            if path.is_file() and (path.name.startswith(("LICENSE", "COPYING")) or path.name in ("SOURCE.md", "NOTICE")):
                destination = notices / path.relative_to(tts_root)
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, destination)
