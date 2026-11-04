//! Per-workspace re-admission suppression store.
//!
//! Remembers checkout paths that were removed while their repository row
//! survived, so the `git worktree list` sweep on the next admission of that
//! repository does not bring the row back. Advisory, per-workspace, additive
//! schema; a corrupt or absent file reads as empty and never fails a launch.
//! Entries are kept until lifted by an explicit admission of that exact path
//! or garbage-collected when the parent repository row is removed.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::breadcrumbs::folder_key;
    use crate::persist::Config;
    use crate::testing::TestRedirect;
    use std::path::{Path, PathBuf};

    const WORKSPACE: &str = "suppression-ws";

    /// Fresh scratch dir per test, following the repo's temp-dir convention.
    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "baude-suppression-{name}-{}",
            std::process::id()
        ));
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
