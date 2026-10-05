#!/bin/sh
set -eu

# No compiler or development packages: exercise the shipped archive with
# exactly the runtime dependencies documented for users.
apk add --no-cache ca-certificates alsa-lib libstdc++ onnxruntime
mkdir /tmp/trnovel-musl
cd /tmp/trnovel-musl
cp /work/target/musl-distrib/trnovel-aarch64-unknown-linux-musl.tar.gz .
sha256sum -c /work/target/musl-distrib/trnovel-aarch64-unknown-linux-musl.tar.gz.sha256
tar -xzf trnovel-aarch64-unknown-linux-musl.tar.gz
cd trnovel-aarch64-unknown-linux-musl
cat > /tmp/book-sources.json <<'JSON'
[{
  "schema": "trnovel-booksource/v2",
  "name": "musl-smoke-test",
  "url": "https://example.com",
  "bookInfo": {},
  "toc": {"list": {}, "name": {}, "url": {}},
  "content": {"value": {}}
}]
JSON
for binary in trnovel trn; do
    export HOME=/tmp/home-$binary
    mkdir -p "$HOME"
    "./$binary" --version
    "./$binary" --help > /dev/null
    # import reports errors without a failing exit status: verify persistence.
    "./$binary" import /tmp/book-sources.json
    grep -q 'musl-smoke-test' "$HOME/.novel/book_sources.json"
done

# Probe the native API used by the pinned ort rc.10. Python is only a test
# helper, installed after checking the actual application runtime packages.
apk add --no-cache python3
python3 - <<'PY'
import ctypes

class ApiBase(ctypes.Structure):
    _fields_ = [
        ("get_api", ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_uint32)),
        ("get_version", ctypes.CFUNCTYPE(ctypes.c_char_p)),
    ]

runtime = ctypes.CDLL("libonnxruntime.so.1")
runtime.OrtGetApiBase.restype = ctypes.POINTER(ApiBase)
base = runtime.OrtGetApiBase().contents
assert base.get_api(22), "ort rc.10 requires ONNX Runtime API 22"
print("ONNX Runtime:", base.get_version().decode(), "(API 22 available)")
PY
