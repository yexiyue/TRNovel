#!/usr/bin/env bash
# 校验书源 JSON:在 trnovel 仓库根构建并跑 doctor 全流程体检。
# 用法: validate.sh <书源.json>
# 首次会 cargo build(慢),之后直接用 ./target/debug/trn 反复跑很快。
set -euo pipefail

file="${1:?用法: validate.sh <书源.json>}"
[ -f "$file" ] || { echo "找不到文件: $file" >&2; exit 1; }
file="$(cd "$(dirname "$file")" && pwd)/$(basename "$file")"   # 绝对化(下面要 cd)

# 定位仓库根(脚本可能经 .claude/skills 符号链接调用,用 git 最稳)
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(git -C "$script_dir" rev-parse --show-toplevel 2>/dev/null || echo "")"
[ -n "$root" ] || { echo "未找到 git 仓库根;请在 trnovel 仓库内运行,或直接: cargo run -- doctor <文件>" >&2; exit 1; }
cd "$root"

bin="./target/debug/trn"
[ -x "$bin" ] || cargo build -q
exec "$bin" doctor "$file"
