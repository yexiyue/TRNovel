#!/bin/sh
set -eu

# No compiler or development packages: exercise the shipped archive with
# exactly the runtime dependencies documented for users.
apk add --no-cache ca-certificates alsa-lib libstdc++ onnxruntime
mkdir /tmp/trnovel-musl
cd /tmp/trnovel-musl
cp /work/target/musl-distrib/trnovel-aarch64-unknown-linux-musl.tar.gz .
sha256sum -c /work/target/musl-distrib/trnovel-aarch64-unknown-linux-musl.tar.gz.sha256
tar -xzf /work/target/musl-distrib/trnovel-aarch64-unknown-linux-musl.tar.gz
cd trnovel-aarch64-unknown-linux-musl
for binary in trnovel trn; do
    "./$binary" --version
    "./$binary" --help > /dev/null
    # Import exercises parsing, file I/O and persistent state beyond clap.
    printf '[]\n' > /tmp/book-sources.json
    "./$binary" import /tmp/book-sources.json
done
