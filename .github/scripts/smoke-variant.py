#!/usr/bin/env python3
"""Check installed package contents and a protocol handshake without models."""
from pathlib import Path
import json
import subprocess
import sys

variant, directory, target = sys.argv[1:4]
directory = Path(directory).resolve()
suffix = ".exe" if "windows" in target else ""
for binary in ("trnovel", "trn"):
    program = directory / (binary + suffix)
    subprocess.run([str(program), "--version"], check=True)
    result = subprocess.run([str(program), "--help"], check=True, capture_output=True, text=True)
    assert ("--tts-program" in result.stdout) == (variant == "listening")
listener = directory / ("novel-tts" + suffix)
if variant == "basic":
    assert not listener.exists()
else:
    requests = [dict(protocol_version=1, request_id=str(index), session_id=None, type=kind)
                for index, kind in enumerate(("hello", "shutdown"))]
    result = subprocess.run([str(listener), "--protocol"], input="".join(json.dumps(request)+"\n" for request in requests), capture_output=True, text=True, timeout=10, check=True)
    messages = [json.loads(line) for line in result.stdout.splitlines()]
    assert messages[0]["type"] == "ready"
    assert messages[0]["protocol_version"] == 1
    assert messages[-1]["type"] == "accepted"
if "linux" in target:
    for binary in ("trnovel", "trn"):
        result = subprocess.run(["readelf", "-d", str(directory / binary)], check=True, capture_output=True, text=True)
        lowered = result.stdout.lower()
        assert not any(name in lowered for name in ("onnxruntime", "libasound", "kokoro"))
