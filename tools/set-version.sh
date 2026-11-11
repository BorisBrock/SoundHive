#!/usr/bin/env bash
# Sets the app version in package.json, src-tauri/tauri.conf.json and src-tauri/Cargo.toml
# (and their lock files). The release workflow and the AUR package run this with the version
# from the release tag. The repository itself stays at 0.0.0, don't commit the result.
#
# Usage: tools/set-version.sh 1.2.0
set -euo pipefail
cd "$(dirname "$0")/.."

version="${1:-}"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "Usage: $0 <version>, e.g. $0 1.2.0 or $0 1.2.0-rc1" >&2
  exit 1
fi

npm version "$version" --no-git-tag-version --allow-same-version >/dev/null
sed -i -E "0,/\"version\": \"[^\"]*\"/s//\"version\": \"$version\"/" src-tauri/tauri.conf.json
sed -i -E "0,/^version = \"[^\"]*\"/s//version = \"$version\"/" src-tauri/Cargo.toml
cargo metadata --manifest-path src-tauri/Cargo.toml --format-version 1 >/dev/null

echo "Version set to $version"
