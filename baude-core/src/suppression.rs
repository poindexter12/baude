//! Per-workspace re-admission suppression store.
//!
//! Remembers checkout paths that were removed while their repository row
//! survived, so the `git worktree list` sweep on the next admission of that
//! repository does not bring the row back. Advisory, per-workspace, additive
//! schema; a corrupt or absent file reads as empty and never fails a launch.
//! Entries are kept until lifted by an explicit admission of that exact path
//! or garbage-collected when the parent repository row is removed. An entry
//! written for a removed primary checkout also carries that fact, so the
//! repository derives no new primary while the entry stands.
//!
//! The store lives beside the breadcrumbs file under the config dir as
//! `suppressed-admissions-<workspace>.json`, keyed by
//! [`folder_key`] of the canonical checkout path. It never reads or writes
//! the schema-locked state file; every write goes through the shared
//! advisory-lock merge path so parallel TUIs never clobber each other. The
//! store can only suppress, never admit: a tampered entry hides a worktree
//! until the next explicit admission of that exact path lifts it.
//!
//! All helpers take the config root explicitly so callers pass
//! `persist::config_dir()` and tests pass a scratch dir.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::breadcrumbs::{folder_key, load_json, locked_merge_write};

pub const SCHEMA_VERSION: u32 = 1;
/// Base name; the on-disk file is `suppressed-admissions-<workspace>.json`.
pub const FILE_BASE: &str = "suppressed-admissions";

fn schema_version_default() -> u32 {
    SCHEMA_VERSION
}

/// Whole-file shape. No `deny_unknown_fields`: additive evolution is expected
/// and an older baude reading a newer file must not treat it as corrupt.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SuppressionFile {
    #[serde(default = "schema_version_default")]
    pub schema_version: u32,
    /// Keyed by the canonical checkout path (lossy UTF-8).
    #[serde(default)]
    pub entries: BTreeMap<String, SuppressionEntry>,
}

/// One suppressed checkout path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SuppressionEntry {
    /// `folder_key` of the surviving repository's common dir, so the entry
    /// can be garbage-collected when that repository row is removed.
    pub repository_common_dir: String,
    #[serde(default)]
    pub suppressed_ms: u64,
    /// The removed row was the repository's primary checkout. Absent in a
    /// file written before the flag existed and left out when false, so a
    /// file with no removed primary is written exactly as before.
    #[serde(default, skip_serializing_if = "is_false")]
    pub primary: bool,
}

fn is_false(flag: &bool) -> bool {
    !*flag
}

/// The per-workspace file name, like the breadcrumbs file.
fn file_name() -> String {
    crate::workspace::active().state_file(FILE_BASE)
}

/// Where the store lives under `root` for the active workspace.
pub fn file_path(root: &Path) -> PathBuf {
    root.join(file_name())
}

/// The store as it is on disk right now; absent or corrupt reads as empty.
fn current(root: &Path) -> SuppressionFile {
    load_json::<SuppressionFile>(&file_path(root)).0
}

/// Whether `path` is currently suppressed. Exact `folder_key` match only; a
/// parent or sibling entry does not suppress it. Never errors: an absent or
/// corrupt file reads as empty so a bad store cannot fail a launch.
pub fn is_suppressed(root: &Path, path: &Path) -> bool {
    current(root).entries.contains_key(&folder_key(path))
}

/// The path an explicit admission of `admitted` lifts: the checkout root git
/// reports for it, else its canonical path, else the path as given. The TUI
/// and the daemon both lift this key, so opening a folder lifts the same
/// entry in either client, also when the folder is a subfolder of a
/// checkout. For a linked worktree the root is the worktree itself, never
/// the main worktree, so admitting one never lifts a sibling's entry.
///
/// Residual, by design: a daemon row stored on a subfolder of a checkout
/// writes its entry on that subfolder when it is removed, and `lift_key` of
/// the subfolder names the checkout root, so that entry is never lifted. It
/// is never read either: the TUI sweep consults only the worktree roots git
/// lists, and the daemon consults no entry.
pub fn lift_key(admitted: &Path) -> PathBuf {
    crate::git::repo_root(admitted)
        .or_else(|| admitted.canonicalize().ok())
        .unwrap_or_else(|| admitted.to_path_buf())
}

/// Forget the entry for exactly `path`. Returns `Ok(true)` when an entry was
/// removed and `Ok(false)` without writing when the key or the file is
/// absent, so an untouched file stays byte-identical and an absent file
/// stays absent. Other entries are left intact.
pub fn lift(root: &Path, path: &Path) -> io::Result<bool> {
    let key = folder_key(path);
    if !current(root).entries.contains_key(&key) {
        return Ok(false);
    }
    locked_merge_write(root, &file_name(), |merged: &mut SuppressionFile| {
        merged.schema_version = SCHEMA_VERSION;
        merged.entries.remove(&key);
    })?;
    Ok(true)
}

/// The path of the removed primary checkout of the repository at
/// `common_dir`, if an entry of that repository carries the fact. A query by
/// repository, so it holds when the removed folder is gone from disk. Never
/// errors: an absent or corrupt file reads as empty, and an entry written
/// before the flag existed does not answer.
pub fn removed_primary(root: &Path, common_dir: &Path) -> Option<PathBuf> {
    let common = folder_key(common_dir);
    current(root)
        .entries
        .into_iter()
        .find(|(_, entry)| entry.primary && entry.repository_common_dir == common)
        .map(|(path, _)| PathBuf::from(path))
}

/// Record that `path` was removed while the repository at `common_dir`
/// survived. A later entry for the same path replaces the earlier one.
pub fn suppress(root: &Path, path: &Path, common_dir: &Path, now_ms: u64) -> io::Result<()> {
    write_entry(root, path, common_dir, now_ms, false)
}

/// Like [`suppress`], for the repository's primary checkout: the entry also
/// records that the user removed the primary, which [`removed_primary`]
/// reports until the entry is lifted or dropped.
pub fn suppress_primary(
    root: &Path,
    path: &Path,
    common_dir: &Path,
    now_ms: u64,
) -> io::Result<()> {
    write_entry(root, path, common_dir, now_ms, true)
}

fn write_entry(
    root: &Path,
    path: &Path,
    common_dir: &Path,
    now_ms: u64,
    primary: bool,
) -> io::Result<()> {
    let key = folder_key(path);
    let entry = SuppressionEntry {
        repository_common_dir: folder_key(common_dir),
        suppressed_ms: now_ms,
        primary,
    };
    locked_merge_write(root, &file_name(), |merged: &mut SuppressionFile| {
        merged.schema_version = SCHEMA_VERSION;
        merged.entries.insert(key, entry);
    })
}

/// Garbage-collect every entry whose `repository_common_dir` is
/// `folder_key(common_dir)`, returning how many were dropped. Writes nothing
/// when there are none.
pub fn drop_children(root: &Path, common_dir: &Path) -> io::Result<usize> {
    let common = folder_key(common_dir);
    let count = current(root)
        .entries
        .values()
        .filter(|entry| entry.repository_common_dir == common)
        .count();
    if count == 0 {
        return Ok(0);
    }
    locked_merge_write(root, &file_name(), |merged: &mut SuppressionFile| {
        merged.schema_version = SCHEMA_VERSION;
        merged
            .entries
            .retain(|_, entry| entry.repository_common_dir != common);
    })?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::Config;
    use crate::testing::TestRedirect;
    const WORKSPACE: &str = "suppression-ws";

    /// Fresh scratch dir per test, following the repo's temp-dir convention.
    fn scratch(name: &str) -> crate::testing::FixtureRoot {
        crate::testing::FixtureRoot::new(&format!("baude-suppression-{name}"))
    }

    /// Redirect every fixture path and pin a workspace identity so
    /// `workspace::active()` resolves under test. The redirect's root is the
    /// last element, so it drops after both guards.
    fn identity(name: &str) -> (TestRedirect, TestRedirect, crate::testing::FixtureRoot) {
        let root = scratch(&format!("{name}-root"));
        let redirect = TestRedirect::new(&root);
        let config = Config {
            workspace: Some(WORKSPACE.to_string()),
            ..Config::default()
        };
        let ws = crate::workspace::override_for_test(&config, None);
        (redirect, ws, root)
    }

    /// A real folder under `base` so `folder_key` canonicalizes it the way a
    /// checkout path is canonicalized in production.
    fn folder(base: &Path, name: &str) -> PathBuf {
        let path = base.join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn read(root: &Path) -> SuppressionFile {
        let text = std::fs::read_to_string(file_path(root)).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn suppress_then_consult_then_lift_round_trip() {
        let _guards = identity("roundtrip");
        let dir = scratch("roundtrip");
        let common = folder(&dir, "repo.git");
        let path = folder(&dir, "child");

        assert!(!is_suppressed(&dir, &path));
        assert!(!file_path(&dir).exists());

        suppress(&dir, &path, &common, 10).unwrap();
        assert!(is_suppressed(&dir, &path));
        let on_disk = read(&dir);
        assert_eq!(on_disk.schema_version, SCHEMA_VERSION);
        assert_eq!(on_disk.entries.len(), 1);
        assert_eq!(
            on_disk.entries.get(&folder_key(&path)),
            Some(&SuppressionEntry {
                repository_common_dir: folder_key(&common),
                suppressed_ms: 10,
                primary: false,
            })
        );

        assert!(lift(&dir, &path).unwrap());
        assert!(!is_suppressed(&dir, &path));
        assert!(read(&dir).entries.is_empty());

        // A second lift finds nothing and leaves the bytes alone.
        let bytes_before = std::fs::read(file_path(&dir)).unwrap();
        assert!(!lift(&dir, &path).unwrap());
        assert_eq!(std::fs::read(file_path(&dir)).unwrap(), bytes_before);

        // An absent file stays absent.
        let empty = scratch("roundtrip-empty");
        assert!(!lift(&empty, &path).unwrap());
        assert!(!file_path(&empty).exists());
    }

    #[test]
    fn lift_removes_only_the_exact_key() {
        let _guards = identity("exact");
        let dir = scratch("exact");
        let common = folder(&dir, "repo.git");
        let a = folder(&dir, "a");
        let b = folder(&dir, "b");
        suppress(&dir, &a, &common, 1).unwrap();
        suppress(&dir, &b, &common, 2).unwrap();
        let b_before = read(&dir).entries[&folder_key(&b)].clone();

        assert!(lift(&dir, &a).unwrap());

        let on_disk = read(&dir);
        assert_eq!(on_disk.schema_version, SCHEMA_VERSION);
        assert_eq!(on_disk.entries.len(), 1);
        assert_eq!(on_disk.entries.get(&folder_key(&b)), Some(&b_before));
        assert!(!is_suppressed(&dir, &a));
        assert!(is_suppressed(&dir, &b));
    }

    #[test]
    fn drop_children_removes_entries_of_one_common_dir_only() {
        let _guards = identity("gc");
        let dir = scratch("gc");
        let common_a = folder(&dir, "a.git");
        let common_b = folder(&dir, "b.git");
        let a1 = folder(&dir, "a1");
        let a2 = folder(&dir, "a2");
        let b1 = folder(&dir, "b1");
        suppress(&dir, &a1, &common_a, 1).unwrap();
        suppress(&dir, &a2, &common_a, 2).unwrap();
        suppress(&dir, &b1, &common_b, 3).unwrap();
        let b1_before = read(&dir).entries[&folder_key(&b1)].clone();

        assert_eq!(drop_children(&dir, &common_a).unwrap(), 2);

        let on_disk = read(&dir);
        assert_eq!(on_disk.schema_version, SCHEMA_VERSION);
        assert_eq!(on_disk.entries.len(), 1);
        assert_eq!(on_disk.entries.get(&folder_key(&b1)), Some(&b1_before));
        assert!(!is_suppressed(&dir, &a1));
        assert!(!is_suppressed(&dir, &a2));
        assert!(is_suppressed(&dir, &b1));

        // Nothing left under A: no write, bytes unchanged.
        let bytes_before = std::fs::read(file_path(&dir)).unwrap();
        assert_eq!(drop_children(&dir, &common_a).unwrap(), 0);
        assert_eq!(std::fs::read(file_path(&dir)).unwrap(), bytes_before);
    }

    #[test]
    fn corrupt_file_reads_as_empty_and_is_replaced_on_next_write() {
        let _guards = identity("corrupt");
        let dir = scratch("corrupt");
        let common = folder(&dir, "repo.git");
        let path = folder(&dir, "child");
        std::fs::write(file_path(&dir), b"{not json").unwrap();

        assert!(!is_suppressed(&dir, &path));
        assert!(!lift(&dir, &path).unwrap());
        assert_eq!(drop_children(&dir, &common).unwrap(), 0);

        suppress(&dir, &path, &common, 5).unwrap();
        let on_disk = read(&dir);
        assert_eq!(on_disk.schema_version, SCHEMA_VERSION);
        assert_eq!(on_disk.entries.len(), 1);
        assert_eq!(
            on_disk.entries[&folder_key(&path)],
            SuppressionEntry {
                repository_common_dir: folder_key(&common),
                suppressed_ms: 5,
                primary: false,
            }
        );
        assert!(is_suppressed(&dir, &path));
    }

    /// The file as untyped JSON, so a member's presence can be told apart
    /// from a member that deserialized to its default.
    fn read_raw(root: &Path) -> serde_json::Value {
        let text = std::fs::read_to_string(file_path(root)).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// The removed-primary fact lives and dies with the entry that carries
    /// it: answered per repository, untouched by a lift of any other path,
    /// ended by a lift of its own path or by dropping the repository's
    /// entries, and never written for an ordinary entry.
    #[test]
    fn primary_fact_follows_its_entry() {
        let _guards = identity("primary-fact");
        let dir = scratch("primary-fact");
        let common_a = folder(&dir, "a.git");
        let common_b = folder(&dir, "b.git");
        let a_primary = folder(&dir, "a-primary");
        let a_child = folder(&dir, "a-child");
        let b_primary = folder(&dir, "b-primary");
        let a_fact = Some(PathBuf::from(folder_key(&a_primary)));
        let b_fact = Some(PathBuf::from(folder_key(&b_primary)));

        assert_eq!(removed_primary(&dir, &common_a), None);
        suppress(&dir, &a_child, &common_a, 1).unwrap();
        assert_eq!(
            removed_primary(&dir, &common_a),
            None,
            "an ordinary entry carries no fact"
        );

        suppress_primary(&dir, &a_primary, &common_a, 2).unwrap();
        assert_eq!(removed_primary(&dir, &common_a), a_fact);
        assert_eq!(
            removed_primary(&dir, &common_b),
            None,
            "the fact answers for its own repository only"
        );
        suppress_primary(&dir, &b_primary, &common_b, 3).unwrap();
        assert_eq!(removed_primary(&dir, &common_a), a_fact);
        assert_eq!(removed_primary(&dir, &common_b), b_fact);
        assert!(is_suppressed(&dir, &a_primary) && is_suppressed(&dir, &b_primary));

        // On disk: only the flagged entries carry the member.
        let raw = read_raw(&dir);
        let entries = raw["entries"].as_object().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[&folder_key(&a_child)].get("primary"),
            None,
            "suppress writes no primary member"
        );
        assert_eq!(
            entries[&folder_key(&a_primary)].get("primary"),
            Some(&serde_json::Value::Bool(true))
        );
        assert_eq!(
            entries[&folder_key(&b_primary)].get("primary"),
            Some(&serde_json::Value::Bool(true))
        );

        // A lift of another path of the same repository leaves the fact.
        assert!(lift(&dir, &a_child).unwrap());
        assert_eq!(removed_primary(&dir, &common_a), a_fact);

        // A lift of the flagged path ends it, for that repository only.
        assert!(lift(&dir, &a_primary).unwrap());
        assert_eq!(removed_primary(&dir, &common_a), None);
        assert_eq!(removed_primary(&dir, &common_b), b_fact);

        // Dropping the repository's entries ends it too.
        assert_eq!(drop_children(&dir, &common_b).unwrap(), 1);
        assert_eq!(removed_primary(&dir, &common_b), None);
        assert!(read(&dir).entries.is_empty());
    }

    /// T-22-18: a file in the format written before the flag existed, with
    /// and without `suppressed_ms`, loads through every public function,
    /// still suppresses its paths, carries no removed-primary fact, and
    /// keeps its entries as they were when a flagged entry is written later.
    #[test]
    fn entries_without_the_primary_member_load() {
        let _guards = identity("previous-format");
        let dir = scratch("previous-format");
        let common = folder(&dir, "repo.git");
        let timed = folder(&dir, "timed");
        let untimed = folder(&dir, "untimed");
        let later = folder(&dir, "later");
        let quoted = |text: &str| serde_json::to_string(text).unwrap();
        let literal = format!(
            r#"{{
  "schema_version": 1,
  "entries": {{
    {timed}: {{
      "repository_common_dir": {common},
      "suppressed_ms": 7
    }},
    {untimed}: {{
      "repository_common_dir": {common}
    }}
  }}
}}
"#,
            timed = quoted(&folder_key(&timed)),
            untimed = quoted(&folder_key(&untimed)),
            common = quoted(&folder_key(&common)),
        );
        assert!(
            !literal.contains("primary"),
            "the literal predates the flag"
        );
        std::fs::write(file_path(&dir), literal).unwrap();

        assert!(is_suppressed(&dir, &timed));
        assert!(is_suppressed(&dir, &untimed));
        assert_eq!(removed_primary(&dir, &common), None);
        let timed_entry = SuppressionEntry {
            repository_common_dir: folder_key(&common),
            suppressed_ms: 7,
            primary: false,
        };
        let untimed_entry = SuppressionEntry {
            repository_common_dir: folder_key(&common),
            suppressed_ms: 0,
            primary: false,
        };
        let loaded = read(&dir);
        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[&folder_key(&timed)], timed_entry);
        assert_eq!(loaded.entries[&folder_key(&untimed)], untimed_entry);

        suppress_primary(&dir, &later, &common, 9).unwrap();

        let rewritten = read(&dir);
        assert_eq!(rewritten.entries.len(), 3);
        assert_eq!(rewritten.entries[&folder_key(&timed)], timed_entry);
        assert_eq!(rewritten.entries[&folder_key(&untimed)], untimed_entry);
        let raw = read_raw(&dir);
        let entries = raw["entries"].as_object().unwrap();
        assert_eq!(entries[&folder_key(&timed)].get("primary"), None);
        assert_eq!(entries[&folder_key(&untimed)].get("primary"), None);
        assert_eq!(
            entries[&folder_key(&timed)].get("suppressed_ms"),
            Some(&serde_json::json!(7))
        );
        assert!(is_suppressed(&dir, &timed) && is_suppressed(&dir, &untimed));
        assert_eq!(
            removed_primary(&dir, &common),
            Some(PathBuf::from(folder_key(&later))),
            "only the entry written with the flag answers"
        );
    }

    /// The worktree path `git worktree list --porcelain`, run in `checkout`,
    /// prints for `checkout` itself. Exactly one listed worktree names it.
    fn porcelain_path(checkout: &Path) -> PathBuf {
        let listed = crate::testing::git_in(checkout, &["worktree", "list", "--porcelain"]);
        let matching: Vec<PathBuf> = listed
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .map(PathBuf::from)
            .filter(|path| folder_key(path) == folder_key(checkout))
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "git lists {} exactly once: {listed}",
            checkout.display()
        );
        matching[0].clone()
    }

    /// T-22.1-22: the lift key of every row kind, and of a subfolder of
    /// each, is the worktree git lists for that checkout: never the main
    /// worktree for a linked one, never a parent. A folder git does not know
    /// keys on its own canonical path.
    #[test]
    fn lift_key_names_the_checkout_root_for_every_row_kind() {
        let guards = identity("lift-key");
        let root: &Path = &guards.2;

        let topology = crate::testing::cloned_topology(root);
        let init = folder(root, "init");
        crate::testing::git_in(&init, &["init", "-q", "-b", "main"]);
        crate::testing::set_git_identity(&init, "Test", "test@example.com");
        std::fs::write(init.join("tracked"), b"one\n").unwrap();
        crate::testing::git_in(&init, &["add", "tracked"]);
        crate::testing::git_in(&init, &["commit", "-q", "-m", "initial"]);
        let by_hand = root.join("by-hand");
        crate::testing::git_in(
            &init,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "by-hand",
                by_hand.to_str().unwrap(),
            ],
        );

        let kinds: [(&str, &Path); 5] = [
            ("clone main checkout", &topology.clone),
            ("managed default worktree", &topology.default_worktree),
            ("managed branch worktree", &topology.managed_worktree),
            ("git-init main checkout", &init),
            ("hand-made linked worktree", &by_hand),
        ];
        for (kind, checkout) in kinds {
            let listed = porcelain_path(checkout);
            assert_eq!(
                folder_key(&lift_key(checkout)),
                folder_key(&listed),
                "{kind} {}",
                checkout.display()
            );
            let sub = folder(checkout, "sub");
            assert_eq!(
                folder_key(&lift_key(&sub)),
                folder_key(&listed),
                "subfolder {} of the {kind}",
                sub.display()
            );
        }

        let plain = folder(root, "plain");
        assert_eq!(lift_key(&plain), plain.canonicalize().unwrap());
    }

    #[test]
    fn file_is_named_per_workspace_beside_breadcrumbs() {
        let _guards = identity("naming");
        let dir = scratch("naming");
        let path = file_path(&dir);
        assert!(path.starts_with(&dir));
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("json"));
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        assert!(name.contains(FILE_BASE), "got {name}");
        assert!(name.contains(WORKSPACE), "got {name}");
        let workspace = crate::workspace::active();
        assert_eq!(name, workspace.state_file(FILE_BASE));
        assert_ne!(name, workspace.state_file(crate::breadcrumbs::FILE_BASE));
        assert_eq!(path.parent(), Some(dir.path()));
    }
}
