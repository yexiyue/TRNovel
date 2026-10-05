#!/bin/sh
set -eu

# Run in the native ARM64 rust:1-alpine3.23 container. ONNX Runtime's
# upstream prebuilt GNU libraries cannot be linked into a musl executable.
apk add --no-cache build-base pkgconf alsa-lib-dev onnxruntime-dev binutils
export ORT_LIB_LOCATION=/usr/lib
export ORT_PREFER_DYNAMIC_LINK=1
export ORT_SKIP_DOWNLOAD=1
# ALSA and Alpine's ONNX Runtime are shared libraries, so disable Rust's
# default static musl CRT. Keep the pinned ort/kokoro versions unchanged.
export RUSTFLAGS='-C target-feature=-crt-static'

target=aarch64-unknown-linux-musl
cargo build --locked --profile dist --target "$target" --bins -p trnovel

output=target/musl-distrib
archive=trnovel-$target
mkdir -p "$output/$archive"
for binary in trnovel trn; do
    source=target/$target/dist/$binary
    readelf -h "$source" | grep -q 'Machine:.*AArch64'
    readelf -l "$source" | grep -q '/lib/ld-musl-aarch64.so.1'
    install -m 755 "$source" "$output/$archive/$binary"
done
cp LICENSE "$output/$archive/"
cp .github/scripts/musl-README.md "$output/$archive/README.md"
tar -czf "$output/$archive.tar.gz" -C "$output" "$archive"
(cd "$output" && sha256sum "$archive.tar.gz" > "$archive.tar.gz.sha256")
