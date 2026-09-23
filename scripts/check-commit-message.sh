#!/usr/bin/env bash
#
# Reject commit messages that release-please cannot parse because parentheses
# nest. release-please treats every opening parenthesis as a scope opener,
# even in the body, then silently skips an unparsable commit without opening a
# release PR. This guard makes that parser failure visible before the commit
# reaches main.
#
#   check-commit-message.sh <file|->       check one commit message
#   check-commit-message.sh --range A..B   check every non-merge commit in A..B
#   check-commit-message.sh --self-test    exercise the BL-06 examples and
#                                          compare them to the pinned parser
#
# A single parenthesis pair is fine: scopes, squash suffixes, and ordinary
# function calls all use them. The parser trap is an opening parenthesis while
# another opening parenthesis remains unmatched. An unbalanced opening by
# itself is deliberately accepted when the oracle accepts it.
#
# Exit codes: 0 pass, 1 a message has nested parentheses, 2 usage or internal
# error.
set -eu

CHECK_SCRIPT="${BASH_SOURCE[0]:-$0}"
if [[ "$CHECK_SCRIPT" != /* ]]; then
  CHECK_SCRIPT="$(cd "$(dirname "$CHECK_SCRIPT")" && pwd)/$(basename "$CHECK_SCRIPT")"
fi
CHECK_BASH="${BASH:-/bin/bash}"
export CHECK_SCRIPT CHECK_BASH

usage() {
  cat >&2 <<'USAGE'
usage:
  check-commit-message.sh <file|->
  check-commit-message.sh --range <base>..<head>
  check-commit-message.sh --self-test
USAGE
  exit 2
}

check_message() {
  local input="$1"
  local label="$2"

  # Count only unmatched opening parentheses. This deliberately does not try
  # to understand Markdown fences, Conventional Commit headers, or Rust: the
  # release-please parser does not make those distinctions either.
  awk -v label="$label" '
    {
      for (column = 1; column <= length($0); column++) {
        character = substr($0, column, 1)
        if (character == "(") {
          depth++
          if (depth > 1) {
            printf "%s:%d:%d: nested parentheses are not parseable by release-please\n", label, NR, column > "/dev/stderr"
            failed = 1
            exit 1
          }
        } else if (character == ")" && depth > 0) {
          depth--
        }
      }
    }
    END { exit failed ? 1 : 0 }
  ' "$input"
}

check_range() {
  local range="$1"
  local scratch commit status
  scratch="$(mktemp "${TMPDIR:-/tmp}/baude-commit-message.XXXXXX")"
  trap 'rm -f "$scratch"' RETURN

  while IFS= read -r commit; do
    git show -s --format=%B "$commit" > "$scratch"
    if ! check_message "$scratch" "$commit"; then
      status=1
    fi
  done < <(git rev-list --no-merges "$range")

  return "${status:-0}"
}

write_case() {
  local directory="$1"
  local number="$2"
  local message="$3"
  local path="$directory/$number"
  printf '%s' "$message" > "$path"
  printf '%s\t%s\n' "$number" "$path" >> "$directory/oracle-input"
}

run_oracle() {
  local directory="$1"
  local oracle_script="$directory/oracle.js"

  if ! command -v node >/dev/null 2>&1; then
    printf 'SKIP oracle: node is unavailable; checked BL-06 cases with the local guard only.\n'
    return 2
  fi

  cat > "$oracle_script" <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

const packageBin = process.env.PATH.split(path.delimiter).find((entry) =>
  entry.endsWith('/node_modules/.bin'),
);
if (!packageBin) {
  throw new Error('npx did not expose @conventional-commits/parser');
}
const {parser} = require(path.join(
  path.dirname(packageBin),
  '@conventional-commits/parser',
));

for (const line of fs.readFileSync(process.argv[2], 'utf8').trimEnd().split('\n')) {
  const [number, messagePath] = line.split('\t');
  const message = fs.readFileSync(messagePath, 'utf8');
  try {
    parser(message);
    console.log(`${number}\tpass`);
  } catch (_error) {
    console.log(`${number}\tfail`);
  }
}
NODE

  # npx supplies the parser from a clean, pinned package cache. Node 22+ does
  # not add that cache to module resolution, so oracle.js resolves it from the
  # node_modules/.bin path npx prepends to PATH.
  if ! npx --yes --package=@conventional-commits/parser@0.4.1 -- \
    node "$oracle_script" "$directory/oracle-input" > "$directory/oracle-output" 2> "$directory/oracle-error"; then
    printf 'SKIP oracle: node is available, but npx could not fetch or run @conventional-commits/parser@0.4.1.\n'
    sed -n '1,3p' "$directory/oracle-error" >&2
    return 2
  fi
}

oracle_verdict() {
  local directory="$1"
  local number="$2"
  awk -F '\t' -v number="$number" '$1 == number { print $2; exit }' "$directory/oracle-output"
}

actual_verdict() {
  local path="$1"
  if "$CHECK_BASH" "$CHECK_SCRIPT" "$path" >/dev/null 2>&1; then
    printf 'pass'
  else
    printf 'fail'
  fi
}

self_test() {
  local directory number table oracle expected actual corrections=0
  directory="$(mktemp -d "${TMPDIR:-/tmp}/baude-commit-message-self-test.XXXXXX")"
  trap 'rm -rf "$directory"' RETURN
  : > "$directory/oracle-input"

  # BL-06 cases 1-10. Case 8's bad non-merge message reaches both the parser
  # and the one-message guard. Case 9's bad merge message reaches the parser,
  # then --range proves it is skipped rather than checked as a standalone file.
  # That keeps the oracle tied to every numbered case without changing Git's
  # intentional merge exclusion.
  write_case "$directory" 1 'fix(core): plain header'
  write_case "$directory" 2 'fix(core): plain header (#93)'
  write_case "$directory" 3 $'fix(core): body\n\npasses validate() and foo(bar)'
  write_case "$directory" 4 $'fix(core): quote\n\nContradictoryLifecycle(CheckoutKey(1))'
  write_case "$directory" 5 $'fix(core): fenced code\n\n```\nFoo(Bar(1))\n```'
  write_case "$directory" 6 $'fix(core): references\n\nRefs #92, #93.'
  write_case "$directory" 7 'fix(core(x)): nested scope'
  write_case "$directory" 8 $'fix: bad\n\nBroken(Inner(1))'
  write_case "$directory" 9 $'merge: Broken(Inner(1))'
  write_case "$directory" 10 $'fix(core): unbalanced\n\nvalue ('

  if run_oracle "$directory"; then
    oracle_available=1
  else
    oracle_available=0
  fi

  for number in 1 2 3 4 5 6 7 8 10; do
    case "$number" in
      4|5|7|8) table=fail ;;
      *) table=pass ;;
    esac
    expected="$table"

    if [ "$oracle_available" -eq 1 ]; then
      oracle="$(oracle_verdict "$directory" "$number")"
      if [ -z "$oracle" ]; then
        printf 'case %s: FAIL oracle returned no verdict\n' "$number" >&2
        return 1
      fi
      if [ "$number" = 10 ]; then
        # BL-06 explicitly leaves this to the parser rather than assuming
        # unmatched input is invalid.
        printf 'case 10 oracle verdict: %s\n' "$oracle"
      elif [ "$table" != "$oracle" ]; then
        printf 'case %s: parser correction: table=%s, oracle=%s\n' "$number" "$table" "$oracle"
        corrections=1
      fi
      expected="$oracle"
    fi

    actual="$(actual_verdict "$directory/$number")"
    if [ "$actual" != "$expected" ]; then
      printf 'case %s: FAIL guard=%s, expected=%s\n' "$number" "$actual" "$expected" >&2
      return 1
    fi
    printf 'case %s: %s\n' "$number" "$actual"
  done

  # Case 8: one bad non-merge commit among three good commits names its SHA.
  local range_repo base bad_sha range_output
  range_repo="$directory/range"
  git init -q "$range_repo"
  git -C "$range_repo" config user.name 'BL-06 self-test'
  git -C "$range_repo" config user.email 'bl-06@example.invalid'
  : > "$range_repo/file"
  git -C "$range_repo" add file
  git -C "$range_repo" commit -q -m 'chore: base'
  base="$(git -C "$range_repo" rev-parse HEAD)"
  for message in 'fix: first good' $'fix: bad\n\nBroken(Inner(1))' 'fix: second good' 'fix: third good'; do
    printf '%s\n' "$message" >> "$range_repo/file"
    git -C "$range_repo" add file
    git -C "$range_repo" commit -q -F - <<< "$message"
  done
  bad_sha="$(git -C "$range_repo" log -1 --format=%H --grep='bad')"
  range_output="$directory/range-output"
  if (cd "$range_repo" && "$CHECK_BASH" "$CHECK_SCRIPT" --range "$base..HEAD") > "$range_output" 2>&1; then
    printf 'case 8: FAIL --range accepted a bad commit\n' >&2
    return 1
  fi
  if ! grep -Fq "$bad_sha" "$range_output"; then
    printf 'case 8: FAIL --range did not name bad commit %s\n' "$bad_sha" >&2
    return 1
  fi
  printf 'case 8: fail as expected and names %s\n' "$bad_sha"

  # Case 9: a merge commit containing a bad message is skipped. Its two parent
  # commits have ordinary messages, so a successful range proves --no-merges
  # selected only the non-merge commits.
  local merge_repo merge_base merge_branch
  merge_repo="$directory/merge"
  git init -q "$merge_repo"
  git -C "$merge_repo" config user.name 'BL-06 self-test'
  git -C "$merge_repo" config user.email 'bl-06@example.invalid'
  : > "$merge_repo/file"
  git -C "$merge_repo" add file
  git -C "$merge_repo" commit -q -m 'chore: base'
  merge_base="$(git -C "$merge_repo" rev-parse HEAD)"
  merge_branch="$(git -C "$merge_repo" branch --show-current)"
  git -C "$merge_repo" checkout -q -b topic
  printf 'topic\n' >> "$merge_repo/file"
  git -C "$merge_repo" add file
  git -C "$merge_repo" commit -q -m 'fix: topic good'
  git -C "$merge_repo" checkout -q "$merge_branch"
  printf 'main\n' > "$merge_repo/main-file"
  git -C "$merge_repo" add main-file
  git -C "$merge_repo" commit -q -m 'fix: main good'
  git -C "$merge_repo" merge -q --no-ff topic -m $'merge: Broken(Inner(1))'
  if ! (cd "$merge_repo" && "$CHECK_BASH" "$CHECK_SCRIPT" --range "$merge_base..HEAD"); then
    printf 'case 9: FAIL --range did not skip the merge commit\n' >&2
    return 1
  fi
  if [ "$oracle_available" -eq 1 ] && [ "$(oracle_verdict "$directory" 9)" != pass ]; then
    printf 'case 9: FAIL parser did not accept the skipped merge message\n' >&2
    return 1
  fi
  printf 'case 9: merge commit skipped\n'

  if [ "$corrections" -eq 1 ]; then
    printf 'parser corrections above were used as expected verdicts.\n'
  fi
  printf 'case 11: self-test completed%s oracle.\n' \
    "$([ "$oracle_available" -eq 1 ] && printf ' with' || printf ' without')"
}

case "${1:-}" in
  --self-test)
    [ "$#" -eq 1 ] || usage
    self_test
    ;;
  --range)
    [ "$#" -eq 2 ] || usage
    check_range "$2"
    ;;
  -|?*)
    [ "$#" -eq 1 ] || usage
    check_message "$1" "$1"
    ;;
  *)
    usage
    ;;
esac
