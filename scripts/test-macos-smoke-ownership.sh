#!/bin/bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd -P)"
bundle="$repository_root/target/macos/AI Daily Signal.app"
executable="$bundle/Contents/MacOS/AI Daily Signal"
test_root="$(mktemp -d)"
existing_pid=""

cleanup() {
  if [[ -n "$existing_pid" ]] && kill -0 "$existing_pid" 2>/dev/null; then
    kill -TERM "$existing_pid" 2>/dev/null || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      [[ "$(ps -p "$existing_pid" -o command= 2>/dev/null)" == "$executable" ]] || break
      sleep 0.1
    done
    if [[ "$(ps -p "$existing_pid" -o command= 2>/dev/null)" == "$executable" ]]; then
      kill -KILL "$existing_pid" 2>/dev/null || true
    fi
    wait "$existing_pid" 2>/dev/null || true
  fi
  rm -rf "$test_root"
}
trap cleanup EXIT

[[ -x "$executable" ]] || {
  echo "ownership test requires a built bundle" >&2
  exit 1
}

mkdir -p "$test_root/home"
env -u SIGNAL_HOME -u DYLD_LIBRARY_PATH -u DYLD_FRAMEWORK_PATH -u DYLD_INSERT_LIBRARIES \
  HOME="$test_root/home" PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
  "$executable" >"$test_root/existing.stdout" 2>"$test_root/existing.stderr" &
existing_pid=$!

for _ in 1 2 3 4 5 6 7 8 9 10; do
  kill -0 "$existing_pid" 2>/dev/null && break
  sleep 0.1
done
kill -0 "$existing_pid" 2>/dev/null || {
  echo "could not establish the pre-existing exact bundle process" >&2
  exit 1
}

status=0
"$repository_root/scripts/smoke-test-macos-app.sh" \
  >"$test_root/smoke.stdout" 2>"$test_root/smoke.stderr" || status=$?
[[ "$status" -ne 0 ]] || {
  echo "smoke test did not refuse a pre-existing exact bundle process" >&2
  exit 1
}
grep -Fq 'refusing to launch while an exact bundle process already exists' \
  "$test_root/smoke.stderr" || {
  echo "smoke test failed for the wrong reason" >&2
  sed -n '1,80p' "$test_root/smoke.stderr" >&2
  exit 1
}
kill -0 "$existing_pid" 2>/dev/null || {
  echo "smoke test terminated the process it did not own" >&2
  exit 1
}

echo "macOS smoke process-ownership adversarial test passed"
