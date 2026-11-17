#!/usr/bin/env bash
# Decide which CI work a change needs, from the paths it touches.
#
#   git diff --name-only <base>...<head> | scripts/ci-changes.sh
#   scripts/ci-changes.sh --all          # everything, for pushes to main
#   scripts/ci-changes.sh --self-test
#
# Prints GITHUB_OUTPUT lines:
#   packages=<space-separated workspace packages whose tests must run>
#   rust=true|false     fmt, clippy and the test shards
#   web=true|false      the PWA node:test suites
#   docker=true|false   the image build and smoke test
#
# Selection is by what a path can reach, never by guessing what a change
# means. A path in a crate runs that crate's tests and every workspace crate
# that depends on it (vendor/vt100 and baude-core reach all of them). Paths
# that no test, build or image reads are skipped: planning, docs and the
# release-please config. Anything else, including a path no rule names, runs
# everything, so a new top-level file is tested until a rule says otherwise.
set -u

ALL_PACKAGES="vt100 baude-core baude bauded"

classify() {
  local all=0 web=0 docker=0 p
  local -a pk=()
  while IFS= read -r p || [ -n "$p" ]; do
    [ -n "$p" ] || continue
    case "$p" in
      .planning/* | docs/* | graphify-out/* | LICENSE \
        | README.md | CHANGELOG.md | CLAUDE.md \
        | release-please-config.json | .release-please-manifest.json \
        | .githooks/*)
        ;;
      vendor/vt100/* | baude-core/*)
        pk+=(vt100 baude-core baude bauded)
        ;;
      baude/*)
        pk+=(baude)
        ;;
      bauded/web/*)
        pk+=(bauded)
        web=1
        ;;
      bauded/*)
        pk+=(bauded)
        ;;
      Dockerfile | docker-entrypoint.sh | compose.yaml | .dockerignore | .env.example)
        docker=1
        ;;
      *)
        all=1
        ;;
    esac
  done
  if [ "$all" -eq 1 ]; then
    emit "$ALL_PACKAGES" 1 1
    return
  fi
  local selected="" name
  for name in $ALL_PACKAGES; do
    case " ${pk[*]-} " in *" $name "*) selected="${selected:+$selected }$name" ;; esac
  done
  # The image builds baude and bauded from source, so any Rust change rebuilds it.
  [ -n "$selected" ] && docker=1
  emit "$selected" "$web" "$docker"
}

emit() {
  local rust=false web=false docker=false
  [ -n "$1" ] && rust=true
  [ "$2" -eq 1 ] && web=true
  [ "$3" -eq 1 ] && docker=true
  printf 'packages=%s\nrust=%s\nweb=%s\ndocker=%s\n' "$1" "$rust" "$web" "$docker"
}

self_test() {
  local fails=0
  expect() {
    local got want="$2"
    got="$(printf '%s' "$1" | classify | tr '\n' ' ')"
    if [ "$got" != "$want" ]; then
      printf 'FAIL: %s\n  want: %s\n  got:  %s\n' "$(printf '%s' "$1" | tr '\n' ',')" "$want" "$got"
      fails=$((fails + 1))
    fi
  }
  expect $'.planning/STATE.md\ndocs/deploy.md\nREADME.md' 'packages= rust=false web=false docker=false '
  expect '' 'packages= rust=false web=false docker=false '
  expect 'baude/src/app.rs' 'packages=baude rust=true web=false docker=true '
  expect 'bauded/src/api.rs' 'packages=bauded rust=true web=false docker=true '
  expect 'bauded/web/app.js' 'packages=bauded rust=true web=true docker=true '
  expect $'baude/src/ui.rs\nbauded/src/api.rs' 'packages=baude bauded rust=true web=false docker=true '
  expect 'baude-core/src/git.rs' 'packages=vt100 baude-core baude bauded rust=true web=false docker=true '
  expect 'vendor/vt100/src/lib.rs' 'packages=vt100 baude-core baude bauded rust=true web=false docker=true '
  expect 'Dockerfile' 'packages= rust=false web=false docker=true '
  expect $'.planning/x.md\nCargo.lock' 'packages=vt100 baude-core baude bauded rust=true web=true docker=true '
  expect 'scripts/test-shard.sh' 'packages=vt100 baude-core baude bauded rust=true web=true docker=true '
  expect '.github/workflows/ci.yml' 'packages=vt100 baude-core baude bauded rust=true web=true docker=true '
  expect 'some-new-file.toml' 'packages=vt100 baude-core baude bauded rust=true web=true docker=true '
  expect 'docs.md' 'packages=vt100 baude-core baude bauded rust=true web=true docker=true '
  if [ "$fails" -eq 0 ]; then
    echo "ci-changes: self-test passed"
    return 0
  fi
  return 1
}

case "${1:-}" in
  --all) emit "$ALL_PACKAGES" 1 1 ;;
  --self-test) self_test ;;
  "") classify ;;
  *)
    echo "usage: $0 [--all | --self-test] < paths" >&2
    exit 2
    ;;
esac
