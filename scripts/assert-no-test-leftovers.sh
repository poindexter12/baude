#!/usr/bin/env bash
#
# Assert that running a test command leaves nothing behind in its TMPDIR: no
# fixture entry, no orphaned runtime while it runs, no live runtime after it.
#
# The PTY and signal tests passed while leaking a `sleep 30` each. Only an
# observer outside the test process sees that, and only while it happens: a
# leak older than the leaked command's lifetime has already exited by the end of
# the suite. So `run` samples the process table beside the command and checks
# the directory and the process table again once it exits.
#
#   assert-no-test-leftovers.sh run <dir> -- <command> [args...]
#       create <dir> if missing, refuse it with exit 2 when it lies inside a git
#       work tree or already holds a fixture entry, run the command with TMPDIR
#       set to <dir> exactly as given, sample processes while it runs, then run
#       the end check
#   assert-no-test-leftovers.sh check <dir>
#       the end check alone
#   assert-no-test-leftovers.sh --self-test
#       synthetic cases only, never a cargo run
#
# A fixture entry is a top-level name in <dir> beginning `baude-` or `bauded-`.
# An orphan is a process whose cwd is under <dir>, which is not a descendant of
# this script, and whose pid and command were seen in two samples at least the
# persist window apart. A live process is any process other than this script
# whose cwd is under <dir> at the end check. Paths are compared after realpath
# on both sides, because macOS lsof reports /private/var where TMPDIR says /var.
#
# The script only observes. It never signals, kills or deletes anything outside
# its own self-test scratch.
#
# Output: exactly one line on stdout,
#   assert-no-test-leftovers: command=<rc> orphans=<n> live=<n> dirs=<n>
# with `command=-` for `check`; details and the command's own output go to
# stderr. Exit codes: 0 all four are 0, 1 otherwise, 2 usage or internal error.
#
# Environment: BAUDE_ASSERT_PYTHON names the interpreter, default python3, and
# the body runs on Python 3.9. BAUDE_LEFTOVER_SAMPLE_SECS, default 2, and
# BAUDE_LEFTOVER_PERSIST_SECS, default 5, set the sampling interval and the
# persist window.
set -eu

# The self-test re-invokes this file, and argv[0] is "-" under the heredoc, so
# the concrete script, shell and interpreter are handed down deliberately.
BAUDE_ASSERT_SCRIPT="${BASH_SOURCE[0]:-$0}"
BAUDE_ASSERT_BASH="${BASH:-/bin/bash}"
export BAUDE_ASSERT_SCRIPT BAUDE_ASSERT_BASH

exec "${BAUDE_ASSERT_PYTHON:-python3}" - "$@" <<'PY'
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import traceback

PREFIX = "assert-no-test-leftovers"


def summary(command, orphans, live, dirs):
    return "{}: command={} orphans={} live={} dirs={}".format(
        PREFIX, command, orphans, live, dirs
    )


FIXTURE_PREFIXES = ("baude-", "bauded-")


class Refused(Exception):
    """A usage or precondition error: exit 2, no summary line."""


def err(message):
    sys.stderr.write("{}: {}\n".format(PREFIX, message))


def enclosing_work_tree(path):
    """The nearest ancestor of realpath(path) holding a `.git`, or None.

    Walks from the deepest existing component so a dir that does not exist yet
    is judged by where it would be created.
    """
    current = os.path.realpath(path)
    while True:
        if os.path.lexists(os.path.join(current, ".git")):
            return current
        parent = os.path.dirname(current)
        if parent == current:
            return None
        current = parent


def fixture_entries(directory):
    names = sorted(os.listdir(directory))
    return [n for n in names if n.startswith(FIXTURE_PREFIXES)]


class InternalError(Exception):
    """The check itself could not run: exit 2, no summary line."""


def env_secs(name, default):
    raw = os.environ.get(name, "")
    if raw == "":
        return default
    try:
        value = float(raw)
    except ValueError:
        raise Refused("{}={!r} is not a number of seconds".format(name, raw))
    if value <= 0:
        raise Refused("{}={!r} must be positive".format(name, raw))
    return value


# --- process table -----------------------------------------------------------
#
# A snapshot is (ppids, commands, cwds), each keyed by pid. Parents are read
# before working folders, so a process born in between has no parent entry and
# is skipped rather than misjudged as a non-descendant; it is seen next sample.

DELETED_SUFFIX = " (deleted)"


def linux_snapshot():
    ppids, commands = {}, {}
    for name in os.listdir("/proc"):
        if not name.isdigit():
            continue
        pid = int(name)
        try:
            with open("/proc/{}/stat".format(pid), "rb") as handle:
                stat = handle.read()
        except OSError:
            continue
        # The command name may hold spaces and parentheses: the fields after it
        # start past the LAST closing parenthesis.
        close = stat.rfind(b")")
        fields = stat[close + 2:].split()
        try:
            ppids[pid] = int(fields[1])
        except (IndexError, ValueError):
            continue
        command = b""
        try:
            with open("/proc/{}/cmdline".format(pid), "rb") as handle:
                command = handle.read().rstrip(b"\0").replace(b"\0", b" ")
        except OSError:
            pass
        if not command:
            command = stat[stat.find(b"(") + 1:close]
        commands[pid] = command.decode("utf-8", "replace")
    cwds = {}
    for pid in ppids:
        try:
            link = os.readlink("/proc/{}/cwd".format(pid))
        except OSError:
            continue
        # A working folder deleted while the process ran reads back with this
        # suffix; the process is still there and still a leak.
        if link.endswith(DELETED_SUFFIX):
            link = link[: -len(DELETED_SUFFIX)]
        cwds[pid] = link
    return ppids, commands, cwds


def find_lsof():
    found = shutil.which("lsof")
    if found:
        return found
    for candidate in ("/usr/sbin/lsof", "/usr/bin/lsof"):
        if os.access(candidate, os.X_OK):
            return candidate
    raise InternalError("lsof is required to read process working folders here and was not found")


def darwin_snapshot():
    # Every helper runs from / so its own cwd can never land under the dir.
    lsof = subprocess.run(
        [find_lsof(), "-nP", "-w", "-d", "cwd", "-Fpn"],
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        cwd="/",
    )
    cwds = {}
    pid = None
    for line in lsof.stdout.split(b"\n"):
        if line.startswith(b"p"):
            try:
                pid = int(line[1:])
            except ValueError:
                pid = None
        elif line.startswith(b"n") and pid is not None:
            cwds[pid] = os.fsdecode(line[1:])
    # lsof exits 1 whenever any process could not be read; that is normal for a
    # non-root user. Only an exit with nothing parsed is a failure.
    if not cwds:
        raise InternalError("lsof exited {} with no working folders parsed".format(lsof.returncode))

    ps = subprocess.run(
        ["ps", "-axo", "pid=,ppid=,command="],
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        cwd="/",
    )
    if ps.returncode != 0:
        raise InternalError("ps exited {}: {}".format(
            ps.returncode, ps.stderr.decode("utf-8", "replace").strip()))
    ppids, commands = {}, {}
    for line in ps.stdout.decode("utf-8", "replace").splitlines():
        parts = line.split(None, 2)
        if len(parts) < 2:
            continue
        try:
            row_pid, row_ppid = int(parts[0]), int(parts[1])
        except ValueError:
            continue
        ppids[row_pid] = row_ppid
        commands[row_pid] = parts[2] if len(parts) == 3 else ""
    return ppids, commands, cwds


def snapshot():
    if sys.platform.startswith("linux"):
        return linux_snapshot()
    if sys.platform == "darwin":
        return darwin_snapshot()
    raise InternalError("no process reader for platform {}".format(sys.platform))


def under(path, root):
    real = os.path.realpath(path)
    return real == root or real.startswith(root.rstrip(os.sep) + os.sep)


def is_descendant(pid, ppids, me):
    seen = set()
    current = pid
    while True:
        if current == me:
            return True
        if current is None or current <= 1 or current in seen:
            return False
        seen.add(current)
        current = ppids.get(current)


def describe(kind, pid, ppids, commands, cwd, extra=""):
    return "{} pid={} ppid={} cwd={} command={}{}".format(
        kind, pid, ppids.get(pid, "?"), cwd, commands.get(pid, "?"), extra
    )


class Sampler(threading.Thread):
    """Watch for orphans under the dir while the command runs."""

    def __init__(self, root, me, interval, persist):
        threading.Thread.__init__(self, daemon=True)
        self.root = root
        self.me = me
        self.interval = interval
        self.persist = persist
        self.first_seen = {}
        self.orphans = {}
        self.error = None
        self.stopping = threading.Event()

    def run(self):
        try:
            while True:
                self.sample()
                if self.stopping.wait(self.interval):
                    return
        except Exception as error:  # reported as exit 2 once the command ends
            self.error = error

    def sample(self):
        now = time.monotonic()
        ppids, commands, cwds = snapshot()
        for pid, cwd in cwds.items():
            if pid == self.me or pid not in ppids or not under(cwd, self.root):
                continue
            if is_descendant(pid, ppids, self.me):
                continue
            key = (pid, commands.get(pid, "?"))
            first = self.first_seen.setdefault(key, now)
            if now - first >= self.persist and key not in self.orphans:
                self.orphans[key] = describe(
                    "orphan", pid, ppids, commands, cwd,
                    " seen_for={:.1f}s".format(now - first),
                )

    def stop(self):
        self.stopping.set()
        self.join()


def end_check(directory, root, me):
    """Return (live, dirs) at the end; details go to stderr."""
    ppids, commands, cwds = snapshot()
    live = 0
    for pid in sorted(cwds):
        if pid == me or pid not in ppids or not under(cwds[pid], root):
            continue
        live += 1
        err(describe("live process", pid, ppids, commands, cwds[pid]))
    entries = fixture_entries(directory)
    for name in entries:
        err("fixture entry left: {}".format(os.path.join(directory, name)))
    return live, len(entries)


def mode_run(argv):
    if len(argv) < 3 or argv[1] != "--":
        raise Refused("run needs <dir> -- <command> [args...]")
    directory, command = argv[0], argv[2:]
    interval = env_secs("BAUDE_LEFTOVER_SAMPLE_SECS", 2.0)
    persist = env_secs("BAUDE_LEFTOVER_PERSIST_SECS", 5.0)
    tree = enclosing_work_tree(directory)
    if tree is not None:
        raise Refused(
            "refusing {}: it lies inside the git work tree {}; fixture roots "
            "there are admitted as checkouts".format(directory, tree)
        )
    os.makedirs(directory, exist_ok=True)
    stale = fixture_entries(directory)
    if stale:
        raise Refused(
            "refusing {}: it already holds fixture entries: {}".format(
                directory, " ".join(stale)
            )
        )
    root = os.path.realpath(directory)
    me = os.getpid()
    # Fail before the command starts if the process table cannot be read.
    snapshot()

    sampler = Sampler(root, me, interval, persist)
    sampler.start()
    env = dict(os.environ)
    # Exactly as given, never its realpath: tests rely on the /var form.
    env["TMPDIR"] = directory
    try:
        child = subprocess.Popen(
            command, env=env, stdin=subprocess.DEVNULL, stdout=sys.stderr.fileno()
        )
    except OSError as error:
        err("cannot start {}: {}".format(command[0], error))
        rc = 127
    else:
        try:
            rc = child.wait()
        except KeyboardInterrupt:
            # The command shares our process group and got the same signal.
            child.wait()
            sampler.stop()
            raise InternalError("interrupted")
        if rc < 0:
            rc = 128 - rc
    sampler.stop()
    if sampler.error is not None:
        raise InternalError("sampling failed: {}".format(sampler.error))

    for key in sorted(sampler.orphans):
        err(sampler.orphans[key])
    orphans = len(sampler.orphans)
    live, dirs = end_check(directory, root, me)
    sys.stdout.write(summary(rc, orphans, live, dirs) + "\n")
    return 0 if (rc, orphans, live, dirs) == (0, 0, 0, 0) else 1


def mode_check(argv):
    if len(argv) != 1:
        raise Refused("check needs exactly one <dir>")
    directory = argv[0]
    if not os.path.isdir(directory):
        raise Refused("{} is not a directory".format(directory))
    live, dirs = end_check(directory, os.path.realpath(directory), os.getpid())
    sys.stdout.write(summary("-", 0, live, dirs) + "\n")
    return 0 if (live, dirs) == (0, 0) else 1


class SelfTest(object):
    def __init__(self, out):
        self.out = out
        self.checks = 0
        self.failures = 0

    def check(self, name, actual, expected):
        self.checks += 1
        if actual == expected:
            self.out.write("  ok   {}\n".format(name))
            return True
        self.failures += 1
        self.out.write(
            "  FAIL {}\n       expected {!r}\n       got      {!r}\n".format(
                name, expected, actual
            )
        )
        return False


def scratch_base():
    return os.environ.get("TMPDIR") or "/tmp"


def invoke(args, extra_env=None, timeout=60):
    script = os.environ.get("BAUDE_ASSERT_SCRIPT")
    shell = os.environ.get("BAUDE_ASSERT_BASH") or "/bin/bash"
    env = dict(os.environ)
    env["BAUDE_ASSERT_PYTHON"] = sys.executable
    if extra_env:
        env.update(extra_env)
    try:
        completed = subprocess.run(
            [shell, script] + list(args),
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return None, "", "timed out after {}s".format(timeout)
    return (
        completed.returncode,
        completed.stdout.decode("utf-8", "replace"),
        completed.stderr.decode("utf-8", "replace"),
    )


def summary_case(test, name, args, expect_rc, expect_line, setup=None):
    scratch = tempfile.mkdtemp(prefix="leftover-selftest.", dir=scratch_base())
    try:
        if setup:
            setup(scratch)
        argv = [a.replace("{dir}", scratch) for a in args]
        rc, out, err = invoke(argv)
        ok = test.check(name, (rc, out), (expect_rc, expect_line + "\n"))
        if not ok and err:
            test.out.write("       stderr: {}\n".format(err.strip().replace("\n", "\n               ")))
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


def fixture_entry_cases(test):
    summary_case(
        test, "run -- true is clean",
        ["run", "{dir}", "--", "true"],
        0, summary(0, 0, 0, 0),
    )
    summary_case(
        test, "a baude- directory left behind fails with dirs=1",
        ["run", "{dir}", "--", "sh", "-c", 'mkdir "$TMPDIR/baude-left"'],
        1, summary(0, 0, 0, 1),
    )
    summary_case(
        test, "a bauded- file fails with dirs=1 and another name does not count",
        ["run", "{dir}", "--", "sh", "-c", 'touch "$TMPDIR/bauded-file" "$TMPDIR/other"'],
        1, summary(0, 0, 0, 1),
    )
    summary_case(
        test, "a failing command fails with command=1",
        ["run", "{dir}", "--", "false"],
        1, summary(1, 0, 0, 0),
    )
    summary_case(
        test, "check alone counts a baude- entry",
        ["check", "{dir}"],
        1, summary("-", 0, 0, 1),
        setup=lambda d: os.mkdir(os.path.join(d, "baude-x")),
    )

    scratch = tempfile.mkdtemp(prefix="leftover-selftest.", dir=scratch_base())
    try:
        os.mkdir(os.path.join(scratch, "pre"))
        os.mkdir(os.path.join(scratch, "pre", "baude-stale"))
        rc, out, err = invoke(["run", os.path.join(scratch, "pre"), "--", "true"])
        test.check("run refuses a dir already holding a baude- entry [status]", rc, 2)
        test.check(
            "run refuses a dir already holding a baude- entry [names it]",
            "baude-stale" in err, True,
        )
    finally:
        shutil.rmtree(scratch, ignore_errors=True)

    scratch = tempfile.mkdtemp(prefix="leftover-selftest.", dir=scratch_base())
    try:
        repo = os.path.join(scratch, "repo")
        os.mkdir(repo)
        init = subprocess.run(
            ["git", "init", "-q", repo],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        )
        if test.check("git init for the work-tree case", init.returncode, 0):
            target = os.path.join(repo, "tmp")
            rc, out, err = invoke(["run", target, "--", "true"])
            test.check("run inside a git work tree is refused [status]", rc, 2)
            test.check(
                "run inside a git work tree is refused [names the work tree]",
                "git work tree" in err and os.path.realpath(repo) in err, True,
            )
            test.check(
                "run inside a git work tree is refused [does not create the dir]",
                os.path.exists(target), False,
            )
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


# Short windows keep the whole self-test under a minute.
PROCESS_ENV = {"BAUDE_LEFTOVER_SAMPLE_SECS": "1", "BAUDE_LEFTOVER_PERSIST_SECS": "2"}

# A detached `sleep` started from a subshell already inside the folder, so its
# cwd is set from fork and its pid is on disk before the command goes on. Its
# descriptors are closed so it never holds the self-test's output pipes open.
DETACH = (
    'mkdir "$TMPDIR/{w}" && (cd "$TMPDIR/{w}" && '
    '{{ sleep {secs} </dev/null >/dev/null 2>&1 & echo $! > "$TMPDIR/{w}.pid"; }})'
)


def kill_recorded(scratch, names):
    """Clean up the self-test's own processes, by recorded pid only."""
    for name in names:
        try:
            with open(os.path.join(scratch, name + ".pid")) as handle:
                pid = int(handle.read().strip())
        except (OSError, ValueError):
            continue
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass
        # Inside a container this process may be pid 1 and so the orphan's
        # parent: reap it there; elsewhere it is not our child.
        try:
            os.waitpid(pid, os.WNOHANG)
        except OSError:
            pass


def process_case(test, name, body, expect_rc, expect_line, pid_names=(), names_cwd=None):
    scratch = tempfile.mkdtemp(prefix="leftover-selftest.", dir=scratch_base())
    try:
        rc, out, err = invoke(
            ["run", scratch, "--", "sh", "-c", body], extra_env=PROCESS_ENV
        )
        ok = test.check(name, (rc, out), (expect_rc, expect_line + "\n"))
        if names_cwd is not None:
            # The reported folder is the plain path: lsof's on macOS, the procfs
            # link with its ` (deleted)` suffix stripped on Linux.
            wanted = "cwd={} command=".format(
                os.path.join(os.path.realpath(scratch), names_cwd)
            )
            ok = test.check(
                "{} [names {}]".format(name, wanted), wanted in err, True
            ) and ok
        if not ok and err:
            test.out.write("       stderr: {}\n".format(err.strip().replace("\n", "\n               ")))
    finally:
        kill_recorded(scratch, pid_names)
        shutil.rmtree(scratch, ignore_errors=True)


def process_cases(test):
    probe = tempfile.mkdtemp(prefix="leftover-selftest.", dir=scratch_base())
    try:
        if os.path.realpath(probe) == probe:
            test.out.write(
                "  note: {} is already canonical, so the path-form comparison "
                "was not exercised on this host\n".format(probe)
            )
        else:
            test.out.write(
                "  note: path form exercised: {} resolves to {}\n".format(
                    probe, os.path.realpath(probe)
                )
            )
    finally:
        shutil.rmtree(probe, ignore_errors=True)

    process_case(
        test, "an orphan seen across the persist window fails with orphans=1",
        DETACH.format(w="w", secs=9)
        + '; sleep 5; kill "$(cat "$TMPDIR/w.pid")"; sleep 1',
        1, summary(0, 1, 0, 0), pid_names=("w",),
    )
    process_case(
        test, "a descendant working in the dir past the window is not an orphan",
        'mkdir "$TMPDIR/w2" && cd "$TMPDIR/w2" && sleep 4',
        0, summary(0, 0, 0, 0),
    )
    process_case(
        test, "a process still in the dir after the command fails with live=1",
        DETACH.format(w="w3", secs=6),
        1, summary(0, 0, 1, 0), pid_names=("w3",),
    )
    process_case(
        test, "a live process whose folder was deleted still counts as live",
        DETACH.format(w="w4", secs=6) + '; rm -rf "$TMPDIR/w4"',
        1, summary(0, 0, 1, 0), pid_names=("w4",), names_cwd="w4",
    )


def mode_self_test(out):
    test = SelfTest(out)
    out.write("fixture entries and refusals:\n")
    fixture_entry_cases(test)
    out.write("orphaned and live processes:\n")
    process_cases(test)
    out.write("\n{} checks, {} failures\n".format(test.checks, test.failures))
    return 0 if test.failures == 0 else 1


USAGE = (
    "usage: assert-no-test-leftovers.sh run <dir> -- <command> [args...]\n"
    "       assert-no-test-leftovers.sh check <dir>\n"
    "       assert-no-test-leftovers.sh --self-test\n"
)


def main(argv):
    if not argv:
        sys.stderr.write(USAGE)
        return 2
    mode = argv[0]
    try:
        if mode == "--self-test" and len(argv) == 1:
            return mode_self_test(sys.stdout)
        if mode == "run":
            return mode_run(argv[1:])
        if mode == "check":
            return mode_check(argv[1:])
    except (Refused, InternalError) as refusal:
        err(str(refusal))
        return 2
    except Exception:
        # An uncaught exception would exit 1, which reads as a leftover verdict.
        # Anything unexpected is an internal error instead.
        traceback.print_exc()
        err("internal error")
        return 2
    sys.stderr.write(USAGE)
    return 2


# The status is returned directly and exits the process. It is never piped into
# another command: a pipeline reports only its last stage and would hide the
# very failure this script exists to surface.
sys.exit(main(sys.argv[1:]))
PY
