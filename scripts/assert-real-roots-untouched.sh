#!/usr/bin/env bash
#
# Assert that running the test suite leaves the developer's (or the CI
# runner's) REAL baude roots untouched.
#
# The per-resolver guards in baude-core prove one resolution at a time cannot
# escape a fixture. Nothing proved the aggregate claim the phase goal actually
# makes: that *running the suite* neither creates nor modifies the real config
# dir, the real Claude config dir, the real managed-worktrees root, or the
# clone destination root a `git clone` would be written into. This script is
# that statement, made from OUTSIDE the test process.
#
#   assert-real-roots-untouched.sh before      record a fingerprint of the
#                                              four real roots
#   assert-real-roots-untouched.sh after       recompute and fail if anything
#                                              changed
#   assert-real-roots-untouched.sh --self-test exercise the resolvers and the
#                                              before/after comparison against
#                                              synthetic trees only
#
# `before` and `after` OBSERVE whatever roots resolve in the environment they
# are given; they never write to them. To rehearse the bracket locally without
# reading a developer's real account directories, run both the script and the
# suite with HOME, XDG_CONFIG_HOME, XDG_DATA_HOME and CLAUDE_CONFIG_DIR pointed
# at a throwaway fixture root — the same resolver precedence then lands entirely
# inside the fixture, the clone root included (it is HOME-relative unless
# `clone_base_dir` names an absolute path). `--self-test` never observes a real
# root at all.
#
# Exit codes: 0 pass, 1 a root changed (or the snapshots cannot be compared),
# 2 usage or internal error.
set -eu

# The self-test re-invokes this file with EXPLICIT child environments, which
# means the child cannot rely on anything PATH-resolved in the parent's shell
# (a version-manager shim, for instance, needs state the synthetic HOME does not
# have). Hand down the concrete script, shell and interpreter instead; argv[0]
# is "-" under the heredoc, so the path has to be passed deliberately anyway.
BAUDE_ASSERT_SCRIPT="${BASH_SOURCE[0]:-$0}"
BAUDE_ASSERT_BASH="${BASH:-/bin/bash}"
export BAUDE_ASSERT_SCRIPT BAUDE_ASSERT_BASH

exec "${BAUDE_ASSERT_PYTHON:-python3}" - "$@" <<'PY'
"""Fingerprint the four real baude roots and compare two fingerprints.

Everything here is Python 3 standard library: no package install, and lossless
handling of non-UTF-8 path bytes (which a shell `ls | sort` pipeline cannot
give us, and which is also why no gate in this file is a pipeline -- a pipeline
reports only its last stage's status and would mask the failure we care about).
"""

import base64
import errno
import hashlib
import json
import os
import pwd
import shutil
import stat
import subprocess
import sys
import tempfile

SCHEMA_VERSION = 2

# Ordered so diagnostics always read config -> claude -> worktrees -> clone.
ROOT_NAMES = ("config", "claude", "worktrees", "clone")

ROOT_LABELS = {
    "config": "config and state root",
    "claude": "Claude config root",
    "worktrees": "managed worktrees root",
    "clone": "clone destination root",
}

# How deep each root is walked. `None` is the whole subtree.
#
# The clone root is the exception, and it is bounded rather than omitted. It is
# the developer's entire code tree -- a full walk would be unaffordable and its
# fingerprint would churn on every unrelated build -- but the thing this root
# exists to catch has a known shape: `baude` prefills its clone destination as
# `<clone base>/<host>/<owner>/<repo>` and then runs a real `git clone` into it.
# Three levels covers every directory such a clone CREATES; what changes inside
# an already-present repository is somebody's editor, not this test suite.
ROOT_DEPTH = {"clone": 3}

# `Config::clone_base_dir`'s documented default, in `baude-core/src/persist.rs`.
CLONE_BASE_DEFAULT = "~/Code"

MAX_DIFF_LINES = 40


# --------------------------------------------------------------------------
# Resolver
#
# These mirror `persist::real_config_base`, `meta::real_claude_config_dir` and
# `git::real_worktrees_base` exactly, including the details that are easy to
# get wrong in shell:
#
#   * Rust reads `std::env::var_os(..)`, so a variable that is PRESENT BUT
#     EMPTY still wins. `${XDG_CONFIG_HOME:-default}` would silently fall
#     through in that case and fingerprint the wrong directory.
#   * `dirs::home_dir()` on Unix is nonempty $HOME, then the passwd entry --
#     not a bare $HOME expansion.
#   * The Claude root never consults XDG_CONFIG_HOME and never appends
#     "baude"; it is the one chain that does not share the config prefix.
#   * The clone root is not an XDG chain at all: it is a CONFIGURED string
#     (`clone_base_dir`, default "~/Code") expanded through
#     `persist::expand_tilde`, which substitutes "/" when there is no home.
# --------------------------------------------------------------------------


def rust_join(base, *parts):
    """`Path::join` semantics on plain strings.

    Strings rather than `pathlib` on purpose: `Path("")` normalizes to `"."`,
    but Rust's `PathBuf::from("")` stays empty and fails to stat. An empty
    override value is one of the cases this script is supposed to get right.
    """
    out = base
    for part in parts:
        if part.startswith("/"):
            out = part
        elif out == "":
            out = part
        elif out.endswith("/"):
            out = out + part
        else:
            out = out + "/" + part
    return out


def home_dir(environ, passwd_home):
    """`dirs::home_dir()` on Unix: nonempty HOME, else the passwd entry."""
    home = environ.get("HOME")
    if home:
        return home
    if passwd_home:
        return passwd_home
    return None


def expand_tilde(value, home):
    """`persist::expand_tilde`: a leading `~` is the home, or "/" without one."""
    fallback = home if home else "/"
    if value.startswith("~/"):
        return rust_join(fallback, value[2:])
    if value == "~":
        return fallback
    return value


def resolve_roots(environ, passwd_home, clone_base=None):
    """The four real roots, as the application would resolve them.

    `clone_base` is the configured `clone_base_dir`, or None for its default.
    It is a PARAMETER rather than a read so this stays a pure function of its
    inputs; `resolve_roots_from_disk` supplies the recorded value.
    """
    home = home_dir(environ, passwd_home)

    config_base = environ.get("XDG_CONFIG_HOME")
    if config_base is None:
        config_base = rust_join(home, ".config") if home is not None else "."

    claude = environ.get("CLAUDE_CONFIG_DIR")
    if claude is None:
        claude = rust_join(home, ".claude") if home is not None else "."

    data_base = environ.get("XDG_DATA_HOME")
    if data_base is None:
        data_base = (
            rust_join(home, ".local", "share") if home is not None else "/tmp"
        )

    return {
        "config": rust_join(config_base, "baude"),
        "claude": claude,
        "worktrees": rust_join(data_base, "baude", "worktrees"),
        "clone": expand_tilde(
            clone_base if clone_base else CLONE_BASE_DEFAULT, home
        ),
    }


def read_clone_base(config_root):
    """`clone_base_dir` from the real config.json, or None for the default.

    Reading a file changes its atime and nothing else, and the fingerprint
    records size and mtime only -- so consulting the config is not itself a
    modification of the root being observed.
    """
    try:
        with open(rust_join(config_root, "config.json"), "r", encoding="utf-8") as h:
            parsed = json.load(h)
    except (OSError, ValueError):
        return None
    if not isinstance(parsed, dict):
        return None
    value = parsed.get("clone_base_dir")
    return value if isinstance(value, str) and value else None


def resolve_roots_from_disk(environ, passwd_home):
    """`resolve_roots`, with `clone_base_dir` taken from the real config.json.

    Two passes, because the setting lives inside the config root this same
    function resolves.
    """
    first = resolve_roots(environ, passwd_home)
    configured = read_clone_base(first["config"])
    if configured is None:
        return first
    return resolve_roots(environ, passwd_home, clone_base=configured)


def real_passwd_home():
    try:
        return pwd.getpwuid(os.getuid()).pw_dir or None
    except KeyError:
        return None


# --------------------------------------------------------------------------
# Fingerprinting
# --------------------------------------------------------------------------


def kind_of(mode):
    if stat.S_ISDIR(mode):
        return "dir"
    if stat.S_ISLNK(mode):
        return "link"
    if stat.S_ISREG(mode):
        return "file"
    return "other"


def entry_for(rel_bytes, st, root_bytes):
    kind = kind_of(st.st_mode)
    extra = b""
    if kind == "link":
        target_path = (
            os.path.join(root_bytes, rel_bytes) if rel_bytes != b"." else root_bytes
        )
        try:
            extra = os.readlink(target_path)
        except OSError:
            extra = b"<unreadable-link>"
    return (rel_bytes, kind, st.st_size, st.st_mtime_ns, extra)


def walk_root(root, max_depth=None):
    """Sorted (relpath, kind, size, mtime_ns, extra) tuples, or None if absent.

    Absence is a recorded STATE, not an error: CI runners legitimately have
    none of these directories, and "absent before and absent after" is the
    expected pass there. "Absent before, present after" is the leak.

    Directories carry their own mtime, so a file that is created and deleted
    again inside the run still shows up as a change.

    `max_depth` bounds the descent (see ROOT_DEPTH). Entries AT the bound are
    still recorded -- only their contents are left unread -- so a directory
    appearing at the bound is still a detected change.
    """
    if root == "":
        return None
    root_bytes = os.fsencode(root)
    try:
        st = os.lstat(root_bytes)
    except OSError as err:
        if err.errno in (errno.ENOENT, errno.ENOTDIR, errno.ENAMETOOLONG):
            return None
        return [(b".", "unreadable", 0, 0, os.fsencode(err.strerror or "error"))]

    entries = [entry_for(b".", st, root_bytes)]
    if not stat.S_ISDIR(st.st_mode):
        return entries

    pending = [(b"", 0)]
    while pending:
        rel_dir, depth = pending.pop()
        dir_bytes = os.path.join(root_bytes, rel_dir) if rel_dir else root_bytes
        try:
            names = os.listdir(dir_bytes)
        except OSError as err:
            entries.append(
                (
                    rel_dir or b".",
                    "unreadable",
                    0,
                    0,
                    os.fsencode(err.strerror or "error"),
                )
            )
            continue
        for name in names:
            rel = os.path.join(rel_dir, name) if rel_dir else name
            child = os.path.join(root_bytes, rel)
            try:
                child_st = os.lstat(child)
            except OSError as err:
                entries.append(
                    (rel, "unreadable", 0, 0, os.fsencode(err.strerror or "error"))
                )
                continue
            entries.append(entry_for(rel, child_st, root_bytes))
            if (
                stat.S_ISDIR(child_st.st_mode)
                and not stat.S_ISLNK(child_st.st_mode)
                and (max_depth is None or depth + 1 < max_depth)
            ):
                pending.append((rel, depth + 1))

    entries.sort(key=lambda item: item[0])
    return entries


def digest_of(entries):
    if entries is None:
        return "absent"
    hasher = hashlib.sha256()
    for rel, kind, size, mtime_ns, extra in entries:
        hasher.update(rel)
        hasher.update(b"\t")
        hasher.update(kind.encode("ascii"))
        hasher.update(b"\t")
        hasher.update(str(size).encode("ascii"))
        hasher.update(b"\t")
        hasher.update(str(mtime_ns).encode("ascii"))
        hasher.update(b"\t")
        hasher.update(extra)
        hasher.update(b"\n")
    return hasher.hexdigest()


def encode_entries(entries):
    """JSON-safe, lossless: raw path bytes as base64, plus a display form."""
    if entries is None:
        return None
    return [
        {
            "path": base64.b64encode(rel).decode("ascii"),
            "display": os.fsdecode(rel).encode("utf-8", "replace").decode("utf-8"),
            "kind": kind,
            "size": size,
            "mtime_ns": mtime_ns,
            "extra": base64.b64encode(extra).decode("ascii"),
        }
        for rel, kind, size, mtime_ns, extra in entries
    ]


def snapshot(environ, passwd_home):
    roots = resolve_roots_from_disk(environ, passwd_home)
    recorded = {}
    for name in ROOT_NAMES:
        entries = walk_root(roots[name], ROOT_DEPTH.get(name))
        recorded[name] = {
            "path": roots[name],
            "exists": entries is not None,
            "digest": digest_of(entries),
            "entries": encode_entries(entries),
        }
    return {"schema": SCHEMA_VERSION, "roots": recorded}


# --------------------------------------------------------------------------
# Snapshot storage
# --------------------------------------------------------------------------


def snapshot_path(environ):
    base = (
        environ.get("BAUDE_ROOT_SNAPSHOT_DIR")
        or environ.get("RUNNER_TEMP")
        or tempfile.gettempdir()
    )
    return os.path.join(base, "baude-real-root-assertion", "before.json")


def is_inside(candidate, root):
    if root == "" or candidate == "":
        return False
    root = root.rstrip("/") or "/"
    return candidate == root or candidate.startswith(root + "/")


def assert_snapshot_is_outside_every_root(path, roots, out):
    """The observer's own state must not be part of what it observes."""
    directory = os.path.dirname(path)
    for name in ROOT_NAMES:
        root = roots[name]
        if is_inside(directory, root) or is_inside(root, directory):
            out.write(
                "FATAL: the snapshot directory {} overlaps the {} ({}).\n"
                "Set BAUDE_ROOT_SNAPSHOT_DIR to a location outside every "
                "observed root.\n".format(directory, ROOT_LABELS[name], root)
            )
            return False
    return True


# --------------------------------------------------------------------------
# Comparison
# --------------------------------------------------------------------------


def index(encoded):
    return {item["path"]: item for item in (encoded or [])}


def describe(item):
    return "{} {} size={} mtime_ns={}".format(
        item["kind"], item["display"] or ".", item["size"], item["mtime_ns"]
    )


def report_root(name, before, after, out):
    """Print what changed under one root. Returns True when it is unchanged."""
    label = ROOT_LABELS[name]

    if before["path"] != after["path"]:
        out.write(
            "CHANGED  {}: resolved to a DIFFERENT path between the two "
            "observations\n           before: {}\n           after:  {}\n".format(
                label, before["path"] or "<empty>", after["path"] or "<empty>"
            )
        )
        return False

    if before["digest"] == after["digest"] and before["exists"] == after["exists"]:
        state = "present" if after["exists"] else "absent"
        out.write("ok       {}: unchanged ({}, {})\n".format(label, state, after["path"] or "<empty>"))
        return True

    if not before["exists"] and after["exists"]:
        out.write(
            "CHANGED  {}: CREATED by the run at {}\n".format(label, after["path"])
        )
    elif before["exists"] and not after["exists"]:
        out.write(
            "CHANGED  {}: REMOVED by the run at {}\n".format(label, after["path"])
        )
    else:
        out.write("CHANGED  {}: modified at {}\n".format(label, after["path"]))

    old = index(before["entries"])
    new = index(after["entries"])
    added = [new[key] for key in new if key not in old]
    removed = [old[key] for key in old if key not in new]
    modified = [
        (old[key], new[key])
        for key in new
        if key in old
        and (
            old[key]["kind"] != new[key]["kind"]
            or old[key]["size"] != new[key]["size"]
            or old[key]["mtime_ns"] != new[key]["mtime_ns"]
            or old[key]["extra"] != new[key]["extra"]
        )
    ]

    def dump(title, items, render):
        if not items:
            return
        out.write("           {} ({}):\n".format(title, len(items)))
        for item in items[:MAX_DIFF_LINES]:
            out.write("             {}\n".format(render(item)))
        if len(items) > MAX_DIFF_LINES:
            out.write(
                "             ... and {} more\n".format(len(items) - MAX_DIFF_LINES)
            )

    dump("added", sorted(added, key=lambda i: i["display"]), describe)
    dump("removed", sorted(removed, key=lambda i: i["display"]), describe)
    dump(
        "modified",
        sorted(modified, key=lambda pair: pair[1]["display"]),
        lambda pair: "{}  ->  {}".format(describe(pair[0]), describe(pair[1])),
    )
    return False


def compare(before, after, out):
    unchanged = True
    for name in ROOT_NAMES:
        # Evaluate every root before short-circuiting: a developer fixing one
        # leak wants to see all of them, not the first.
        if not report_root(name, before["roots"][name], after["roots"][name], out):
            unchanged = False
    return unchanged


# --------------------------------------------------------------------------
# Modes
# --------------------------------------------------------------------------


def mode_before(environ, passwd_home, out):
    roots = resolve_roots_from_disk(environ, passwd_home)
    path = snapshot_path(environ)
    if not assert_snapshot_is_outside_every_root(path, roots, out):
        return 2
    os.makedirs(os.path.dirname(path), exist_ok=True)
    recorded = snapshot(environ, passwd_home)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(recorded, handle)
    out.write("recorded real-root fingerprint -> {}\n".format(path))
    for name in ROOT_NAMES:
        entry = recorded["roots"][name]
        out.write(
            "  {:<22} {:<8} {}\n".format(
                ROOT_LABELS[name],
                "present" if entry["exists"] else "absent",
                entry["path"] or "<empty>",
            )
        )
    return 0


def mode_after(environ, passwd_home, out):
    path = snapshot_path(environ)
    try:
        with open(path, "r", encoding="utf-8") as handle:
            before = json.load(handle)
    except OSError as err:
        out.write(
            "FATAL: cannot read the 'before' snapshot at {} ({}). "
            "Run the 'before' mode first.\n".format(path, err)
        )
        return 2
    except ValueError as err:
        out.write("FATAL: the 'before' snapshot at {} is corrupt ({}).\n".format(path, err))
        return 2

    if before.get("schema") != SCHEMA_VERSION:
        out.write(
            "FATAL: snapshot schema {} does not match {}.\n".format(
                before.get("schema"), SCHEMA_VERSION
            )
        )
        return 2

    after = snapshot(environ, passwd_home)
    unchanged = compare(before, after, out)
    if unchanged:
        out.write("PASS: the run left all four real roots untouched.\n")
        return 0
    out.write(
        "FAIL: the run touched a real root. Every effect a test has must land "
        "inside an injected fixture root.\n"
    )
    return 1


# --------------------------------------------------------------------------
# Self-test
#
# Synthetic directories and explicit child environment maps only. It never
# mutates this process's environment and never observes a real account root:
# every home-lookup input is injected.
# --------------------------------------------------------------------------


class SelfTest(object):
    def __init__(self, out):
        self.out = out
        self.failures = 0
        self.checks = 0

    def check(self, name, actual, expected):
        self.checks += 1
        if actual == expected:
            self.out.write("  ok   {}\n".format(name))
        else:
            self.failures += 1
            self.out.write(
                "  FAIL {}\n         expected: {!r}\n         actual:   {!r}\n".format(
                    name, expected, actual
                )
            )


def resolver_table(test):
    cases = [
        (
            "all three overrides present win outright",
            {
                "HOME": "/home/dev",
                "XDG_CONFIG_HOME": "/x/config",
                "XDG_DATA_HOME": "/x/data",
                "CLAUDE_CONFIG_DIR": "/x/claude",
            },
            "/pw/home",
            {
                "config": "/x/config/baude",
                "claude": "/x/claude",
                "worktrees": "/x/data/baude/worktrees",
                "clone": "/home/dev/Code",
            },
        ),
        (
            "CLAUDE_CONFIG_DIR beats HOME and ignores XDG_CONFIG_HOME",
            {"HOME": "/home/dev", "XDG_CONFIG_HOME": "/x/config", "CLAUDE_CONFIG_DIR": "/c"},
            "/pw/home",
            {
                "config": "/x/config/baude",
                "claude": "/c",
                "worktrees": "/home/dev/.local/share/baude/worktrees",
                "clone": "/home/dev/Code",
            },
        ),
        (
            "without CLAUDE_CONFIG_DIR the Claude root is HOME/.claude, never XDG",
            {"HOME": "/home/dev", "XDG_CONFIG_HOME": "/x/config"},
            "/pw/home",
            {
                "config": "/x/config/baude",
                "claude": "/home/dev/.claude",
                "worktrees": "/home/dev/.local/share/baude/worktrees",
                "clone": "/home/dev/Code",
            },
        ),
        (
            "HOME fallback for every root when no override is present",
            {"HOME": "/home/dev"},
            "/pw/home",
            {
                "config": "/home/dev/.config/baude",
                "claude": "/home/dev/.claude",
                "worktrees": "/home/dev/.local/share/baude/worktrees",
                "clone": "/home/dev/Code",
            },
        ),
        (
            "present-but-EMPTY overrides still win (var_os presence, not :- default)",
            {
                "HOME": "/home/dev",
                "XDG_CONFIG_HOME": "",
                "XDG_DATA_HOME": "",
                "CLAUDE_CONFIG_DIR": "",
            },
            "/pw/home",
            {
                "config": "baude",
                "claude": "",
                "worktrees": "baude/worktrees",
                "clone": "/home/dev/Code",
            },
        ),
        (
            "empty HOME falls through to the injected passwd home",
            {"HOME": ""},
            "/pw/home",
            {
                "config": "/pw/home/.config/baude",
                "claude": "/pw/home/.claude",
                "worktrees": "/pw/home/.local/share/baude/worktrees",
                "clone": "/pw/home/Code",
            },
        ),
        (
            "absent HOME falls through to the injected passwd home",
            {},
            "/pw/home",
            {
                "config": "/pw/home/.config/baude",
                "claude": "/pw/home/.claude",
                "worktrees": "/pw/home/.local/share/baude/worktrees",
                "clone": "/pw/home/Code",
            },
        ),
        (
            "no home at all lands on the terminal fallbacks . / . / /tmp / /Code",
            {},
            None,
            {
                "config": "./baude",
                "claude": ".",
                "worktrees": "/tmp/baude/worktrees",
                "clone": "/Code",
            },
        ),
    ]
    for name, environ, passwd_home, expected in cases:
        test.check(name, resolve_roots(environ, passwd_home), expected)


def child_env(fixture, snapshot_dir, home=None):
    """An EXPLICIT child environment. Nothing is inherited from this process.

    The interpreter is pinned to the one already running rather than left to
    PATH: with a synthetic HOME and XDG_DATA_HOME, a version-manager shim on
    PATH cannot find its own state and fails before the script ever runs.
    """
    env = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "HOME": home if home is not None else os.path.join(fixture, "home"),
        "XDG_CONFIG_HOME": os.path.join(fixture, "config"),
        "XDG_DATA_HOME": os.path.join(fixture, "data"),
        "CLAUDE_CONFIG_DIR": os.path.join(fixture, "claude"),
        "BAUDE_ROOT_SNAPSHOT_DIR": snapshot_dir,
        "BAUDE_ASSERT_PYTHON": sys.executable,
    }
    return env


def run_mode(env, mode):
    script = os.environ.get("BAUDE_ASSERT_SCRIPT")
    shell = os.environ.get("BAUDE_ASSERT_BASH") or "/bin/bash"
    completed = subprocess.run(
        [shell, script, mode],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    return completed.returncode, completed.stdout.decode("utf-8", "replace")


def synthetic_roots(env):
    return (
        os.path.join(env["XDG_CONFIG_HOME"], "baude"),
        env["CLAUDE_CONFIG_DIR"],
        os.path.join(env["XDG_DATA_HOME"], "baude", "worktrees"),
    )


def bracket(test, name, fixture_name, seed, mutate, expect_pass, expect_names=()):
    """Run before / mutate / after against synthetic trees and check the status."""
    with tempfile.TemporaryDirectory(prefix="baude-selftest-") as scratch:
        fixture = os.path.join(scratch, fixture_name)
        snapshot_dir = os.path.join(scratch, "snapshots")
        os.makedirs(fixture)
        env = child_env(fixture, snapshot_dir)
        seed(env)

        status, output = run_mode(env, "before")
        if status != 0:
            test.check("{} [before]".format(name), (status, output), (0, "<clean>"))
            return

        # Falsification happens ONLY inside this synthetic tree.
        mutate(env)

        status, output = run_mode(env, "after")
        test.check("{} [status]".format(name), status, 0 if expect_pass else 1)
        for expected_name in expect_names:
            test.check(
                "{} [names {}]".format(name, expected_name),
                expected_name in output,
                True,
            )


def write_file(path, contents):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(contents)


def comparison_table(test):
    def seed_all(env):
        config, claude, worktrees = synthetic_roots(env)
        write_file(os.path.join(config, "config.json"), "{}\n")
        write_file(os.path.join(claude, "settings.json"), "{}\n")
        os.makedirs(os.path.join(worktrees, "claude"), exist_ok=True)

    bracket(
        test,
        "unchanged synthetic roots pass",
        "unchanged",
        seed_all,
        lambda env: None,
        expect_pass=True,
    )

    bracket(
        test,
        "absent before and absent after is a pass",
        "absent",
        lambda env: None,
        lambda env: None,
        expect_pass=True,
    )

    bracket(
        test,
        "a root CREATED during the run fails and is named",
        "created",
        lambda env: None,
        lambda env: os.makedirs(
            os.path.join(synthetic_roots(env)[2], "claude", "repository-1")
        ),
        expect_pass=False,
        expect_names=("managed worktrees root", "CREATED"),
    )

    bracket(
        test,
        "a MODIFIED file under the config root fails and is named",
        "modified",
        seed_all,
        lambda env: write_file(
            os.path.join(synthetic_roots(env)[0], "config.json"),
            '{"editor_cmd":"leaked"}\n',
        ),
        expect_pass=False,
        expect_names=("config and state root", "modified", "config.json"),
    )

    bracket(
        test,
        "a NEW file under the Claude root fails and is named",
        "claude-added",
        seed_all,
        lambda env: write_file(
            os.path.join(synthetic_roots(env)[1], "projects", "leaked.jsonl"),
            "{}\n",
        ),
        expect_pass=False,
        expect_names=("Claude config root", "added", "leaked.jsonl"),
    )

    bracket(
        test,
        "a REMOVED root fails and is named",
        "removed",
        seed_all,
        lambda env: shutil.rmtree(synthetic_roots(env)[1]),
        expect_pass=False,
        expect_names=("Claude config root", "REMOVED"),
    )


def clone_root_table(test):
    """The clone destination root: configured, not named by any XDG variable.

    It is the one root a leak reaches by WRITING A WHOLE REPOSITORY into it --
    `baude`'s clone modal prefills `<clone base>/<host>/<owner>/<repo>` and
    hands a clear destination to a real `git clone`.
    """
    bracket(
        test,
        "a clone under the default clone base fails and is named",
        "cloned",
        lambda env: None,
        lambda env: os.makedirs(
            os.path.join(env["HOME"], "Code", "github.com", "owner", "repo")
        ),
        expect_pass=False,
        expect_names=("clone destination root", "CREATED"),
    )

    def seed_configured_base(env):
        write_file(
            os.path.join(synthetic_roots(env)[0], "config.json"),
            json.dumps({"clone_base_dir": os.path.join(env["HOME"], "Elsewhere")})
            + "\n",
        )
        os.makedirs(os.path.join(env["HOME"], "Elsewhere"))

    bracket(
        test,
        "a configured clone_base_dir is the root that gets observed",
        "configured-clone",
        seed_configured_base,
        lambda env: os.makedirs(
            os.path.join(env["HOME"], "Elsewhere", "github.com", "owner", "repo")
        ),
        expect_pass=False,
        expect_names=("clone destination root", "added", "repo"),
    )

    # The bound is a deliberate trade, stated as a test rather than only as a
    # comment: this root is the developer's whole code tree, so edits INSIDE an
    # already-present checkout are not the suite's doing and are not observed.
    bracket(
        test,
        "a change below the clone root's depth bound is not reported",
        "clone-depth",
        lambda env: os.makedirs(
            os.path.join(env["HOME"], "Code", "github.com", "owner", "repo", "src")
        ),
        lambda env: write_file(
            os.path.join(
                env["HOME"], "Code", "github.com", "owner", "repo", "src", "main.rs"
            ),
            "fn main() {}\n",
        ),
        expect_pass=True,
    )


def snapshot_location_table(test):
    """The observer must refuse to store its state inside what it observes."""
    with tempfile.TemporaryDirectory(prefix="baude-selftest-loc-") as scratch:
        fixture = os.path.join(scratch, "overlap")
        os.makedirs(fixture)
        env = child_env(fixture, os.path.join(fixture, "claude", "snapshots"))
        status, output = run_mode(env, "before")
        test.check("snapshot inside an observed root is refused", status, 2)
        test.check(
            "snapshot overlap names the offending root",
            "Claude config root" in output,
            True,
        )


def resolution_change_table(test):
    """A root that resolves DIFFERENTLY between the two calls is a failure."""
    with tempfile.TemporaryDirectory(prefix="baude-selftest-res-") as scratch:
        fixture = os.path.join(scratch, "resolution")
        snapshot_dir = os.path.join(scratch, "snapshots")
        os.makedirs(fixture)
        before_env = child_env(fixture, snapshot_dir)
        status, _ = run_mode(before_env, "before")
        test.check("resolution-change [before]", status, 0)

        after_env = dict(before_env)
        after_env["CLAUDE_CONFIG_DIR"] = os.path.join(fixture, "claude-elsewhere")
        status, output = run_mode(after_env, "after")
        test.check("a differently resolved root fails", status, 1)
        test.check(
            "the differently resolved root is named",
            "DIFFERENT path" in output and "Claude config root" in output,
            True,
        )


def mode_self_test(out):
    test = SelfTest(out)
    out.write("resolver precedence (pure, injected home inputs):\n")
    resolver_table(test)
    out.write("before/after comparison (synthetic trees, explicit child env):\n")
    comparison_table(test)
    out.write("clone destination root:\n")
    clone_root_table(test)
    out.write("snapshot location:\n")
    snapshot_location_table(test)
    out.write("resolution stability:\n")
    resolution_change_table(test)
    out.write(
        "\n{} checks, {} failures\n".format(test.checks, test.failures)
    )
    return 0 if test.failures == 0 else 1


USAGE = (
    "usage: assert-real-roots-untouched.sh (before | after | --self-test)\n"
)


def main(argv):
    out = sys.stdout
    if len(argv) != 1:
        out.write(USAGE)
        return 2
    mode = argv[0]
    if mode == "--self-test":
        return mode_self_test(out)
    if mode == "before":
        return mode_before(os.environ, real_passwd_home(), out)
    if mode == "after":
        return mode_after(os.environ, real_passwd_home(), out)
    out.write(USAGE)
    return 2


# The status is returned directly and exits the process. It is never piped into
# another command: a pipeline reports only its last stage and would hide the
# very failure this script exists to surface.
sys.exit(main(sys.argv[1:]))
PY
