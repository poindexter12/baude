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
    use super::{discover_repository, parse_clone_target, parse_worktree_porcelain};
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    struct GitFixture {
        root: PathBuf,
    }

    impl GitFixture {
        fn new() -> Self {
            let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "baude-git-test-{}-{sequence}",
                std::process::id()
            ));
            std::fs::create_dir(&root).expect("create unique Git fixture root");
            Self { root }
        }

        fn repo(&self, relative: impl AsRef<Path>) -> PathBuf {
            let repo = self.root.join(relative);
            git_ok(&self.root, &[OsStr::new("init"), repo.as_os_str()]);
            git_ok(
                &repo,
                &[OsStr::new("config"), OsStr::new("user.name"), OsStr::new("Baude Test")],
            );
            git_ok(
                &repo,
                &[
                    OsStr::new("config"),
                    OsStr::new("user.email"),
                    OsStr::new("baude@example.invalid"),
                ],
            );
            std::fs::write(repo.join("tracked.txt"), b"fixture\n").expect("write fixture file");
            git_ok(&repo, &[OsStr::new("add"), OsStr::new("tracked.txt")]);
            git_ok(
                &repo,
                &[OsStr::new("commit"), OsStr::new("-m"), OsStr::new("fixture")],
            );
            repo
        }

        fn linked_worktree(&self, repo: &Path, relative: impl AsRef<Path>, branch: &str) -> PathBuf {
            let path = self.root.join(relative);
            git_ok(
                repo,
                &[OsStr::new("branch"), OsStr::new(branch)],
            );
            git_ok(
                repo,
                &[
                    OsStr::new("worktree"),
                    OsStr::new("add"),
                    path.as_os_str(),
                    OsStr::new(branch),
                ],
            );
            path
        }
    }

    impl Drop for GitFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn git_ok(cwd: &Path, args: &[&OsStr]) -> Vec<u8> {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("run fixture Git command");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    mod admission_identity {
        use super::*;

        #[test]
        fn aliases_converge_on_common_directory_and_main_record() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("main repo");
            let nested = repo.join("nested");
            std::fs::create_dir(&nested).unwrap();
            let linked = fixture.linked_worktree(&repo, "linked checkout", "linked");

            let mut inputs = vec![repo.clone(), nested, linked];
            #[cfg(unix)]
            {
                let alias = fixture.root.join("repo alias");
                std::os::unix::fs::symlink(&repo, &alias).unwrap();
                inputs.push(alias);
            }

            let snapshots: Vec<_> = inputs
                .iter()
                .map(|path| discover_repository(path).unwrap())
                .collect();
            let expected_common = snapshots[0].common_dir.clone();
            let expected_main = snapshots[0].main_worktree.clone();
            for snapshot in snapshots {
                assert_eq!(snapshot.common_dir, expected_common);
                assert_eq!(snapshot.main_worktree, expected_main);
                assert_eq!(snapshot.worktrees.first().unwrap().path, expected_main);
                assert!(snapshot.worktrees.contains(&snapshot.selected_worktree));
            }
        }

        #[test]
        fn repositories_with_the_same_basename_remain_distinct() {
            let fixture = GitFixture::new();
            let first = fixture.repo("first/same-name");
            let second = fixture.repo("second/same-name");

            let first = discover_repository(&first).unwrap();
            let second = discover_repository(&second).unwrap();
            assert_ne!(first.common_dir, second.common_dir);
            assert_ne!(first.main_worktree, second.main_worktree);
        }

        #[cfg(unix)]
        #[test]
        fn nul_inventory_preserves_spaces_and_newlines_in_paths() {
            let fixture = GitFixture::new();
            let repo = fixture.repo("unusual repo");
            let linked = fixture.linked_worktree(&repo, "linked space\nline", "unusual");

            let snapshot = discover_repository(&linked).unwrap();
            assert_eq!(snapshot.selected_worktree.path, linked.canonicalize().unwrap());
            assert!(snapshot
                .worktrees
                .iter()
                .any(|record| record.path == linked.canonicalize().unwrap()));
        }

        #[test]
        fn malformed_porcelain_fails_closed() {
            let malformed = b"worktree /tmp/example\0HEAD deadbeef\0branch refs/heads/main\0";
            assert!(parse_worktree_porcelain(malformed).is_err());
        }

        #[test]
        fn invalid_inputs_are_actionable_and_non_mutating() {
            let fixture = GitFixture::new();
            let ordinary = fixture.root.join("ordinary");
            std::fs::create_dir(&ordinary).unwrap();
            let missing = fixture.root.join("missing");

            let ordinary_error = discover_repository(&ordinary).unwrap_err().to_string();
            assert!(ordinary_error.contains("Git") || ordinary_error.contains("repository"));
            let missing_error = discover_repository(&missing).unwrap_err().to_string();
            assert!(missing_error.contains("canonicalize"));
            assert!(!missing.exists());
        }
    }

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
