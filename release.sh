#!/bin/bash

# 交互式选择要排除的 crate
ALL_CRATES=(parse-book-source novel-tts trnovel)
EXCLUDE_CRATES=()
echo "可选排除的 crate 列表："
select crate in "${ALL_CRATES[@]}" "全部选择完毕"; do
  if [[ $REPLY -gt 0 && $REPLY -le ${#ALL_CRATES[@]} ]]; then
    EXCLUDE_CRATES+=("$crate")
    echo "$crate 已加入排除列表。"
  elif [[ $REPLY -eq $((${#ALL_CRATES[@]}+1)) ]]; then
    break
  else
    echo "无效选择，请重新输入。"
  fi
done


EXCLUDE_ARGS=""
for crate in "${EXCLUDE_CRATES[@]}"; do
  EXCLUDE_ARGS+="--exclude $crate "
done

echo "最终排除参数: $EXCLUDE_ARGS"

cargo release version patch --workspace $EXCLUDE_ARGS --no-confirm --execute
cargo release hook --no-confirm --execute
cargo release commit --no-confirm --execute
cargo release tag --workspace $EXCLUDE_ARGS --execute --no-confirm
git push origin main --tags