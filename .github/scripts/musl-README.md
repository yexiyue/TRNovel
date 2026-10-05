# TRNovel for ARM64 musl Linux

This archive contains `trnovel` and `trn` built natively for
`aarch64-unknown-linux-musl`, including Kokoro TTS support.

It uses dynamic musl linking. It is not a fully static executable. On Alpine
Linux 3.23, install the runtime libraries first:

```sh
apk add --no-cache ca-certificates alsa-lib libstdc++ onnxruntime libssl3 libcrypto3
```

Keep these libraries installed, then put `trnovel` and `trn` in your PATH.
Listening requires a working audio device and the usual downloaded Kokoro
model/voice files. Browser-assisted book sources still need a system browser.

Other musl distributions must provide compatible libraries, including
ONNX Runtime 1.22 or newer. The CI runtime check uses Alpine 3.23.

Download this archive directly from GitHub Releases. The cargo-dist shell,
npm and Homebrew installers do not currently select this custom artifact.
