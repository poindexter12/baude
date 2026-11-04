//! Atomic marker files for managed repository ownership and identity recovery.
//!
//! A marker file records the canonical git common directory and timestamp
//! when a managed repository directory is first created or adopted.
//! Atomicity is enforced by create_new(true) — the first writer succeeds,
//! subsequent writers see AlreadyExists and read the winner's marker.
//!
//! Markers are fail-closed: symlinks, oversized files, and malformed JSON
//! are never returned as Err; instead, they return Ok(MarkerRead::Invalid).
//! Only I/O errors other than NotFound are propagated.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Metadata recorded in a marker file.
///
/// The marker captures the canonical common directory (in non-UTF-8 safe bytes)
/// and the scheme version for future migrations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MarkerMetadata {
    /// Base64-encoded canonical common directory bytes.
    /// Serialized as a string in JSON, deserialized to Vec<u8>.
    pub canonical_common_dir: Vec<u8>,
    /// Schema version for future format migrations.
    pub scheme_version: u32,
    /// Unix milliseconds when the marker was recorded.
    pub recorded_at_ms: u64,
}

/// Result of a marker write operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkerWrite {
    /// Marker already exists with matching canonical_common_dir; no file was created.
    AlreadyOwned(MarkerMetadata),
    /// Marker was successfully created.
    Created,
}

/// Reason a marker read returned Invalid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvalidReason {
    /// Marker path is a symlink (rejected via O_NOFOLLOW).
    Symlink,
    /// File exceeds 64 KiB.
    Oversized(u64),
    /// JSON deserialization failed.
    Malformed(String),
    /// Marker scheme version is newer than this binary supports.
    SchemeTooNew(u32),
}

/// Result of a marker read operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkerRead {
    /// Marker file does not exist.
    Missing,
    /// Marker file exists and is valid.
    Valid(MarkerMetadata),
    /// Marker file exists but is invalid (symlink, oversized, malformed, etc.).
    Invalid(InvalidReason),
}

/// Error returned from marker write operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkerError {
    /// Marker exists and belongs to a different repository.
    OwnedByOther(MarkerMetadata),
    /// Marker exists but is invalid (ownership unknown).
    UnknownOwner(InvalidReason),
}

/// Write a marker file into an existing directory.
///
/// `dir` must already exist. The marker file (`.baude-marker.json`) is written
/// atomically using create_new(true), which is O_EXCL on Unix and guarantees
/// exactly one writer succeeds. On AlreadyExists, this function reads the existing
/// marker and compares ownership.
///
/// # Returns
///
/// - `Ok(MarkerWrite::Created)` — marker was created successfully
/// - `Ok(MarkerWrite::AlreadyOwned(existing))` — marker exists with matching canonical_common_dir
/// - `Err(MarkerError::OwnedByOther(existing))` — marker exists with different canonical_common_dir
/// - `Err(MarkerError::UnknownOwner(reason))` — marker exists but is invalid
pub fn write_marker(dir: &Path, meta: &MarkerMetadata) -> Result<MarkerWrite, MarkerError> {
    let marker_path = dir.join(".baude-marker.json");

    match File::create_new(&marker_path) {
        Ok(mut file) => {
            // Serialize to JSON and write
            let json = serde_json::to_string(meta)
                .map_err(|e| MarkerError::UnknownOwner(InvalidReason::Malformed(e.to_string())))?;
            file.write_all(json.as_bytes())
                .map_err(|e| MarkerError::UnknownOwner(InvalidReason::Malformed(e.to_string())))?;

            // Sync file
            file.sync_all()
                .map_err(|e| MarkerError::UnknownOwner(InvalidReason::Malformed(e.to_string())))?;

            // Sync parent directory on Unix to ensure durability
            #[cfg(unix)]
            {
                let parent = dir;
                if let Ok(parent_file) = std::fs::File::open(parent) {
                    let _ = parent_file.sync_all();
                }
            }

            Ok(MarkerWrite::Created)
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            // Marker already exists; read it and compare ownership.
            //
            // `create_new` is atomic, but creating the file and writing its
            // contents are two steps. A racing process that loses the create
            // can therefore observe a ZERO-LENGTH marker the winner has not
            // filled in yet, read it as invalid, and conclude "unknown owner"
            // — which makes the caller allocate a suffixed directory for a
            // repository baude already owns. Linux CI caught this; macOS
            // timing hid it. Give the winner a bounded moment to finish
            // before believing an unreadable marker.
            match read_marker_settled(dir) {
                Ok(MarkerRead::Valid(existing)) => {
                    if existing.canonical_common_dir == meta.canonical_common_dir {
                        Ok(MarkerWrite::AlreadyOwned(existing))
                    } else {
                        Err(MarkerError::OwnedByOther(existing))
                    }
                }
                Ok(MarkerRead::Invalid(reason)) => Err(MarkerError::UnknownOwner(reason)),
                Ok(MarkerRead::Missing) => {
                    // File was deleted between create_new and read; retry once
                    // For now, treat as unknown owner
                    Err(MarkerError::UnknownOwner(InvalidReason::Malformed(
                        "Marker disappeared between create and read".into(),
                    )))
                }
                Err(io_err) => Err(MarkerError::UnknownOwner(InvalidReason::Malformed(
                    format!("Failed to read existing marker: {}", io_err),
                ))),
            }
        }
        Err(e) => Err(MarkerError::UnknownOwner(InvalidReason::Malformed(
            format!("Failed to create marker: {}", e),
        ))),
    }
}

/// [`read_marker`], retried briefly while the marker looks half-written.
///
/// Only the transient shapes are retried — a missing file, or one that does
/// not parse yet. A marker that is still unreadable after the window is
/// returned as-is, so a genuinely corrupt marker is reported exactly as
/// before, just ~100 ms later.
fn read_marker_settled(dir: &Path) -> io::Result<MarkerRead> {
    const ATTEMPTS: u32 = 10;
    const PAUSE: std::time::Duration = std::time::Duration::from_millis(10);
    let mut last = read_marker(dir);
    for _ in 1..ATTEMPTS {
        match &last {
            Ok(MarkerRead::Valid(_)) => return last,
            Ok(MarkerRead::Missing) | Ok(MarkerRead::Invalid(_)) | Err(_) => {}
        }
        std::thread::sleep(PAUSE);
        last = read_marker(dir);
    }
    last
}

/// Read a marker file from a directory.
///
/// Reads the marker file and rejects symlinks via is_symlink() check.
/// Reads up to 64 KiB. Malformed, oversized, or symlinked markers are
/// returned as `Ok(MarkerRead::Invalid(...))`, never as `Err`.
///
/// # Returns
///
/// - `Ok(MarkerRead::Missing)` — marker file does not exist
/// - `Ok(MarkerRead::Valid(metadata))` — marker was read and deserialized successfully
/// - `Ok(MarkerRead::Invalid(reason))` — marker exists but is symlink, oversized, malformed, or too-new version
/// - `Err(io::Error)` — I/O error other than NotFound
pub fn read_marker(dir: &Path) -> Result<MarkerRead, io::Error> {
    let marker_path = dir.join(".baude-marker.json");

    // Use symlink_metadata to detect symlinks (it doesn't follow them)
    match std::fs::symlink_metadata(&marker_path) {
        Ok(metadata) => {
            // Reject symlinks
            if metadata.is_symlink() {
                return Ok(MarkerRead::Invalid(InvalidReason::Symlink));
            }

            // Check size (max 64 KiB)
            const MAX_MARKER_SIZE: u64 = 64 * 1024;
            if metadata.len() > MAX_MARKER_SIZE {
                return Ok(MarkerRead::Invalid(InvalidReason::Oversized(
                    metadata.len(),
                )));
            }

            // Now open and read the file (which is known to be non-symlink)
            let mut file = std::fs::File::open(&marker_path)?;
            let mut content = Vec::new();
            file.read_to_end(&mut content)?;

            match serde_json::from_slice::<MarkerMetadata>(&content) {
                Ok(meta) => {
                    // Check scheme version
                    const CURRENT_SCHEME_VERSION: u32 = 1;
                    if meta.scheme_version > CURRENT_SCHEME_VERSION {
                        Ok(MarkerRead::Invalid(InvalidReason::SchemeTooNew(
                            meta.scheme_version,
                        )))
                    } else {
                        Ok(MarkerRead::Valid(meta))
                    }
                }
                Err(e) => Ok(MarkerRead::Invalid(InvalidReason::Malformed(e.to_string()))),
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(MarkerRead::Missing),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[allow(dead_code)]
    fn test_dir(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("baude-marker-test-{}-{}", name, n));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test dir");
        path
    }

    #[test]
    fn digest_is_deterministic() {
        // Test that compute_repository_digest produces the same output for the same input
        let test_bytes = b"canonical/common/dir";
        let digest1 = crate::repository::compute_repository_digest(test_bytes);
        let digest2 = crate::repository::compute_repository_digest(test_bytes);
        assert_eq!(digest1, digest2, "Same input should produce same digest");
        assert_eq!(
            digest1.len(),
            12,
            "Digest should be exactly 12 hex characters"
        );
    }

    #[test]
    fn marker_roundtrip() {
        let test_dir = test_dir("roundtrip");

        let meta = MarkerMetadata {
            canonical_common_dir: vec![0xFF, 0xFE, 0xFD, 0x00, 0x01],
            scheme_version: 1,
            recorded_at_ms: 1234567890,
        };

        // Write marker
        let result = write_marker(&test_dir, &meta);
        assert!(
            matches!(result, Ok(MarkerWrite::Created)),
            "First write should create marker"
        );

        // Read marker back
        let read_result = read_marker(&test_dir);
        assert!(
            matches!(read_result, Ok(MarkerRead::Valid(_))),
            "Should read valid marker"
        );

        if let Ok(MarkerRead::Valid(read_meta)) = read_result {
            assert_eq!(
                read_meta.canonical_common_dir, meta.canonical_common_dir,
                "Bytes should survive roundtrip"
            );
            assert_eq!(read_meta.scheme_version, meta.scheme_version);
            assert_eq!(read_meta.recorded_at_ms, meta.recorded_at_ms);
        }
    }

    #[test]
    fn write_marker_first_writer_wins_second_sees_already_owned() {
        let test_dir = test_dir("first_wins");

        let meta = MarkerMetadata {
            canonical_common_dir: b"test/common".to_vec(),
            scheme_version: 1,
            recorded_at_ms: 1000,
        };

        let test_dir_path = test_dir.clone();
        let barrier = Arc::new(Barrier::new(2));

        let thread1_barrier = barrier.clone();
        let thread1_meta = meta.clone();
        let thread1_path = test_dir_path.clone();
        let thread1 = thread::spawn(move || {
            thread1_barrier.wait();
            write_marker(&thread1_path, &thread1_meta)
        });

        let thread2_barrier = barrier.clone();
        let thread2_meta = meta.clone();
        let thread2_path = test_dir_path.clone();
        let thread2 = thread::spawn(move || {
            thread2_barrier.wait();
            write_marker(&thread2_path, &thread2_meta)
        });

        let result1 = thread1.join().unwrap();
        let result2 = thread2.join().unwrap();

        // One should create, one should see already owned
        let created_count = [&result1, &result2]
            .iter()
            .filter(|r| matches!(r, Ok(MarkerWrite::Created)))
            .count();

        let already_owned_count = [&result1, &result2]
            .iter()
            .filter(|r| matches!(r, Ok(MarkerWrite::AlreadyOwned(_))))
            .count();

        assert_eq!(created_count, 1, "Exactly one thread should create marker");
        assert_eq!(
            already_owned_count, 1,
            "Exactly one thread should see already owned"
        );
    }

    #[test]
    fn write_marker_refuses_foreign_marker() {
        let test_dir = test_dir("foreign");

        let meta1 = MarkerMetadata {
            canonical_common_dir: b"repo1/common".to_vec(),
            scheme_version: 1,
            recorded_at_ms: 1000,
        };

        // Write marker for repo1
        let result1 = write_marker(&test_dir, &meta1);
        assert!(matches!(result1, Ok(MarkerWrite::Created)));

        // Try to write marker for repo2 with different common dir
        let meta2 = MarkerMetadata {
            canonical_common_dir: b"repo2/common".to_vec(),
            scheme_version: 1,
            recorded_at_ms: 2000,
        };

        let result2 = write_marker(&test_dir, &meta2);
        assert!(
            matches!(result2, Err(MarkerError::OwnedByOther(_))),
            "Should reject foreign marker"
        );
    }

    #[test]
    fn concurrent_marker_writes_exactly_one_creates() {
        let test_dir = test_dir("concurrent");

        let meta = MarkerMetadata {
            canonical_common_dir: b"test/common".to_vec(),
            scheme_version: 1,
            recorded_at_ms: 1000,
        };

        let test_dir_path = test_dir.clone();
        let barrier = Arc::new(Barrier::new(3));

        let mut threads = vec![];
        for _ in 0..3 {
            let barrier_clone = barrier.clone();
            let meta_clone = meta.clone();
            let path_clone = test_dir_path.clone();

            let thread = thread::spawn(move || {
                barrier_clone.wait();
                write_marker(&path_clone, &meta_clone)
            });
            threads.push(thread);
        }

        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();

        let created_count = results
            .iter()
            .filter(|r| matches!(r, Ok(MarkerWrite::Created)))
            .count();

        assert_eq!(
            created_count, 1,
            "Exactly one thread should successfully create marker"
        );
    }

    #[test]
    fn non_utf8_paths_in_marker() {
        let test_dir = test_dir("non_utf8");

        // Non-UTF-8 bytes
        let non_utf8_bytes = vec![0xFF, 0xFE, 0xFD, 0x00, 0x01, 0x80, 0x81];
        let meta = MarkerMetadata {
            canonical_common_dir: non_utf8_bytes.clone(),
            scheme_version: 1,
            recorded_at_ms: 1234567890,
        };

        write_marker(&test_dir, &meta).unwrap();

        let read_result = read_marker(&test_dir).unwrap();
        if let MarkerRead::Valid(read_meta) = read_result {
            assert_eq!(
                read_meta.canonical_common_dir, non_utf8_bytes,
                "Non-UTF-8 bytes should survive"
            );
        } else {
            panic!("Expected valid marker");
        }
    }

    #[test]
    fn read_marker_symlink_is_invalid() {
        #[cfg(unix)]
        {
            let test_dir = test_dir("symlink");

            // Create a symlink at marker path
            let marker_path = test_dir.join(".baude-marker.json");
            let target = test_dir.join("target.json");
            fs::write(&target, "{}").unwrap();
            std::os::unix::fs::symlink(&target, &marker_path).unwrap();

            let result = read_marker(&test_dir).unwrap();
            assert!(
                matches!(result, MarkerRead::Invalid(InvalidReason::Symlink)),
                "Symlink should be invalid"
            );
        }

        #[cfg(not(unix))]
        {
            // Symlink test skipped on non-Unix
        }
    }

    #[test]
    fn read_marker_oversized_is_invalid() {
        let test_dir = test_dir("oversized");

        let marker_path = test_dir.join(".baude-marker.json");
        let large_content = vec![0u8; 65 * 1024]; // 65 KiB
        fs::write(&marker_path, large_content).unwrap();

        let result = read_marker(&test_dir).unwrap();
        assert!(
            matches!(result, MarkerRead::Invalid(InvalidReason::Oversized(_))),
            "Oversized marker should be invalid"
        );
    }

    #[test]
    fn read_marker_malformed_is_invalid() {
        let test_dir = test_dir("malformed");

        let marker_path = test_dir.join(".baude-marker.json");
        fs::write(&marker_path, "not valid json {{{").unwrap();

        let result = read_marker(&test_dir).unwrap();
        assert!(
            matches!(result, MarkerRead::Invalid(InvalidReason::Malformed(_))),
            "Malformed JSON should be invalid"
        );
    }

    fn owner_meta() -> MarkerMetadata {
        MarkerMetadata {
            canonical_common_dir: b"/repo/a/.git".to_vec(),
            scheme_version: 1,
            recorded_at_ms: 1,
        }
    }

    #[test]
    fn clear_owned_removes_owned_marker_and_empty_directory() {
        // Behaviour 1: an owned marker in an otherwise empty directory.
        let owned = test_dir("clear_owned");
        let meta = owner_meta();
        assert_eq!(write_marker(&owned, &meta), Ok(MarkerWrite::Created));

        let outcome = clear_owned(&owned, &meta.canonical_common_dir).unwrap();
        assert_eq!(
            outcome,
            MarkerClear {
                marker: MarkerOutcome::Removed,
                directory: DirectoryOutcome::Removed,
            }
        );
        assert_eq!(read_marker(&owned).unwrap(), MarkerRead::Missing);
        assert!(!owned.exists(), "directory should be gone after the clear");

        // Behaviour 7: no marker but an empty directory.
        let empty = test_dir("clear_empty");
        let outcome = clear_owned(&empty, &meta.canonical_common_dir).unwrap();
        assert_eq!(
            outcome,
            MarkerClear {
                marker: MarkerOutcome::Missing,
                directory: DirectoryOutcome::Removed,
            }
        );
        assert!(!empty.exists(), "empty directory should be removed");
    }

    #[test]
    fn clear_owned_refuses_foreign_marker_and_leaves_directory() {
        let test_dir = test_dir("clear_foreign");
        let foreign = MarkerMetadata {
            canonical_common_dir: b"/repo/b/.git".to_vec(),
            scheme_version: 1,
            recorded_at_ms: 2,
        };
        assert_eq!(write_marker(&test_dir, &foreign), Ok(MarkerWrite::Created));
        let marker_path = test_dir.join(".baude-marker.json");
        let before = fs::read(&marker_path).unwrap();

        let outcome = clear_owned(&test_dir, b"/repo/a/.git").unwrap();
        assert_eq!(
            outcome,
            MarkerClear {
                marker: MarkerOutcome::Refused(ClearRefusal::ForeignOwner),
                directory: DirectoryOutcome::Untouched,
            }
        );
        assert_eq!(
            fs::read(&marker_path).unwrap(),
            before,
            "foreign marker bytes must be unchanged"
        );
        assert_eq!(read_marker(&test_dir).unwrap(), MarkerRead::Valid(foreign));
        assert!(test_dir.is_dir(), "directory must survive a refusal");
    }

    #[test]
    fn clear_owned_refuses_invalid_and_symlinked_marker() {
        let owner = owner_meta();

        // Malformed marker content.
        let malformed_dir = test_dir("clear_malformed");
        let malformed_path = malformed_dir.join(".baude-marker.json");
        fs::write(&malformed_path, "not valid json {{{").unwrap();
        let before = fs::read(&malformed_path).unwrap();

        let outcome = clear_owned(&malformed_dir, &owner.canonical_common_dir).unwrap();
        assert!(
            matches!(
                outcome.marker,
                MarkerOutcome::Refused(ClearRefusal::Invalid(InvalidReason::Malformed(_)))
            ),
            "malformed marker should be refused as Invalid: {:?}",
            outcome
        );
        assert_eq!(outcome.directory, DirectoryOutcome::Untouched);
        assert_eq!(fs::read(&malformed_path).unwrap(), before);
        assert!(malformed_dir.is_dir());

        // Symlinked marker file.
        #[cfg(unix)]
        {
            let link_dir = test_dir("clear_marker_symlink");
            let marker_path = link_dir.join(".baude-marker.json");
            let target = link_dir.join("target.json");
            fs::write(&target, serde_json::to_string(&owner).unwrap()).unwrap();
            std::os::unix::fs::symlink(&target, &marker_path).unwrap();

            let outcome = clear_owned(&link_dir, &owner.canonical_common_dir).unwrap();
            assert_eq!(
                outcome,
                MarkerClear {
                    marker: MarkerOutcome::Refused(ClearRefusal::Invalid(InvalidReason::Symlink)),
                    directory: DirectoryOutcome::Untouched,
                }
            );
            assert!(target.exists(), "symlink target file must survive");
            assert!(
                fs::symlink_metadata(&marker_path).unwrap().is_symlink(),
                "symlinked marker must not be removed"
            );
            assert!(link_dir.is_dir());
        }
    }

    #[test]
    fn clear_owned_missing_directory_is_a_noop() {
        let parent = test_dir("clear_missing");
        // The repository- path a legacy empty physical_key resolves to.
        let missing = parent.join("repository-");
        assert!(!missing.exists());

        let outcome = clear_owned(&missing, b"/repo/a/.git").unwrap();
        assert_eq!(
            outcome,
            MarkerClear {
                marker: MarkerOutcome::Missing,
                directory: DirectoryOutcome::Missing,
            }
        );
        assert!(!missing.exists());
        assert!(parent.is_dir(), "the parent is not touched");
    }

    #[test]
    fn clear_owned_keeps_non_empty_directory_after_marker_removal() {
        let test_dir = test_dir("clear_non_empty");
        let meta = owner_meta();
        assert_eq!(write_marker(&test_dir, &meta), Ok(MarkerWrite::Created));
        let child = test_dir.join("leftover-worktree");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("file.txt"), "keep me").unwrap();

        let outcome = clear_owned(&test_dir, &meta.canonical_common_dir).unwrap();
        assert_eq!(
            outcome,
            MarkerClear {
                marker: MarkerOutcome::Removed,
                directory: DirectoryOutcome::NotEmpty,
            }
        );
        assert_eq!(read_marker(&test_dir).unwrap(), MarkerRead::Missing);
        assert!(child.is_dir(), "subdirectory must survive");
        assert_eq!(
            fs::read_to_string(child.join("file.txt")).unwrap(),
            "keep me"
        );
        assert!(test_dir.is_dir());
    }

    #[test]
    fn clear_owned_refuses_symlinked_directory() {
        #[cfg(unix)]
        {
            let parent = test_dir("clear_dir_symlink");
            let target_dir = parent.join("real");
            fs::create_dir(&target_dir).unwrap();
            let meta = owner_meta();
            assert_eq!(write_marker(&target_dir, &meta), Ok(MarkerWrite::Created));
            let link = parent.join("repository-link");
            std::os::unix::fs::symlink(&target_dir, &link).unwrap();
            let before = fs::read(target_dir.join(".baude-marker.json")).unwrap();

            let outcome = clear_owned(&link, &meta.canonical_common_dir).unwrap();
            assert_eq!(
                outcome,
                MarkerClear {
                    marker: MarkerOutcome::Refused(ClearRefusal::DirectoryIsSymlink),
                    directory: DirectoryOutcome::Untouched,
                }
            );
            assert!(
                fs::symlink_metadata(&link).unwrap().is_symlink(),
                "the symlink itself must survive"
            );
            assert!(target_dir.is_dir(), "link target directory must survive");
            assert_eq!(
                fs::read(target_dir.join(".baude-marker.json")).unwrap(),
                before,
                "link target marker must be unchanged"
            );
            assert_eq!(read_marker(&target_dir).unwrap(), MarkerRead::Valid(meta));
        }

        #[cfg(not(unix))]
        {
            // Symlink test skipped on non-Unix
        }
    }
}
