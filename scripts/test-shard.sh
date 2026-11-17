#!/usr/bin/env bash
# Run one shard of the workspace test suite, serially, by exact test name.
#
#   scripts/test-shard.sh <index> <count>      run shard <index> of <count>
#   scripts/test-shard.sh --list <index> <count>  print that shard's test names
#
# Every shard asks `cargo test -- --list` for the full list of test names and
# deals all of them, so the shards of one count are disjoint and their union is
# the whole suite: a new test lands in the list and is dealt like any other.
#
# The deal balances measured time. scripts/test-weights.tsv holds seconds per
# test from one serial run; a test missing from it, such as a new one, gets
# DEFAULT_WEIGHT. Tests go heaviest first, ties by name, each to the shard with
# the least time so far, ties to the lowest index. Shard 1 starts with
# SHARD1_PRELOAD because it also runs the doctests and the once-per-OS checks.
# Every runner computes the same deal from the same commit. The weights only
# move tests between shards; no weight, stale or missing, can drop a test.
#
# The shard then runs `cargo test --lib --bins --tests -- --test-threads=1
# --exact <names...>`. Each test binary runs the names it holds and filters out
# the rest, so every test still runs serially, in its own binary, exactly as the
# unsharded suite does. Serial execution stays for the reason ci.yml gives (#58).
#
# Doctests are not dealt: rustdoc does not match its own listed names under
# --exact, so a doctest named in a shard would silently run nowhere. Shard 1
# runs every doctest instead, with `cargo test --doc`, before its named tests.
#
# Written for bash 3.2, the macOS system shell: no mapfile, no associative arrays.
set -u

list_only=0
if [ "${1:-}" = "--list" ]; then
  list_only=1
  shift
fi
index="${1:-}"
count="${2:-}"
case "$index$count" in
  '' | *[!0-9]*)
    echo "usage: $0 [--list] <index> <count>" >&2
    exit 2
    ;;
esac
if [ "$count" -lt 1 ] || [ "$index" -lt 1 ] || [ "$index" -gt "$count" ]; then
  echo "test-shard: need 1 <= index <= count, got $index of $count" >&2
  exit 2
fi

listing="$(cargo test -q --lib --bins --tests -- --list --format terse)" || {
  echo "test-shard: cargo test -- --list failed" >&2
  exit 1
}

weights="$(dirname "$0")/test-weights.tsv"
[ -f "$weights" ] || weights=/dev/null
DEFAULT_WEIGHT=0.05
SHARD1_PRELOAD=60

deal="$(
  printf '%s\n' "$listing" | LC_ALL=C sort | awk -F'\t' -v w="$weights" -v d="$DEFAULT_WEIGHT" '
    BEGIN { while ((getline line < w) > 0) { if (line ~ /^#/) continue; split(line, f, "\t"); wt[f[2]] = f[1] } }
    /: test$/ { name = substr($0, 1, length($0) - 6); printf "%s\t%s\n", ((name in wt) ? wt[name] : d), name }
  ' | LC_ALL=C sort -t "$(printf '\t')" -k1,1nr -k2,2 | awk -F'\t' -v n="$count" -v pre="$SHARD1_PRELOAD" '
    BEGIN { for (i = 1; i <= n; i++) load[i] = 0; load[1] = pre }
    { best = 1; for (i = 2; i <= n; i++) if (load[i] < load[best]) best = i; load[best] += $1; print best "\t" $2 }
  '
)"

names=()
position=0
while IFS= read -r line; do
  [ -n "$line" ] || continue
  position=$((position + 1))
  if [ "${line%%	*}" -eq "$index" ]; then
    names+=("${line#*	}")
  fi
done <<EOF
$deal
EOF

if [ "$position" -eq 0 ]; then
  echo "test-shard: cargo listed no tests" >&2
  exit 1
fi
echo "test-shard: shard $index of $count runs ${#names[@]} of $position tests" >&2

if [ "$list_only" -eq 1 ]; then
  if [ "${#names[@]}" -gt 0 ]; then
    printf '%s\n' "${names[@]}"
  fi
  exit 0
fi
if [ "$index" -eq 1 ]; then
  cargo test --doc -- --test-threads=1 || exit $?
fi
if [ "${#names[@]}" -eq 0 ]; then
  exit 0
fi
exec cargo test --lib --bins --tests -- --test-threads=1 --exact "${names[@]}"
