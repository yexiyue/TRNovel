#!/usr/bin/env bash
set -euo pipefail

package="npm/trnovel-npm-package.tar.gz"
metadata=$(tar -xOf "$package" package/package.json)
name=$(jq -r '.name' <<< "$metadata")
version=$(jq -r '.version' <<< "$metadata")
repository=$(jq -r '.repository | if type == "string" then . else .url end' <<< "$metadata")
[[ "$name" == '@trnovel/trnovel' ]]
[[ "$repository" == 'git+https://github.com/yexiyue/TRNovel.git' || "$repository" == 'https://github.com/yexiyue/TRNovel' ]]

if [[ -n "${PLAN:-}" ]]; then
  expected_version=$(jq -er '.releases[] | select(.app_name == "trnovel") | .app_version' <<< "$PLAN")
  [[ "$version" == "$expected_version" ]]
  if jq -e '.announcement_is_prerelease and (.publish_prereleases | not)' <<< "$PLAN" > /dev/null; then
    echo 'Skipping prerelease npm publication.'
    exit 0
  fi
elif [[ "${DRY_RUN:-false}" != true ]]; then
  [[ "${RELEASE_TAG:-}" == "trnovel-v$version" ]]
fi

npm publish "$package" --access public --dry-run --ignore-scripts
if [[ "${DRY_RUN:-false}" == true ]]; then
  exit 0
fi

# Leave registry errors visible; an already published version is the only safe skip.
npm view "$name" versions --json > npm/versions.json
if jq -e --arg version "$version" 'if type == "array" then index($version) != null else . == $version end' npm/versions.json > /dev/null; then
  echo "$name@$version is already published."
  exit 0
fi
npm publish "$package" --access public --ignore-scripts
