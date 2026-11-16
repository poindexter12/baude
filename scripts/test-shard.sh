#!/usr/bin/env bash
# Run one shard of the workspace test suite, serially, by exact test name.
#
#   scripts/test-shard.sh <index> <count>      run shard <index> of <count>
#   scripts/test-shard.sh --list <index> <count>  print that shard's test names
#
# Every shard asks `cargo test -- --list` for the full list of test names, sorts
# it byte-wise, and takes every <count>-th name starting at <index>, counting
# from 1. Each runner computes the same list from the same commit, so the shards
# of one count are disjoint and their union is the whole suite: no test can be
# skipped by a new file or module, because new tests land in the list and the
# list is dealt round-robin. Round-robin over sorted names also spreads the slow
# families of tests, which share a prefix, across the shards.
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

names=()
position=0
while IFS= read -r line; do
  case "$line" in
    *': test') ;;
    *) continue ;;
  esac
  position=$((position + 1))
  if [ $(((position - 1) % count + 1)) -eq "$index" ]; then
    names+=("${line%: test}")
  fi
done <<EOF
$(printf '%s\n' "$listing" | LC_ALL=C sort)
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
