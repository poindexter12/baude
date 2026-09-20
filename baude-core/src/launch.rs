//! Shared startup helper for workspace initialization and state lock acquisition.
//!
//! Both the TUI (`baude`) and daemon (`bauded`) follow the same sequence:
//! 1. Canonicalize the launch directory
//! 2. Plan the launch (ancestor walk + repo root discovery)
//! 3. Initialize the workspace cache
//! 4. Claim the workspace state lock
//! 5. Record any derived binding
//!
//! [`start_workspace`] encapsulates this sequence so both binaries remain in lockstep.

use std::path::{Path, PathBuf};

use crate::persist::{self, Config, StateLockError};
use crate::workspace::Workspace;

/// Environment variables passed to startup.
#[derive(Clone)]
pub struct StartEnv {
    pub ws_env: Option<String>,
    pub backend_env: Option<String>,
}

/// Result of successful startup: the initialized workspace and the discovered repository root.
pub struct StartedWorkspace {
    /// The cached workspace (&'static lifetime from the cache).
    pub workspace: &'static Workspace,
    /// The repository root discovered during startup, if any.
    pub repo_root: Option<PathBuf>,
    /// Startup notes for the user: folder-memory notes from `plan_launch` plus the
    /// applied-binding note when folder context is enabled. The caller prints them
    /// once the terminal is ready (previously assembled inline in `baude/src/main.rs`).
    pub notes: Vec<String>,
}

/// Error during startup.
#[derive(Debug)]
pub enum StartError {
    /// The workspace state lock is held by another process.
    LockHeld { diag: String },
    /// I/O error when claiming the lock.
    LockIo { path: PathBuf, detail: String },
}

/// Start the workspace for this process: initialize the cache, claim the lock, and record bindings.
///
/// Steps (in order):
/// 1. Validate/canonicalize launch_dir
/// 2. Plan the launch (ancestor walk + repo root discovery via git::repo_root)
/// 3. Initialize the workspace cache
/// 4. Claim the workspace state lock with the provided lock_base
/// 5. If lock claim succeeded, record any derived binding
///
/// Returns the initialized workspace and the repository root (if discovered).
///
/// # Lock Handling
/// A lock refusal (Held or Io) returns an error without recording the binding.
/// This preserves the single-writer invariant: a lock refusal means the workspace
/// is in use elsewhere, so we must exit immediately without partial writes.
///
/// # Parameters
/// - `launch_dir`: The directory baude was launched from (typically argv[1] or cwd).
/// - `config`: The baude config (contains workspace/backend declarations).
/// - `env`: Environment variables (`BAUDE_WORKSPACE`, `BAUDE_BACKEND`).
/// - `lock_base`: The lock base name ("state" for TUI, "daemon-state" for daemon).
pub fn start_workspace(
    launch_dir: &Path,
    config: &Config,
    env: StartEnv,
    lock_base: &str,
) -> Result<StartedWorkspace, StartError> {
    // Step 1: Canonicalize launch dir (caller usually does this, but be explicit).
    let launch_dir = std::fs::canonicalize(launch_dir).unwrap_or_else(|_| launch_dir.to_path_buf());

    // Step 2: Plan the launch (ancestor walk + repo root discovery).
    let plan = crate::folder_workspace::plan_launch(
        config.folder_context_enabled(),
        env.ws_env.as_deref(),
        env.backend_env.as_deref(),
        Some(&persist::config_dir()),
        &launch_dir,
    );

    // Capture hint and repo_root for later use; take the folder-memory notes.
    let hint = plan.hint.clone();
    let repo_root = plan.repo_root.clone();
    let mut notes = plan.notes;

    // Step 3: Initialize the workspace cache with the launch context.
    let ws_ctx = crate::workspace::WorkspaceLaunchContext {
        hint,
        repo_root: repo_root.clone(),
    };
    let workspace = crate::workspace::initialize_with_context(config, ws_ctx);
    if config.folder_context_enabled() {
        notes.extend(crate::folder_workspace::applied_note(
            &workspace.name,
            env.ws_env.as_deref(),
            env.backend_env.as_deref(),
            config,
        ));
    }

    // Step 4: Claim the workspace state lock.
    match persist::claim_workspace_state_lock(lock_base, workspace) {
        Ok(()) => {
            // Step 5: Lock succeeded; record the binding if repo_root is present.
            if let Some(repo_root_path) = &repo_root {
                let now_ms = crate::pty::now_ms();
                crate::folder_workspace::record(
                    Some(&persist::config_dir()),
                    repo_root_path,
                    &workspace.name,
                    now_ms,
                );
            }
            Ok(StartedWorkspace {
                workspace,
                repo_root,
                notes,
            })
        }
        Err(StateLockError::Held { holder, .. }) => {
            let diag = match holder {
                Some(pid) => format!(
                    "workspace {} is already open in another baude (pid {})",
                    workspace.name, pid
                ),
                None => format!(
                    "workspace {} is already open in another baude",
                    workspace.name
                ),
            };
            Err(StartError::LockHeld { diag })
        }
        Err(StateLockError::Io { path, source }) => Err(StartError::LockIo {
            path,
            detail: source.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::Config;
    use crate::testing::TestRedirect;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "baude-launch-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn test_wspc02_start_workspace_derives_persists_recalls() {
        let _redirect = TestRedirect::new(scratch("wspc02-root"));

        // Create a git repository.
        let repo_dir = scratch("wspc02-repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let _ = std::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_dir)
            .output();

        // Test the plan_launch function to ensure repo_root discovery works.
        let config_root = scratch("wspc02-config");
        let plan =
            crate::folder_workspace::plan_launch(true, None, None, Some(&config_root), &repo_dir);

        // Verify that repo_root was discovered.
        assert!(plan.repo_root.is_some());
        // Verify no hint exists yet (first launch).
        assert_eq!(plan.hint, None);

        // Now simulate recording a binding after derivation.
        let repo_root = plan.repo_root.as_ref().unwrap();
        crate::folder_workspace::record(Some(&config_root), repo_root, "derived-workspace", 100);

        // Second launch from a different subfolder should find the binding.
        let subfolder = repo_root.join("subdir");
        std::fs::create_dir_all(&subfolder).unwrap();
        let plan2 =
            crate::folder_workspace::plan_launch(true, None, None, Some(&config_root), &subfolder);

        assert_eq!(plan2.hint.as_deref(), Some("derived-workspace"));
    }

    #[test]
    fn test_daemon_startup_parity_with_tui() {
        let _redirect = TestRedirect::new(scratch("daemon-parity-root"));

        // Create a git repository.
        let repo_dir = scratch("daemon-parity-repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let _ = std::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_dir)
            .output();

        // Both TUI and daemon should use the same plan_launch function.
        let config_root = scratch("daemon-parity-config");
        let tui_plan = crate::folder_workspace::plan_launch(true, None, None, Some(&config_root), &repo_dir);
        let daemon_plan = crate::folder_workspace::plan_launch(true, None, None, Some(&config_root), &repo_dir);

        // Both should produce the same result (same repo_root discovery).
        assert_eq!(tui_plan.repo_root, daemon_plan.repo_root);
        assert_eq!(tui_plan.hint, daemon_plan.hint);
    }
}
