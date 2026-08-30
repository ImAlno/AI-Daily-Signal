#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

mode_output="$(mktemp -d)"
trap 'rm -rf "$mode_output"' EXIT

env -u SIGNAL_SWIFT_CLT_TESTING \
  swift package dump-package --package-path apps/macos >"$mode_output/normal.json"
if rg -q '/Library/Developer/CommandLineTools' "$mode_output/normal.json"; then
  echo "normal package mode unexpectedly contains Command Line Tools paths" >&2
  exit 1
fi

SIGNAL_SWIFT_CLT_TESTING=1 \
  swift package dump-package --package-path apps/macos >"$mode_output/clt.json"
if ! rg -q '/Library/Developer/CommandLineTools' "$mode_output/clt.json"; then
  echo "explicit CLT test mode is missing Command Line Tools paths" >&2
  exit 1
fi

echo "Swift package modes are isolated"
