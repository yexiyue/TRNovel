#!/bin/sh
set -eu
# Native Alpine build; readers and worker have separate Cargo feature graphs.
apk add --no-cache build-base pkgconf alsa-lib-dev onnxruntime-dev openssl-dev binutils
export ORT_LIB_LOCATION=/usr/lib ORT_PREFER_DYNAMIC_LINK=1 ORT_SKIP_DOWNLOAD=1
export RUSTFLAGS='-C target-feature=-crt-static'
target=aarch64-unknown-linux-musl
rustc -vV | grep -q "host: $target"
output=target/musl-distrib
mkdir -p "$output"
for variant in basic listening; do
    export CARGO_TARGET_DIR="/work/target/musl-build/$variant"
    if [ "$variant" = basic ]; then
        cargo build --locked --profile dist --bins -p trnovel --no-default-features
        archive="trnovel-basic-$target"
    else
        cargo build --locked --profile dist --bins -p trnovel
        cargo build --locked --profile dist -p novel-tts
        archive="trnovel-$target"
    fi
    mkdir -p "$output/$archive"
    binaries='trnovel trn'
    if [ "$variant" = listening ]; then binaries="$binaries novel-tts"; fi
    for binary in $binaries; do
        source="$CARGO_TARGET_DIR/dist/$binary"
        readelf -h "$source" | grep -q 'Machine:.*AArch64'
        readelf -l "$source" | grep -q '/lib/ld-musl-aarch64.so.1'
        if [ "$binary" != novel-tts ] && readelf -d "$source" | grep -Ei 'NEEDED.*(onnxruntime|asound)'; then
            echo 'Reader unexpectedly links native listening libraries' >&2
            exit 1
        fi
        install -m 755 "$source" "$output/$archive/$binary"
    done
    cp LICENSE "$output/$archive/"
    cp .github/scripts/musl-README.md "$output/$archive/README.md"
    tar -czf "$output/$archive.tar.gz" -C "$output" "$archive"
    (cd "$output" && sha256sum "$archive.tar.gz" > "$archive.tar.gz.sha256")
done
