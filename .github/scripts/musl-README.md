# TRNovel for ARM64 musl Linux

Both variants use dynamic musl linking on Alpine 3.23.

The basic archive contains only `trnovel` and `trn`. Install:

```sh
apk add --no-cache ca-certificates libstdc++ libssl3 libcrypto3
```

The listening archive also contains `novel-tts`. Keep all three binaries in
the same directory and additionally install:

```sh
apk add --no-cache alsa-lib onnxruntime
```

Only the listening worker loads ONNX Runtime/ALSA. It requires a working audio
device and the separately downloaded Kokoro model files. Other musl systems
must provide ONNX Runtime API 22 (1.22 or newer) and compatible shared libraries.
Browser-assisted sources still require a system browser.

Download these archives directly from GitHub Releases; shell, npm and Homebrew
installers do not select the custom musl artifacts. Containers are checked in
CI. Local Docker validation remains pending when the daemon is unavailable.
