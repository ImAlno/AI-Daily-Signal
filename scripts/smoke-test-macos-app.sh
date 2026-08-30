#!/bin/bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd -P)"
bundle="$repository_root/target/macos/AI Daily Signal.app"
bundle_id="com.AIDailySignal.AI-Daily-Signal"
smoke_root="$(mktemp -d)"
isolated_home="$smoke_root/home"
application_support="$isolated_home/Library/Application Support/$bundle_id"
app_stdout="$smoke_root/app.stdout"
app_stderr="$smoke_root/app.stderr"
open_stdout="$smoke_root/open.stdout"
open_stderr="$smoke_root/open.stderr"
process_path="$smoke_root/process-path.txt"
safe_path="/usr/bin:/bin:/usr/sbin:/sbin"
bundled_executable="$bundle/Contents/MacOS/AI Daily Signal"
application_pid=""

cleanup() {
  if [[ -n "$application_pid" ]] && kill -0 "$application_pid" 2>/dev/null; then
    osascript -e "tell application id \"$bundle_id\" to quit" >/dev/null 2>&1 || true
  fi
  rm -rf "$smoke_root"
}
trap cleanup EXIT

scripts/verify-macos-app.sh
mkdir -p "$application_support"
: >"$app_stdout"
: >"$app_stderr"

open -n -F -g \
  -o "$app_stdout" \
  --stderr "$app_stderr" \
  --env "HOME=$isolated_home" \
  --env "PATH=$safe_path" \
  "$bundle" >"$open_stdout" 2>"$open_stderr"

for _ in $(seq 1 50); do
  launch_record="$(lsappinfo find "bundleID=$bundle_id" 2>/dev/null || true)"
  application_pid="$(pgrep -n -f "^$bundled_executable$" 2>/dev/null || true)"
  if [[ -n "$launch_record" && -n "$application_pid" ]] \
    && kill -0 "$application_pid" 2>/dev/null; then
    break
  fi
  sleep 0.2
done

[[ -n "$application_pid" ]] && kill -0 "$application_pid" 2>/dev/null || {
  echo "standalone app did not start within ten seconds" >&2
  sed -n '1,80p' "$open_stderr" >&2
  sed -n '1,80p' "$app_stderr" >&2
  exit 1
}

ps eww -p "$application_pid" | tr ' ' '\n' | grep '^PATH=' >"$process_path"
grep -Fxq "PATH=$safe_path" "$process_path" || {
  echo "standalone app launched with an unexpected PATH" >&2
  exit 1
}
if grep -Fq "$repository_root/target" "$process_path"; then
  echo "standalone app PATH contains a repository CLI directory" >&2
  exit 1
fi

osascript -e "tell application id \"$bundle_id\" to quit" >/dev/null
for _ in $(seq 1 25); do
  if ! kill -0 "$application_pid" 2>/dev/null; then
    break
  fi
  sleep 0.2
done
if kill -0 "$application_pid" 2>/dev/null; then
  echo "standalone app did not quit within five seconds" >&2
  exit 1
fi

sentinel_pattern='SENTINEL|SIGNAL_TEST_CREDENTIAL|SIGNAL_TEST_PROVIDER_BODY|credential-contract-secret|provider-body-sentinel'
if find "$bundle" "$application_support" -type f -print0 \
  | xargs -0 grep -aEIil -- "$sentinel_pattern" >/dev/null 2>&1; then
  echo "credential or provider-body sentinel found in bundle or isolated Application Support" >&2
  exit 1
fi
if grep -aEIil -- "$sentinel_pattern" \
  "$app_stdout" "$app_stderr" "$open_stdout" "$open_stderr" "$process_path" >/dev/null 2>&1; then
  echo "credential or provider-body sentinel found in captured launch output" >&2
  exit 1
fi

echo "standalone launch verified for bundle ID $bundle_id (pid $application_pid, stopped)"
echo "captured stdout/stderr and scanned bundle plus isolated Application Support"
