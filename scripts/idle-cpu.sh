#!/usr/bin/env bash
#
# Measure the CPU a running baude uses while nothing is happening, which is
# what the release checklist's "idle CPU near zero" item asserts.
#
#   idle-cpu.sh [seconds] [pid]
#
# Samples %CPU once a second for `seconds` (default 60) and prints the mean and
# the peak. Without a pid it takes the oldest process named exactly `baude`.
# Leave the TUI untouched and its sessions idle while it runs: a keypress or a
# busy session is real work and will show up in the numbers.
#
# Exit codes: 0 measured, 1 the process was not found or exited mid-sample,
# 2 usage error.
set -u

seconds="${1:-60}"
pid="${2:-$(pgrep -ox baude)}"
if ! [[ "$seconds" =~ ^[0-9]+$ ]] || [[ "$seconds" -eq 0 ]]; then
  echo "usage: idle-cpu.sh [seconds] [pid]" >&2
  exit 2
fi
if [[ -z "$pid" ]] || ! kill -0 "$pid" 2>/dev/null; then
  echo "idle-cpu: no running baude process found" >&2
  exit 1
fi

echo "sampling pid $pid for ${seconds}s"
samples=()
for ((i = 0; i < seconds; i++)); do
  cpu="$(ps -o %cpu= -p "$pid")" || {
    echo "idle-cpu: pid $pid exited after ${i}s" >&2
    exit 1
  }
  samples+=("${cpu// /}")
  sleep 1
done

printf '%s\n' "${samples[@]}" | awk '
  { sum += $1; if ($1 > max) max = $1 }
  END { printf "mean %.2f%%  peak %.2f%%  over %d samples\n", sum / NR, max, NR }'
