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
# the gate, except that the root check after the suite still runs when the suite
# fails, as in CI, so a run that fails AND leaks reports both. Locally the suite
# and that check run against a throwaway home; see below.
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

# The suite and its root check run against a throwaway home, never the
# developer's real one. On a workstation the real roots are never quiet: a
# running baude rewrites its breadcrumbs under the config root, and every Claude
# session, this one included, writes to the Claude root. During a suite this long
# the check reported that traffic as test leakage on every run, so the local gate
# could not pass while baude or Claude was open. Pointing HOME and the three
# overrides into the throwaway root sends every resolver there, and anything the
# suite leaks lands where the check is looking and nobody else writes. The
# script's own header describes this rehearsal. CI runs the bracket against the
# runner's real roots, which nothing else touches, and that stays the
# authoritative check that a leak through a hard-coded path would need.
#
# cargo and python3 may be version-manager shims that find their toolchains
# through HOME. A shim that loses its HOME can install a whole toolchain into the
# throwaway one, so both resolve to real binaries first, and cargo and rustup
# keep their real homes.
isolated_root="$(mktemp -d "${TMPDIR:-/tmp}/baude-gate.XXXXXX")" || exit 2
trap 'rm -rf "$isolated_root"' EXIT
cargo_bin="$(rustup which cargo 2>/dev/null || command -v cargo)" || exit 2
isolated() {
  env \
    HOME="$isolated_root/home" \
    XDG_CONFIG_HOME="$isolated_root/home/.config" \
    XDG_DATA_HOME="$isolated_root/home/.local/share" \
    CLAUDE_CONFIG_DIR="$isolated_root/home/.claude" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" \
    RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    BAUDE_ASSERT_PYTHON="$(python3 -c 'import sys; print(sys.executable)')" \
    PATH="$(dirname "$cargo_bin"):$PATH" \
    "$@"
}
mkdir -p "$isolated_root/home" || exit 2

step isolated bash scripts/assert-real-roots-untouched.sh before || exit 1
step isolated cargo test -- --test-threads=1
suite=$?
step isolated bash scripts/assert-real-roots-untouched.sh after
roots=$?

if [[ $suite -ne 0 || $roots -ne 0 ]]; then
  exit 1
fi
echo "gate: all steps passed"
