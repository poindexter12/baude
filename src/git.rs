use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Result};

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(anyhow!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub fn repo_root(path: &Path) -> Option<PathBuf> {
    git(path, &["rev-parse", "--show-toplevel"])
        .ok()
        .map(PathBuf::from)
}

fn worktrees_base() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("baude")
        .join("worktrees")
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Create a worktree for `branch` under the managed directory.
/// Creates the branch if it doesn't exist; otherwise checks out the existing one.
pub fn create_worktree(repo: &Path, branch: &str) -> Result<PathBuf> {
    let repo_name = repo
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());
    let dir = worktrees_base()
        .join(sanitize(&repo_name))
        .join(sanitize(branch));
    if dir.exists() {
        // Reuse an existing managed worktree for this branch.
        return Ok(dir);
    }
    std::fs::create_dir_all(dir.parent().unwrap())?;
    let dir_str = dir.to_string_lossy().to_string();

    let new_branch = git(repo, &["worktree", "add", &dir_str, "-b", branch]);
    if new_branch.is_ok() {
        return Ok(dir);
    }
    // Branch may already exist — try checking it out instead.
    git(repo, &["worktree", "add", &dir_str, branch]).map_err(|e| anyhow!("{e}"))?;
    Ok(dir)
}

pub fn is_dirty(worktree: &Path) -> bool {
    git(worktree, &["status", "--porcelain"])
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

pub fn remove_worktree(repo: &Path, worktree: &Path) -> Result<()> {
    git(repo, &["worktree", "remove", &worktree.to_string_lossy()])?;
    Ok(())
}
