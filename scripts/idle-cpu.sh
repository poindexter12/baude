#!/usr/bin/env bash
#
# Measure baude's CPU consumption while it is idle in a real, headless PTY.
# This is a local evidence tool, not a CI gate or a battery-draw measurement.
#
# Usage: scripts/idle-cpu.sh [--seconds N] [--threshold CPU_SECONDS_PER_MINUTE]
#
# The release binary runs with every resolver root redirected to a newly-created
# temporary directory. The directory and the PTY child are removed on every exit.
set -euo pipefail

seconds=60
# Calibrated on the v2.3 closeout machine; see SQ-11 evidence for the baseline.
threshold=0.25
settle_seconds=5

usage() {
  printf '%s\n' \
    'usage: scripts/idle-cpu.sh [--seconds N] [--threshold CPU_SECONDS_PER_MINUTE]' \
    '  --seconds N     idle sampling window in seconds (default: 60)' \
    '  --threshold N   fail above N CPU-seconds per idle minute (default: 0.25)'
}

is_positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

is_nonnegative_number() {
  awk -v value="$1" 'BEGIN { exit !(value ~ /^[0-9]+([.][0-9]+)?$/) }'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --seconds)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      seconds="$2"
      shift 2
      ;;
    --threshold)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      threshold="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

if ! is_positive_integer "$seconds" || ! is_nonnegative_number "$threshold"; then
  usage >&2
  exit 2
fi

os="$(uname -s)"
case "$os" in
  Darwin|Linux)
    ;;
  *)
    printf 'idle-cpu: unsupported platform %s; supported platforms are macOS and Linux\n' "$os" >&2
    exit 0
    ;;
esac

repo_root="$(git rev-parse --show-toplevel)"
binary="$repo_root/target/release/baude"
root="$(mktemp -d "${TMPDIR:-/tmp}/baude-idle-cpu.XXXXXX")"
launcher_pid=""
baude_pid=""

cleanup() {
  local status=$?
  trap - EXIT HUP INT TERM

  if [[ -n "$baude_pid" ]] && kill -0 "$baude_pid" 2>/dev/null; then
    kill -TERM "$baude_pid" 2>/dev/null || true
  fi
  if [[ -n "$launcher_pid" ]] && kill -0 "$launcher_pid" 2>/dev/null; then
    kill -TERM "$launcher_pid" 2>/dev/null || true
    wait "$launcher_pid" 2>/dev/null || true
  fi
  rm -rf "$root"
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

printf 'idle-cpu: building release binary\n'
cargo build --manifest-path "$repo_root/Cargo.toml" --release -p baude
[[ -x "$binary" ]] || { printf 'idle-cpu: release binary was not built: %s\n' "$binary" >&2; exit 1; }

mkdir -p "$root/home" "$root/config" "$root/data" "$root/state" "$root/cache" "$root/claude"
export BAUDE_TEST_FIXTURE_ROOT="$root"
export HOME="$root/home"
export XDG_CONFIG_HOME="$root/config"
export XDG_DATA_HOME="$root/data"
export XDG_STATE_HOME="$root/state"
export XDG_CACHE_HOME="$root/cache"
export CLAUDE_CONFIG_DIR="$root/claude"
export ZDOTDIR="$root/home"
export ENV=/dev/null
export BASH_ENV=/dev/null
export SHELL=/bin/sh
export BAUDE_IDLE_CPU_PID_FILE="$root/baude.pid"
export BAUDE_IDLE_CPU_BIN="$binary"

launcher='printf "%s\\n" "$$" > "$BAUDE_IDLE_CPU_PID_FILE"; exec "$BAUDE_IDLE_CPU_BIN"'
case "$os" in
  Darwin)
    # BSD script takes FILE followed by COMMAND and its arguments.
    script -q /dev/null /bin/sh -c "$launcher" < /dev/null >"$root/pty.log" 2>&1 &
    launcher_pid=$!
    ;;
  Linux)
    # util-linux script takes COMMAND through -c, then its transcript file.
    script -q -c "$launcher" /dev/null < /dev/null >"$root/pty.log" 2>&1 &
    launcher_pid=$!
    ;;
esac

for _ in {1..50}; do
  if [[ -s "$BAUDE_IDLE_CPU_PID_FILE" ]]; then
    read -r baude_pid < "$BAUDE_IDLE_CPU_PID_FILE"
    if [[ "$baude_pid" =~ ^[1-9][0-9]*$ ]] && kill -0 "$baude_pid" 2>/dev/null; then
      break
    fi
  fi
  if ! kill -0 "$launcher_pid" 2>/dev/null; then
    printf 'idle-cpu: PTY launcher exited before baude started\n' >&2
    exit 1
  fi
  sleep 0.1
done

if [[ -z "$baude_pid" ]] || ! kill -0 "$baude_pid" 2>/dev/null; then
  printf 'idle-cpu: baude did not start in the PTY\n' >&2
  exit 1
fi

cpu_seconds() {
  local raw
  raw="$(ps -o time= -p "$1")"
  [[ -n "${raw//[[:space:]]/}" ]] || return 1
  awk '
    {
      gsub(/^[[:space:]]+|[[:space:]]+$/, "")
      days = 0
      clock = $0
      parts = split(clock, day_parts, "-")
      if (parts == 2) {
        days = day_parts[1] + 0
        clock = day_parts[2]
      }
      fields = split(clock, units, ":")
      seconds = units[fields] + 0
      minutes = fields >= 2 ? units[fields - 1] + 0 : 0
      hours = fields >= 3 ? units[fields - 2] + 0 : 0
      printf "%.6f\n", days * 86400 + hours * 3600 + minutes * 60 + seconds
    }'
}

printf 'idle-cpu: settling %ss past startup and session restore\n' "$settle_seconds"
sleep "$settle_seconds"

start_cpu="$(cpu_seconds "$baude_pid")" || { printf 'idle-cpu: baude exited during settle\n' >&2; exit 1; }
start_wall="$(date +%s)"
printf 'idle-cpu: sampling pid %s for %ss\n' "$baude_pid" "$seconds"
sleep "$seconds"
end_cpu="$(cpu_seconds "$baude_pid")" || { printf 'idle-cpu: baude exited during sample\n' >&2; exit 1; }
end_wall="$(date +%s)"

window_seconds=$((end_wall - start_wall))
if (( window_seconds <= 0 )); then
  printf 'idle-cpu: invalid wall-clock sampling window: %s\n' "$window_seconds" >&2
  exit 1
fi

read -r cpu_delta cpu_per_minute <<EOF
$(awk -v start="$start_cpu" -v end="$end_cpu" -v window="$window_seconds" 'BEGIN {
  delta = end - start
  printf "%.6f %.6f\n", delta, delta * 60 / window
}')
EOF

printf 'idle-cpu: raw_cpu_delta_seconds=%s wall_window_seconds=%s cpu_seconds_per_idle_minute=%s threshold=%s\n' \
  "$cpu_delta" "$window_seconds" "$cpu_per_minute" "$threshold"
printf '%s\n' \
  'idle-cpu: limits: one machine, one terminal, wall-clock sampling, and process CPU; this does not measure battery draw.'

if ! awk -v measured="$cpu_per_minute" -v limit="$threshold" 'BEGIN { exit !(measured <= limit) }'; then
  printf 'idle-cpu: FAIL: %s CPU-seconds per idle minute exceeds threshold %s\n' "$cpu_per_minute" "$threshold" >&2
  exit 1
fi

printf 'idle-cpu: PASS\n'
