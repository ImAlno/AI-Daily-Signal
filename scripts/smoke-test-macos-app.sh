#!/bin/bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd -P)"
bundle="$repository_root/target/macos/AI Daily Signal.app"
bundle_id="com.AIDailySignal.AI-Daily-Signal"
bundled_executable="$bundle/Contents/MacOS/AI Daily Signal"
smoke_root="$(mktemp -d)"
isolated_home="$smoke_root/home"
application_support="$isolated_home/Library/Application Support/$bundle_id"
config="$application_support/config.toml"
database="$application_support/signal.sqlite3"
app_stdout="$smoke_root/app.stdout"
app_stderr="$smoke_root/app.stderr"
process_environment="$smoke_root/process-environment.txt"
launch_info="$smoke_root/launch-info.txt"
safe_path="/usr/bin:/bin:/usr/sbin:/sbin"
application_pid=""
# shellcheck source=scripts/macos-packaging-common.sh
source "$repository_root/scripts/macos-packaging-common.sh"

run_bounded() {
  local seconds="$1"
  shift
  /usr/bin/perl -e '$SIG{ALRM}=sub { exit 124 }; alarm shift; exec @ARGV' \
    "$seconds" "$@"
}

owned_process_is_alive() {
  [[ -n "$application_pid" ]] || return 1
  kill -0 "$application_pid" 2>/dev/null || return 1
  [[ "$(ps -p "$application_pid" -o command= 2>/dev/null)" == "$bundled_executable" ]]
}

stop_owned_process() {
  owned_process_is_alive || return 0
  kill -TERM "$application_pid" 2>/dev/null || return 1
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    if ! owned_process_is_alive; then
      wait "$application_pid" 2>/dev/null || true
      return 0
    fi
    sleep 0.1
  done
  if ! owned_process_is_alive; then
    wait "$application_pid" 2>/dev/null || true
    return 0
  fi
  kill -KILL "$application_pid" 2>/dev/null || return 1
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    if ! owned_process_is_alive; then
      wait "$application_pid" 2>/dev/null || true
      return 0
    fi
    sleep 0.1
  done
  return 1
}

cleanup() {
  stop_owned_process >/dev/null 2>&1 || true
  rm -rf "$smoke_root"
}
trap cleanup EXIT

scripts/verify-macos-app.sh

preexisting_pids=""
while IFS= read -r candidate_pid; do
  [[ -n "$candidate_pid" ]] || continue
  if [[ "$(ps -p "$candidate_pid" -o command= 2>/dev/null)" == "$bundled_executable" ]]; then
    preexisting_pids="${preexisting_pids}${preexisting_pids:+,}$candidate_pid"
  fi
done < <(ps -axo pid= | tr -d ' ')
[[ -z "$preexisting_pids" ]] || {
  echo "refusing to launch while an exact bundle process already exists: $preexisting_pids" >&2
  exit 1
}

mkdir -p "$isolated_home"
[[ ! -e "$application_support" ]] || {
  echo "isolated Application Support unexpectedly existed before launch" >&2
  exit 1
}

env -u SIGNAL_HOME \
  -u DYLD_LIBRARY_PATH \
  -u DYLD_FRAMEWORK_PATH \
  -u DYLD_INSERT_LIBRARIES \
  HOME="$isolated_home" \
  PATH="$safe_path" \
  "$bundled_executable" >"$app_stdout" 2>"$app_stderr" &
application_pid=$!

for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 \
  16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do
  if owned_process_is_alive && [[ -f "$config" && -f "$database" ]]; then
    break
  fi
  sleep 0.1
done
owned_process_is_alive || {
  echo "standalone app did not start as the owned exact bundle executable within three seconds" >&2
  sed -n '1,80p' "$app_stderr" >&2
  exit 1
}
[[ -f "$config" && -f "$database" ]] || {
  echo "standalone app did not create config.toml and signal.sqlite3 in isolated Application Support" >&2
  exit 1
}

ps eww -p "$application_pid" >"$process_environment"
child_home="$(tr ' ' '\n' <"$process_environment" | grep '^HOME=' || true)"
child_path="$(tr ' ' '\n' <"$process_environment" | grep '^PATH=' || true)"
[[ "$child_home" == "HOME=$isolated_home" ]] || {
  echo "standalone app launched with an unexpected HOME" >&2
  exit 1
}
[[ "$child_path" == "PATH=$safe_path" ]] || {
  echo "standalone app launched with an unexpected PATH" >&2
  exit 1
}
if tr ' ' '\n' <"$process_environment" \
  | grep -Eq '^(SIGNAL_HOME|DYLD_LIBRARY_PATH|DYLD_FRAMEWORK_PATH|DYLD_INSERT_LIBRARIES)='; then
  echo "standalone app inherited an application-root or loader override" >&2
  exit 1
fi
if [[ "$child_path" == *"$repository_root/target"* ]]; then
  echo "standalone app PATH contains a repository CLI directory" >&2
  exit 1
fi

run_bounded 2 /usr/bin/lsappinfo info \
  -only bundleID -only pid -only bundlepath "$application_pid" >"$launch_info"
grep -Fq '"CFBundleIdentifier"="com.AIDailySignal.AI-Daily-Signal"' "$launch_info" || {
  echo "owned process did not register the expected bundle ID" >&2
  exit 1
}
grep -Fq "\"pid\"=$application_pid" "$launch_info" || {
  echo "LaunchServices record did not identify the owned PID" >&2
  exit 1
}
grep -Fq "\"LSBundlePath\"=\"$bundle\"" "$launch_info" || {
  echo "LaunchServices record did not identify the exact bundle path" >&2
  exit 1
}
[[ "$(run_bounded 2 /usr/bin/sqlite3 "$database" 'PRAGMA integrity_check;')" == 'ok' ]] || {
  echo "isolated SQLite database failed integrity_check" >&2
  exit 1
}

stop_owned_process || {
  echo "owned standalone app did not stop after bounded TERM/KILL handling" >&2
  exit 1
}
if kill -0 "$application_pid" 2>/dev/null; then
  echo "owned standalone app is still running" >&2
  exit 1
fi

scan_status=0
binary_scan_tree "$credential_provider_sentinel_pattern" \
  "$bundle" "$application_support" "$app_stdout" "$app_stderr" \
  "$process_environment" "$launch_info" || scan_status=$?
case "$scan_status" in
  0) echo "credential or provider-body sentinel found in standalone smoke artifacts" >&2; exit 1 ;;
  1) ;;
  *) echo "standalone smoke sentinel scan failed" >&2; exit 1 ;;
esac

echo "standalone direct bundle launch verified for bundle ID $bundle_id (owned pid $application_pid, stopped)"
echo "verified isolated HOME/PATH, absent overrides, created config/SQLite, integrity, and binary-safe scans"
