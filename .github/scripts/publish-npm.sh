#!/usr/bin/env bash
set -euo pipefail

if [[ -n "${PLAN:-}" ]] && jq -e '.announcement_is_prerelease and (.publish_prereleases | not)' <<< "$PLAN" > /dev/null; then
  echo 'Skipping prerelease npm publication.'
  exit 0
fi
for variant in trnovel; do
  package="./npm/$variant-npm-package.tar.gz"
  metadata=$(tar -xOf "$package" package/package.json)
  name=$(jq -er '.name' <<< "$metadata")
  version=$(jq -er '.version' <<< "$metadata")
  repository=$(jq -er '.repository | if type == "string" then . else .url end' <<< "$metadata")
  [[ "$name" == "@trnovel/$variant" ]]
  [[ "$repository" == 'git+https://github.com/yexiyue/TRNovel.git' || "$repository" == 'https://github.com/yexiyue/TRNovel' ]]
  if [[ -n "${PLAN:-}" ]]; then
    expected_version=$(jq -er '.releases[] | select(.app_name == "trnovel") | .app_version' <<< "$PLAN")
    [[ "$version" == "$expected_version" ]]
  elif [[ "${DRY_RUN:-false}" != true ]]; then
    [[ "${RELEASE_TAG:-}" == "trnovel-v$version" ]]
  fi
  npm pack "$package" --dry-run --ignore-scripts
  if [[ "${DRY_RUN:-false}" == true ]]; then continue; fi
  # A first publication has no registry entry. Only E404 is safe to ignore.
  if npm view "$name" versions --json > npm/versions.json 2> npm/registry-error.log; then
    if jq -e --arg version "$version" 'if type == "array" then index($version) != null else . == $version end' npm/versions.json > /dev/null; then
      echo "$name@$version is already published."
      continue
    fi
  elif ! jq -e '.error.code == "E404"' npm/versions.json > /dev/null; then
    cat npm/registry-error.log >&2
    exit 1
  fi
  npm publish "$package" --access public --ignore-scripts
  available=false
  # Registry scans can delay visibility; never upload the same version twice.
  for attempt in $(seq 1 90); do
    if published_version=$(npm view "$name@$version" version --prefer-online 2>/dev/null) && [[ "$published_version" == "$version" ]]; then
      available=true
      break
    fi
    sleep 10
  done
  if [[ "$available" != true ]]; then
    echo "::error::npm accepted $name@$version; check scan/review status before retrying."
    exit 1
  fi
  echo "$name@$version is publicly available."
done
