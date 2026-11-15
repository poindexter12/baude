#!/usr/bin/env bash
#
# Run the PWA's node:test suites: every *.test.js directly under
# bauded/web/tests/. The suites are plain node:test files with no package and
# no build step, so the whole run is `node --test` over that directory.
#
#   check-web-suites.sh             run the suites under bauded/web/tests
#   check-web-suites.sh <directory> run the suites under another directory
#
# scripts/gate.sh and the `check` job of .github/workflows/ci.yml both run this
# with no argument, so the two cannot drift. The directory argument exists only
# so the failure modes below can be driven against fixture directories.
#
# The script fails loudly where `node --test` alone would not. Over an empty
# directory node reports `tests 0` and exits 0, so a deleted or renamed suite
# would leave the gate green; here that is a failure, and so is a missing node.
# No environment variable is read, and no path returns 0 unless `node --test`
# ran and passed.
#
# Exit codes: 0 the suites ran and passed; 1 node is missing, the directory is
# missing, it holds no suite, or a test failed; 2 usage error.
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

if [[ $# -gt 1 ]]; then
  echo "check-web-suites: usage: check-web-suites.sh [directory]" >&2
  exit 2
fi
dir="${1:-bauded/web/tests}"
if [[ ${#dir} -gt 1 ]]; then
  dir="${dir%/}"
fi

if ! command -v node >/dev/null 2>&1; then
  echo "check-web-suites: node is not on PATH, so the PWA suites cannot run" >&2
  exit 1
fi

if [[ ! -d "$dir" ]]; then
  echo "check-web-suites: no such directory: $dir" >&2
  exit 1
fi

# An unmatched glob stays a literal pattern, which names no file. Testing the
# first element also works under the bash 3.2 a bare macOS PATH resolves.
suites=("$dir"/*.test.js)
if [[ ! -e "${suites[0]}" ]]; then
  echo "check-web-suites: no *.test.js suite in $dir" >&2
  exit 1
fi

echo "check-web-suites: node $(node --version), ${#suites[@]} suite file(s) in $dir"

node --test "$dir/"
status=$?
if [[ $status -ne 0 ]]; then
  echo "check-web-suites: node --test exited $status" >&2
  exit 1
fi

echo "check-web-suites: passed"
exit 0
