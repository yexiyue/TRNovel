#!/bin/sh
set -eu
version='@VERSION@'
base=${TRNOVEL_RELEASE_BASE_URL:-https://github.com/yexiyue/TRNovel/releases/download/trnovel-v$version}
install_dir=${TRNOVEL_INSTALL_DIR:-$HOME/.cargo/bin}
case "$(uname -s):$(uname -m)" in
  Darwin:arm64) target=aarch64-apple-darwin ;;
  Linux:x86_64) target=x86_64-unknown-linux-gnu ;;
  *) echo 'Unsupported platform; use the documented ARM64 musl archive when applicable.' >&2; exit 1 ;;
esac
if [ "$(uname -s)" = Linux ] && ! ldd --version 2>&1 | head -1 | grep -qi 'glibc\|GNU libc'; then
  echo 'This installer needs GNU libc; use the musl archive on Alpine.' >&2; exit 1
fi
asset=trnovel-basic-$target.tar.xz
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
fetch() { curl --fail --location --silent --show-error "$1" -o "$2"; }
fetch "$base/$asset" "$temporary/$asset"
fetch "$base/$asset.sha256" "$temporary/$asset.sha256"
(cd "$temporary" && if command -v sha256sum >/dev/null; then sha256sum -c "$asset.sha256"; else shasum -a 256 -c "$asset.sha256"; fi)
tar -xf "$temporary/$asset" -C "$temporary"
mkdir -p "$install_dir"
for binary in trnovel trn; do
  install -m 755 "$temporary/trnovel-basic-$target/$binary" "$install_dir/$binary"
done
printf 'Installed basic reader in %s (add this directory to PATH).\n' "$install_dir"
