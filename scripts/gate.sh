#!/usr/bin/env bash
#
# Run the `check` job of .github/workflows/ci.yml locally, in the same order,
# so a push is not the first time the gate runs.
#
#   gate.sh           run the gate on this machine
#   gate.sh --linux   run it in rust:1-bookworm as uid 1000, which is how the
#                     Linux-only defects of v2.3 would have shown up before CI
#
# Every step runs unpiped and its own exit status decides the result: piping a
# gate command into tail or grep reports the pipe's status and has already
# masked a real `cargo fmt --check` failure once. The first failing step stops
# the gate, except that the real-root check after the suite still runs when the
# suite fails, as in CI, so a run that fails AND leaks reports both.
#
# --linux runs as non-root on purpose: as root the seed_guard write-failure test
# fails spuriously, because root can write where the test expects a denial.
# Cargo state for the container lives under target/linux-gate so it never
# mixes with the host build.
#
# Exit codes: 0 every step passed, 1 a step failed, 2 usage error.
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

if [[ "${1:-}" == "--linux" ]]; then
  # A linked worktree's .git is a file naming the main checkout's git dir by
  # absolute path, so that dir is mounted at the same path or git cannot find
  # the repository inside the container.
  git_common="$(cd "$(git rev-parse --git-common-dir)" && pwd)" || exit 2
  # The stock image lacks rustfmt and clippy, and uid 1000 cannot add them to
  # its root-owned toolchain, so they are baked into a local image; docker's
  # layer cache makes every build after the first a no-op.
  printf 'FROM rust:1-bookworm\nRUN rustup component add rustfmt clippy\n' |
    docker build -q -t baude-gate - >/dev/null || exit 2
  exec docker run --rm -u 1000:1000 \
    -v "$ROOT":/work -w /work \
    -v "$git_common":"$git_common" \
    -e HOME=/work/target/linux-gate/home \
    -e CARGO_HOME=/work/target/linux-gate/cargo \
    -e CARGO_TARGET_DIR=/work/target/linux-gate/target \
    -e RUSTUP_HOME=/usr/local/rustup \
    -e PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    baude-gate \
    bash -c 'mkdir -p "$HOME" "$CARGO_HOME" && git config --global --add safe.directory /work && bash scripts/gate.sh'
elif [[ $# -gt 0 ]]; then
  echo "usage: gate.sh [--linux]" >&2
  exit 2
fi

step() {
  echo "==> $*"
  "$@"
  local status=$?
  if [[ $status -ne 0 ]]; then
    echo "FAIL ($status): $*" >&2
  fi
  return $status
}

step cargo fmt --check || exit 1
step cargo clippy --all-targets -- -D warnings || exit 1
step bash scripts/check-commit-message.sh --self-test || exit 1
# CI checks the PR's commits against origin/main; locally that is whatever this
# branch adds, and nothing on main itself.
if git rev-parse --verify -q origin/main >/dev/null; then
  step bash scripts/check-commit-message.sh --range origin/main..HEAD || exit 1
fi
step bash scripts/assert-real-roots-untouched.sh --self-test || exit 1
step bash scripts/assert-real-roots-untouched.sh before || exit 1
step cargo test -- --test-threads=1
suite=$?
step bash scripts/assert-real-roots-untouched.sh after
roots=$?

if [[ $suite -ne 0 || $roots -ne 0 ]]; then
  exit 1
fi
echo "gate: all steps passed"
