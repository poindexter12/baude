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
    fn scratch(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("baude-suppression-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// Redirect every fixture path and pin a workspace identity so
    /// `workspace::active()` resolves under test.
    fn identity(name: &str) -> (TestRedirect, TestRedirect) {
        let redirect = TestRedirect::new(scratch(&format!("{name}-root")));
        let config = Config {
            workspace: Some(WORKSPACE.to_string()),
            ..Config::default()
        };
        let ws = crate::workspace::override_for_test(&config, None);
        (redirect, ws)
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
        assert_eq!(path.parent(), Some(dir.as_path()));
    }
}
