#!/usr/bin/env bash
# Count and time the git processes the test suite starts.
#
#   measure-git-spawns.sh run <label> <counted|timed|both>
#   measure-git-spawns.sh bench <label>
#   measure-git-spawns.sh report <label>
#
# Output lives under ${GIT_MEASURE_ROOT:-${TMPDIR:-/tmp}/git-measure}/<label>,
# never inside a git work tree.
#
# run: a counted run puts a generated git wrapper first on PATH and runs
# scripts/gate.sh once. The wrapper appends one line per call to
# <out>/log/<PPID>.log holding NEXTEST_BINARY_ID, NEXTEST_TEST_NAME and the
# argv, each argument followed by the 0x1f unit separator, then execs the real
# git. Its real-git path and log directory are literals, because some children
# run with a cleared environment. Because the wrapper resolves first, the gate's
# xcrun rewrite is skipped and the wrapper calls the real binary directly. A
# timed run is two plain gate runs. Every gate exit status is captured
# directly, never through a pipe.
#
# bench: c, the milliseconds per spawn, from 50 rounds of init, add and commit.
#
# report: suite and outside spawns, per-subcommand counts, fixture, ambiguous
# and probe class totals, the top 25 tests by total and by fixture spawns, and
# the nextest Summary of each timed log.
#
# Exit codes: 0 ok, 1 a gate run failed, 2 usage or missing data.
#
# Written for bash 3.2: no mapfile, no associative arrays.
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)" || exit 2

usage() {
  echo "usage: measure-git-spawns.sh run <label> <counted|timed|both> | bench <label> | report <label>" >&2
  exit 2
}

out_dir() {
  printf '%s/%s' "${GIT_MEASURE_ROOT:-${TMPDIR:-/tmp}/git-measure}" "$1" | sed 's|//*|/|g'
}

guard_out() {
  local out="$1" probe
  mkdir -p "$out" || exit 2
  probe="$out"
  if git -C "$probe" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "measure: $out lies inside a git work tree; set GIT_MEASURE_ROOT elsewhere" >&2
    exit 2
  fi
}

real_git() {
  local g
  g="$(command -v git)" || return 1
  if [ "$(uname -s)" = Darwin ] && [ "$g" = /usr/bin/git ]; then
    g="$(xcrun -f git 2>/dev/null)" || return 1
  fi
  case "$g" in
    /*) ;;
    *) return 1 ;;
  esac
  [ -x "$g" ] || return 1
  printf '%s' "$g"
}

sh_quote() {
  printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

write_wrapper() {
  local out="$1" git_bin="$2"
  mkdir -p "$out/bin" "$out/log" || exit 2
  {
    echo '#!/bin/sh'
    echo "log_dir=$(sh_quote "$out/log")"
    echo "real_git=$(sh_quote "$git_bin")"
    cat <<'EOF'
nl='
'
tab='	'
{
  printf '%s\t%s\t' "${NEXTEST_BINARY_ID:--}" "${NEXTEST_TEST_NAME:--}"
  for a in "$@"; do
    case $a in
      *"$nl"* | *"$tab"*) a=$(printf '%s' "$a" | tr '\n\t' '  ') ;;
    esac
    printf '%s\037' "$a"
  done
  printf '\n'
} >>"$log_dir/$PPID.log"
exec "$real_git" "$@"
EOF
  } >"$out/bin/git" || exit 2
  chmod +x "$out/bin/git" || exit 2
}

run_cmd() {
  local label="$1" mode="$2" out git_bin status failed=0 n
  out="$(out_dir "$label")"
  guard_out "$out"
  case "$mode" in
    counted | timed | both) ;;
    *) usage ;;
  esac
  if [ "$mode" = counted ] || [ "$mode" = both ]; then
    git_bin="$(real_git)" || {
      echo "measure: cannot resolve an absolute, executable real git" >&2
      exit 2
    }
    rm -rf "${out:?}/bin" "${out:?}/log"
    write_wrapper "$out" "$git_bin"
    echo "measure: counted gate run -> $out/gate-counted.log"
    PATH="$out/bin:$PATH" bash "$ROOT/scripts/gate.sh" >"$out/gate-counted.log" 2>&1
    status=$?
    echo "measure: counted gate exit $status"
    [ "$status" -eq 0 ] || failed=1
  fi
  if [ "$mode" = timed ] || [ "$mode" = both ]; then
    for n in 1 2; do
      echo "measure: timed gate run $n -> $out/gate-timed-$n.log"
      bash "$ROOT/scripts/gate.sh" >"$out/gate-timed-$n.log" 2>&1
      status=$?
      echo "measure: timed gate $n exit $status"
      if [ "$status" -ne 0 ]; then
        failed=1
        if [ "$n" -eq 1 ]; then
          echo "measure: first timed gate run failed; no timing can be taken on a red gate" >&2
          exit 1
        fi
      fi
    done
  fi
  [ "$failed" -eq 0 ] || exit 1
}

now() {
  perl -MTime::HiRes=time -e 'printf "%.6f\n", time'
}

bench_cmd() {
  local label="$1" out git_bin scratch start end i
  out="$(out_dir "$label")"
  guard_out "$out"
  git_bin="$(real_git)" || exit 2
  scratch="$out/bench"
  rm -rf "$scratch"
  mkdir -p "$scratch/home" || exit 2
  # No global or system config: a developer's commit signing or hooks must not
  # enter the per-spawn cost.
  export HOME="$scratch/home" GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
  start="$(now)"
  i=0
  while [ "$i" -lt 50 ]; do
    mkdir "$scratch/r$i" || exit 2
    printf 'fixture\n' >"$scratch/r$i/tracked.txt"
    "$git_bin" -C "$scratch/r$i" init -q || exit 1
    "$git_bin" -C "$scratch/r$i" add tracked.txt || exit 1
    "$git_bin" -C "$scratch/r$i" -c user.name=Bench -c user.email=bench@example.invalid commit -q -m fixture || exit 1
    i=$((i + 1))
  done
  end="$(now)"
  awk -v s="$start" -v e="$end" 'BEGIN { printf "bench: init+add+commit x50: %.3f s total, c = %.2f ms per spawn\n", e - s, (e - s) * 1000 / 150 }' |
    tee "$out/bench.txt"
  start="$(now)"
  i=0
  while [ "$i" -lt 50 ]; do
    "$git_bin" -C "$scratch/r0" rev-parse HEAD >/dev/null || exit 1
    i=$((i + 1))
  done
  end="$(now)"
  awk -v s="$start" -v e="$end" 'BEGIN { printf "bench: rev-parse HEAD x50: %.3f s total, %.2f ms per spawn\n", e - s, (e - s) * 1000 / 50 }' |
    tee -a "$out/bench.txt"
  rm -rf "$scratch"
}

report_cmd() {
  local label="$1" out have=0 f
  out="$(out_dir "$label")"
  if [ -d "$out/log" ] && [ -n "$(ls "$out/log" 2>/dev/null)" ]; then
    have=1
  fi
  for f in "$out"/gate-timed-*.log; do
    [ -f "$f" ] && have=1
  done
  if [ "$have" -eq 0 ]; then
    echo "measure: no counted or timed logs for label $label under $out" >&2
    exit 2
  fi
  echo "report: $label ($out)"
  [ -f "$out/bench.txt" ] && cat "$out/bench.txt"

  if [ -d "$out/log" ] && [ -n "$(ls "$out/log" 2>/dev/null)" ]; then
    cat "$out"/log/*.log | awk -F '\t' '
      function classify(args, n, start,   sub1, word, j, pos, a, setcfg, br_create, br_query) {
        sub1 = args[start]
        word = (start + 1 <= n) ? args[start + 1] : ""
        pos = 0
        for (j = start + 1; j <= n; j++) if (args[j] !~ /^-/) pos++
        if (sub1 == "init" || sub1 == "add" || sub1 == "commit" || sub1 == "push" || \
            sub1 == "switch" || sub1 == "checkout" || sub1 == "update-ref") return "fixture"
        if (sub1 == "remote" && word == "add") return "fixture"
        if (sub1 == "clone") return "ambiguous"
        if (sub1 == "worktree" && word == "add") return "ambiguous"
        if (sub1 == "symbolic-ref") return (pos >= 2) ? "ambiguous" : "probe"
        if (sub1 == "config") {
          setcfg = 0
          for (j = start + 1; j <= n; j++) {
            a = args[j]
            if (a ~ /^--get/ || a == "--list" || a == "-l" || a ~ /^get$|^list$/) return "probe"
            if (a == "--add" || a ~ /^--unset/ || a == "--replace-all" || a ~ /section$/) setcfg = 1
          }
          if (setcfg || pos >= 2) return "fixture"
          return "probe"
        }
        if (sub1 == "branch") {
          br_create = 0; br_query = 0
          for (j = start + 1; j <= n; j++) {
            a = args[j]
            if (a == "-m" || a == "-M" || a == "--move" || a == "-c" || a == "-C" || a == "--copy") br_create = 1
            if (a == "-d" || a == "-D" || a == "--delete" || a == "-l" || a == "--list" || a == "-a" || \
                a == "-r" || a == "--all" || a == "--remotes" || a ~ /^--contains/ || a ~ /merged/ || \
                a ~ /^--format/ || a == "--show-current" || a ~ /^-v/ || a == "-u" || a ~ /upstream/ || \
                a ~ /^--points-at/) br_query = 1
          }
          if (br_create || (!br_query && pos >= 1)) return "fixture"
          return "probe"
        }
        return "probe"
      }
      {
        n = split($3, args, "\037")
        if (n > 0 && args[n] == "") n--
        i = 1
        while (i <= n && args[i] ~ /^-/) {
          if (args[i] == "-C" || args[i] == "-c") i += 2
          else i++
        }
        sub1 = (i <= n) ? args[i] : "(none)"
        name = sub1
        if (sub1 == "worktree" || sub1 == "remote" || sub1 == "config" || sub1 == "branch" || sub1 == "symbolic-ref") {
          w = (i + 1 <= n) ? args[i + 1] : ""
          if ((sub1 == "branch" || sub1 == "config" || sub1 == "symbolic-ref") && w != "" && w !~ /^-/) w = "<arg>"
          if (w != "") name = sub1 " " w
        }
        cls = (i <= n) ? classify(args, n, i) : "probe"
        if ($1 == "-") {
          outside++
          osub[name]++
          next
        }
        suite++
        ssub[name]++
        scls[cls]++
        if (cls == "fixture" && (sub1 == "init" || sub1 == "add" || sub1 == "commit" || sub1 == "push" || \
            (sub1 == "remote" && args[i + 1] == "add"))) eligible++
        key = $1 " " $2
        tt[key]++
        if (cls == "fixture") tf[key]++
      }
      END {
        printf "suite spawns: %d\n", suite
        printf "outside-suite spawns: %d\n", outside
        printf "class fixture: %d\n", scls["fixture"]
        printf "class ambiguous: %d\n", scls["ambiguous"]
        printf "class probe: %d\n", scls["probe"]
        printf "template-eligible fixture spawns E: %d\n", eligible
        print "--- suite spawns by subcommand"
        for (k in ssub) printf "%8d  %s\n", ssub[k], k | "sort -rn"
        close("sort -rn")
        print "--- outside-suite spawns by subcommand"
        for (k in osub) printf "%8d  %s\n", osub[k], k | "sort -rn"
        close("sort -rn")
        print "--- top 25 tests by suite spawns"
        for (k in tt) printf "%8d  %s\n", tt[k], k | "sort -rn | head -25"
        close("sort -rn | head -25")
        print "--- top 25 tests by fixture-class spawns"
        for (k in tf) printf "%8d  %s\n", tf[k], k | "sort -rn | head -25"
        close("sort -rn | head -25")
        printf "tests that spawned git: %d\n", length(tt)
      }'
  fi

  local walls="" found=0 summary wall
  for f in "$out"/gate-timed-*.log; do
    [ -f "$f" ] || continue
    found=1
    summary="$(grep -E '^ *Summary \[' "$f" | tail -1)"
    if [ -z "$summary" ]; then
      echo "timed $(basename "$f"): no nextest Summary line"
      continue
    fi
    wall="$(printf '%s\n' "$summary" | sed -E 's/^ *Summary \[ *([0-9.]+)s\].*/\1/')"
    walls="$walls $wall"
    echo "timed $(basename "$f"): $(printf '%s' "$summary" | sed -E 's/^ +//')"
    awk '/^ +(PASS|LEAK) \[ *[0-9.]+s\]/ {
           s = $0; sub(/^[^[]*\[ */, "", s); sub(/s\].*/, "", s); t += s; n++
         }
         END { printf "  sum of %d PASS durations: %.3f s\n", n, t }' "$f"
  done
  if [ "$found" -eq 1 ] && [ -n "$walls" ]; then
    # shellcheck disable=SC2086 # one wall time per word, split on purpose
    printf '%s\n' $walls | awk '{ t += $1; n++ } END { printf "mean wall: %.3f s over %d timed runs\n", t / n, n }'
  fi
}

[ $# -ge 2 ] || usage
cmd="$1"
label="$2"
case "$label" in
  "" | */* | .*) usage ;;
esac
case "$cmd" in
  run)
    [ $# -eq 3 ] || usage
    run_cmd "$label" "$3"
    ;;
  bench) bench_cmd "$label" ;;
  report) report_cmd "$label" ;;
  *) usage ;;
esac
