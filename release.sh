#!/usr/bin/env bash
set -eo pipefail

# 用法:./release.sh [level] [exclude-crate ...]
#   level        : patch | minor | major | 具体版本号(如 0.13.0),默认 patch
#   exclude-crate: 本次不升级/不打标签的 crate;不传则进入交互式选择
# 例:
#   ./release.sh minor                                  # 交互式选择排除项
#   ./release.sh 0.13.0 parse-book-source novel-tts     # 非交互,只发 trnovel
#
# 标签 trnovel-v<version> 触发 .github/workflows/trnovel-release.yml(cargo-dist)
# 出多平台二进制 + npm/homebrew;库 crate 的 <crate>-v* 标签不触发该工作流。

LEVEL="${1:-patch}"
if [ "$#" -gt 0 ]; then shift; fi
echo "版本升级级别: $LEVEL"

ALL_CRATES=(parse-book-source novel-tts trnovel)
EXCLUDE_CRATES=("$@")

# 未通过参数传排除项 → 交互式选择
if [ "${#EXCLUDE_CRATES[@]}" -eq 0 ]; then
  echo "选择要排除的 crate(逐个选,选「完成」结束):"
  select crate in "${ALL_CRATES[@]}" "完成"; do
    if [[ "$REPLY" -ge 1 && "$REPLY" -le "${#ALL_CRATES[@]}" ]]; then
      EXCLUDE_CRATES+=("$crate")
      echo "已排除: $crate"
    elif [[ "$REPLY" -eq $(("${#ALL_CRATES[@]}" + 1)) ]]; then
      break
    else
      echo "无效选择,请重新输入。"
    fi
  done
fi

EXCLUDE_ARGS=()
for crate in "${EXCLUDE_CRATES[@]}"; do
  EXCLUDE_ARGS+=(--exclude "$crate")
done
echo "排除参数: ${EXCLUDE_ARGS[*]:-（无）}"

cargo release version "$LEVEL" --workspace "${EXCLUDE_ARGS[@]}" --no-confirm --execute
cargo release hook --no-confirm --execute
cargo release commit --no-confirm --execute
cargo release tag --workspace "${EXCLUDE_ARGS[@]}" --execute --no-confirm
git push origin main --tags
