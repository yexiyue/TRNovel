#!/bin/sh
set -eu
# First check basic readers without installing any audio runtime libraries.
apk add --no-cache ca-certificates libstdc++ libssl3 libcrypto3
mkdir -p /tmp/trnovel-musl
cd /tmp/trnovel-musl
for variant in basic listening; do
    if [ "$variant" = basic ]; then
        archive=trnovel-basic-aarch64-unknown-linux-musl
    else
        apk add --no-cache alsa-lib onnxruntime
        archive=trnovel-aarch64-unknown-linux-musl
    fi
    cp "/work/target/musl-distrib/$archive.tar.gz" .
    sha256sum -c "/work/target/musl-distrib/$archive.tar.gz.sha256"
    tar -xzf "$archive.tar.gz"
    for binary in trnovel trn; do
        "./$archive/$binary" --version
        "./$archive/$binary" --help > /dev/null
    done
    if [ "$variant" = basic ]; then
        test ! -e "$archive/novel-tts"
        if "./$archive/trnovel" --help | grep -q -- '--tts-program'; then exit 1; fi
    else
        printf '%s\n' '{"protocol_version":1,"request_id":"hello","session_id":null,"type":"hello"}' '{"protocol_version":1,"request_id":"bye","session_id":null,"type":"shutdown"}' | "./$archive/novel-tts" --protocol > replies.jsonl
        grep -q '"type":"ready"' replies.jsonl
    fi
done
apk add --no-cache python3
python3 - <<'PY'
import ctypes
class ApiBase(ctypes.Structure):
    _fields_ = [("get_api", ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_uint32)), ("get_version", ctypes.CFUNCTYPE(ctypes.c_char_p))]
runtime = ctypes.CDLL("libonnxruntime.so.1")
runtime.OrtGetApiBase.restype = ctypes.POINTER(ApiBase)
base = runtime.OrtGetApiBase().contents
assert base.get_api(22), "ort rc.10 requires ONNX Runtime API 22"
print("ONNX Runtime:", base.get_version().decode(), "(API 22 available)")
PY
