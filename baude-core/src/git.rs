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

/// A repo named for cloning: where it lives and the URL to clone with.
pub struct CloneTarget {
    pub host: String,
    pub owner: String,
    pub repo: String,
    /// Canonical URL handed to `git clone` — scp-style ssh unless the input
    /// was an `http(s)://` URL, which keeps https.
    pub url: String,
}

/// Parse user input naming a repo to clone. Accepts scp-style ssh
/// (`git@github.com:owner/repo.git`), `ssh://`/`http(s)://` URLs (browser
/// URLs with trailing segments like `/tree/main` are tolerated), a
/// scheme-less `host/owner/repo`, or the `owner/repo` shorthand (assumes
/// github.com). Everything except pasted https keeps ssh, so unattended
/// pushes ride the usual ssh auth.
pub fn parse_clone_target(input: &str) -> Option<CloneTarget> {
    let input = input.trim().split(['?', '#']).next()?.trim_end_matches('/');
    // Strip a trailing `.git` and reject empty/nested segments.
    let clean = |s: &str| {
        let s = s.strip_suffix(".git").unwrap_or(s);
        (!s.is_empty() && !s.contains('/')).then(|| s.to_string())
    };
    let ssh_url = |h: &str, o: &str, r: &str| format!("git@{h}:{o}/{r}.git");
    if let Some(rest) = input.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        let mut seg = path.split('/');
        let (owner, repo) = (clean(seg.next()?)?, clean(seg.next()?)?);
        return Some(CloneTarget {
            url: ssh_url(host, &owner, &repo),
            host: host.to_string(),
            owner,
            repo,
        });
    }
    if let Some((scheme, rest)) = input.split_once("://") {
        let rest = rest.split_once('@').map_or(rest, |(_, r)| r);
        let mut seg = rest.split('/');
        let host = seg.next()?.split(':').next()?.to_string();
        if host.is_empty() {
            return None;
        }
        let (owner, repo) = (clean(seg.next()?)?, clean(seg.next()?)?);
        let url = match scheme {
            "http" | "https" => format!("https://{host}/{owner}/{repo}.git"),
            _ => ssh_url(&host, &owner, &repo),
        };
        return Some(CloneTarget {
            host,
            owner,
            repo,
            url,
        });
    }
    let segs: Vec<&str> = input.split('/').collect();
    let (host, owner, repo) = match segs.as_slice() {
        [owner, repo] => ("github.com".to_string(), clean(owner)?, clean(repo)?),
        [host, owner, repo, ..] if host.contains('.') => {
            (host.to_string(), clean(owner)?, clean(repo)?)
        }
        _ => return None,
    };
    Some(CloneTarget {
        url: ssh_url(&host, &owner, &repo),
        host,
        owner,
        repo,
    })
}

/// Clone `url` into `dest`, creating parent directories. Blocking — callers
/// run it off the UI thread.
pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = Command::new("git")
        .args(["clone", "--", url])
        .arg(dest)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "git clone: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
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

#[cfg(test)]
mod tests {
    use super::parse_clone_target;

    fn parts(input: &str) -> (String, String, String, String) {
        let t = parse_clone_target(input).expect(input);
        (t.host, t.owner, t.repo, t.url)
    }

    #[test]
    fn scp_style_ssh() {
        assert_eq!(
            parts("git@github.com:poindexter12/baude.git"),
            (
                "github.com".into(),
                "poindexter12".into(),
                "baude".into(),
                "git@github.com:poindexter12/baude.git".into()
            )
        );
    }

    #[test]
    fn https_url_keeps_https() {
        let (host, owner, repo, url) = parts("https://github.com/poindexter12/baude");
        assert_eq!(host, "github.com");
        assert_eq!(owner, "poindexter12");
        assert_eq!(repo, "baude");
        assert_eq!(url, "https://github.com/poindexter12/baude.git");
    }

    #[test]
    fn browser_url_extra_segments_and_query() {
        let (_, owner, repo, _) =
            parts("https://github.com/poindexter12/baude/tree/main?tab=readme");
        assert_eq!(owner, "poindexter12");
        assert_eq!(repo, "baude");
    }

    #[test]
    fn ssh_scheme_url() {
        let (_, _, _, url) = parts("ssh://git@github.com/poindexter12/baude.git");
        assert_eq!(url, "git@github.com:poindexter12/baude.git");
    }

    #[test]
    fn shorthand_defaults_to_github_ssh() {
        let (host, owner, repo, url) = parts("poindexter12/baude");
        assert_eq!(host, "github.com");
        assert_eq!(owner, "poindexter12");
        assert_eq!(repo, "baude");
        assert_eq!(url, "git@github.com:poindexter12/baude.git");
    }

    #[test]
    fn schemeless_host_path() {
        let (host, _, _, url) = parts("github.com/poindexter12/baude");
        assert_eq!(host, "github.com");
        assert_eq!(url, "git@github.com:poindexter12/baude.git");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_clone_target("").is_none());
        assert!(parse_clone_target("baude").is_none());
        assert!(parse_clone_target("https://github.com/").is_none());
        assert!(parse_clone_target("a/b/c").is_none());
    }
}
