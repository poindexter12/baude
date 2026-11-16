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
import subprocess
import sys
import tempfile

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


def end_check(directory):
    """Return the fixture-entry count at the end; details go to stderr."""
    entries = fixture_entries(directory)
    for name in entries:
        err("fixture entry left: {}".format(os.path.join(directory, name)))
    return len(entries)


def mode_run(argv):
    if len(argv) < 3 or argv[1] != "--":
        raise Refused("run needs <dir> -- <command> [args...]")
    directory, command = argv[0], argv[2:]
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

    env = dict(os.environ)
    env["TMPDIR"] = directory
    try:
        child = subprocess.Popen(
            command, env=env, stdin=subprocess.DEVNULL, stdout=sys.stderr.fileno()
        )
    except OSError as error:
        err("cannot start {}: {}".format(command[0], error))
        rc = 127
    else:
        rc = child.wait()
        if rc < 0:
            rc = 128 - rc

    dirs = end_check(directory)
    sys.stdout.write(summary(rc, 0, 0, dirs) + "\n")
    return 0 if (rc, dirs) == (0, 0) else 1


def mode_check(argv):
    if len(argv) != 1:
        raise Refused("check needs exactly one <dir>")
    directory = argv[0]
    if not os.path.isdir(directory):
        raise Refused("{} is not a directory".format(directory))
    dirs = end_check(directory)
    sys.stdout.write(summary("-", 0, 0, dirs) + "\n")
    return 0 if dirs == 0 else 1


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


def mode_self_test(out):
    test = SelfTest(out)
    out.write("fixture entries and refusals:\n")
    fixture_entry_cases(test)
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
    except Refused as refusal:
        err(str(refusal))
        return 2
    sys.stderr.write(USAGE)
    return 2


# The status is returned directly and exits the process. It is never piped into
# another command: a pipeline reports only its last stage and would hide the
# very failure this script exists to surface.
sys.exit(main(sys.argv[1:]))
PY
