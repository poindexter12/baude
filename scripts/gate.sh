#!/usr/bin/env bash
#
# Run the `check` job of .github/workflows/ci.yml locally, in the same order,
# so a push is not the first time the gate runs. It starts with the PWA's
# node:test suites, then the cargo steps.
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
  # layer cache makes every build after the first a no-op. It lacks node as
  # well, which the PWA suites need, so the binary is copied in from the node
  # image. Its major must stay equal to node-version in ci.yml.
  printf 'FROM rust:1-bookworm\nRUN rustup component add rustfmt clippy\nCOPY --from=node:26-bookworm /usr/local/bin/node /usr/local/bin/node\n' |
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

# The PWA suites go first: they are the cheapest step and need nothing built.
# They do not run under `isolated` below. node may be a version-manager shim
# that finds its toolchain through HOME, the hazard described there for cargo
# and python3, and the suites are pure functions that write nothing, so they
# have no place inside the real-roots bracket.
step bash scripts/check-web-suites.sh || exit 1
step cargo fmt --check || exit 1
step cargo clippy --all-targets -- -D warnings || exit 1
step bash scripts/check-commit-message.sh --self-test || exit 1
step bash scripts/ci-changes.sh --self-test || exit 1
# CI checks the PR's commits against origin/main; locally that is whatever this
# branch adds, and nothing on main itself.
if git rev-parse --verify -q origin/main >/dev/null; then
  step bash scripts/check-commit-message.sh --range origin/main..HEAD || exit 1
fi
step bash scripts/assert-real-roots-untouched.sh --self-test || exit 1
step bash scripts/assert-no-test-leftovers.sh --self-test || exit 1

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
#
# On macOS /usr/bin/git is usually the xcrun shim, which execs the real git and
# roughly doubles the cost of every call: about 32 ms against 16 ms on a dev
# host. The suite makes some 15,000 git calls, so when git resolves to the shim
# the real binary's directory goes into the suite's PATH just ahead of
# /usr/bin. Only commands that already resolved to /usr/bin change, and each of
# those is an xcrun shim for the binary that now runs directly; anything earlier
# on PATH, a version manager's python3 for one, still wins. A link to the real
# git does not work: Apple's git finds its helpers and templates relative to
# the path it was started from.
#
# The suite itself runs under the test leftover check, with TMPDIR pointed at a
# directory inside the throwaway root. Every fixture root then lands there,
# outside every git work tree, so a fixture never sits inside this checkout.
# While the suite runs, the check's watcher sees a runtime orphaned under that
# directory; when it ends, the check sees any process still working there and
# any fixture entry left behind. Either fails the step, and so does a failing
# suite: the check exits 0 only when the suite and all three counts are zero.
isolated_root="$(mktemp -d "${TMPDIR:-/tmp}/baude-gate.XXXXXX")" || exit 2
trap 'rm -rf "$isolated_root"' EXIT
cargo_bin="$(rustup which cargo 2>/dev/null || command -v cargo)" || exit 2
suite_path="$(dirname "$cargo_bin"):$PATH"
if [ "$(uname -s)" = Darwin ] && [ "$(command -v git)" = /usr/bin/git ] &&
  real_git="$(xcrun -f git 2>/dev/null)" && [ -x "$real_git" ]; then
  suite_path="$(printf '%s' "$suite_path" | awk -v RS=: -v ORS=: -v real="$(dirname "$real_git")" \
    '$0 == "/usr/bin" && !done { print real; done = 1 } { print }' | sed 's/:$//')"
fi
isolated() {
  env \
    HOME="$isolated_root/home" \
    XDG_CONFIG_HOME="$isolated_root/home/.config" \
    XDG_DATA_HOME="$isolated_root/home/.local/share" \
    CLAUDE_CONFIG_DIR="$isolated_root/home/.claude" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" \
    RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    BAUDE_ASSERT_PYTHON="$(python3 -c 'import sys; print(sys.executable)')" \
    PATH="$suite_path" \
    "$@"
}
mkdir -p "$isolated_root/home" || exit 2

step isolated bash scripts/assert-real-roots-untouched.sh before || exit 1
step isolated bash scripts/assert-no-test-leftovers.sh run "$isolated_root/tmp" -- cargo test -- --test-threads=1
suite=$?
step isolated bash scripts/assert-real-roots-untouched.sh after
roots=$?

if [[ $suite -ne 0 || $roots -ne 0 ]]; then
  exit 1
fi
echo "gate: all steps passed"
