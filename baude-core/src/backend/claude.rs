//! The Claude Code backend — the original (and currently only) implementation
//! of [`Backend`]. A thin composition layer: the actual Claude-specific
//! machinery stays in the modules that already own it ([`crate::hook`],
//! [`crate::bridge`], [`crate::meta`], [`crate::permission`]); this impl is
//! the single choke point the binaries call so those modules stop being
//! spawn-path API.

use std::path::Path;

use super::{Backend, SpawnMode, SpawnPlan, RESUME_ID_ENV};
use crate::meta::ClaudeMeta;
use crate::permission::ResolvedCmd;

pub struct ClaudeBackend;

impl Backend for ClaudeBackend {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn default_cmd(&self) -> &'static str {
        "claude"
    }

    fn resolve_cmd(&self, base_cmd: &str) -> ResolvedCmd {
        crate::permission::resolve_claude_cmd_env(base_cmd)
    }

    /// `claude --continue` resumes the most recent conversation; on a fresh
    /// directory it exits non-zero and the `|| exec claude` fallback starts a
    /// new session. The env var is set with `export VAR=...; <inner>` rather
    /// than a `VAR=... cmd` assignment prefix: an assignment prefix applies
    /// only to the single command it prefixes, so on the resume path the
    /// `exec claude` fallback (the common fresh-directory case) would
    /// otherwise run WITHOUT the var and its hooks would silently miss the
    /// daemon transport. `export` sets it for the whole command group,
    /// surviving the `||` fallback and sub-exec (WR-01).
    fn spawn_plan(
        &self,
        resolved_cmd: &str,
        event_url: Option<&str>,
        mode: SpawnMode,
    ) -> SpawnPlan {
        let inner = match &mode {
            SpawnMode::Fresh => format!("exec {resolved_cmd}"),
            SpawnMode::ContinueLatest => {
                format!("{resolved_cmd} --continue 2>/dev/null || exec {resolved_cmd}")
            }
            SpawnMode::ResumeId(_) => {
                format!("exec {resolved_cmd} --resume \"${RESUME_ID_ENV}\"")
            }
        };
        let cmd = match event_url {
            Some(url) => format!("export BAUDE_EVENT_URL={url}; {inner}"),
            None => inner,
        };
        SpawnPlan {
            cmd,
            env: mode.environment(),
            server_port: None,
        }
    }

    /// Seed `.claude/settings.local.json` so the spawned Claude fires baude's
    /// lifecycle hooks, and — in `prompt` mode only — a non-clobbering
    /// `.mcp.json` registering the `permission-mcp` stdio server. Both seeds
    /// resolve `current_exe()`, so a daemon-spawned session wires `bauded`
    /// and a TUI one wires `baude` (the Pitfall-2 reason both binaries carry
    /// the `hook`/`permission-mcp` arms).
    fn prepare_cwd(&self, cwd: &Path) -> Vec<crate::hook::SeedWarning> {
        let mut warnings = crate::hook::seed_settings(cwd);
        if crate::permission::is_prompt_mode() {
            warnings.extend(seed_mcp_config(cwd));
        }
        warnings
    }

    fn poll_meta(
        &self,
        meta: &mut ClaudeMeta,
        cwd: &Path,
        pid: Option<u32>,
        spawn_unix_ms: u64,
        repo_root: &Path,
    ) {
        meta.poll(cwd, pid, spawn_unix_ms, repo_root);
    }

    /// The `permission-mcp` bridge fails CLOSED (denies every tool) when no
    /// daemon injects `$BAUDE_EVENT_URL` — a bare-TUI prompt-mode session
    /// would silently deny everything, so the TUI must warn (WR-01).
    fn prompt_mode_needs_daemon(&self) -> bool {
        true
    }
}

/// Best-effort, non-clobbering seed of a session cwd's `.mcp.json` registering
/// baude's `permission-mcp` stdio server (PERM-01, `prompt` mode only).
///
/// The MCP command is `current_exe()` + ` permission-mcp` (same resolution as
/// [`crate::hook::baude_hook_command`]). Mirrors `seed_settings`: never aborts
/// a spawn on failure, and re-seeding merges `mcpServers.baude` into an
/// existing file via the pure `merge_mcp_config` without discarding a user's
/// sibling MCP servers (idempotent — the command is the sentinel). Previously
/// duplicated byte-identically in `baude/src/app.rs` and `bauded/src/manager.rs`;
/// this is now the single copy.
///
/// Guarded like [`crate::hook::seed_settings`] (D-03, HREG-03): an existing
/// `.mcp.json` that cannot be read, parsed, or whose root is not an object is
/// left byte-identical and reported via [`crate::hook::SeedWarning`] instead
/// of being replaced with the seed alone. The `command` field stays the RAW
/// `current_exe()` string — Claude Code spawns stdio MCP servers directly
/// (no shell), so it is argv data and must never be quoted (D-09).
fn seed_mcp_config(cwd: &Path) -> Vec<crate::hook::SeedWarning> {
    use crate::hook::{SeedWarning, SeedWarningReason};
    let exe = match std::env::current_exe() {
        Ok(p) => p.display().to_string(),
        Err(_) => return Vec::new(), // can't resolve the bridge command — best-effort skip.
    };
    let path = crate::permission::mcp_config_path(cwd);
    // D-03: the identical guard seed_settings uses — on Err the file is user
    // content we could not safely understand; leave it untouched and report.
    let existing = match crate::hook::read_settings_guarded(&path) {
        Ok(existing) => existing,
        Err(warning) => return vec![warning],
    };
    // D-09: `exe` reaches the merge unmodified — argv data for a direct spawn.
    let merged = crate::permission::merge_mcp_config(&existing, &exe);
    match std::fs::write(&path, merged.to_string()) {
        Ok(()) => Vec::new(),
        // D-04: a write failure never aborts the spawn — warn and continue.
        Err(error) => vec![SeedWarning {
            file: path,
            reason: SeedWarningReason::WriteFailed(error),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::SpawnMode;

    // ---- spawn_plan -----------------------------------------------------
    //
    // These pin the exact spawn strings both binaries relied on before the
    // seam (moved from bauded/src/manager.rs's spawn_command tests): the
    // daemon path (event_url = Some) and the TUI path (event_url = None).

    #[test]
    fn spawn_plan_exports_event_url_on_both_paths() {
        // WR-01: the event URL must be exported (not assignment-prefixed) so it
        // survives the resume path's `|| exec claude` fallback. Both the resume
        // and fresh commands must start with `export BAUDE_EVENT_URL=<url>;`.
        let url = "http://127.0.0.1:8642/sessions/3/event";

        let fresh = ClaudeBackend
            .spawn_plan("claude", Some(url), SpawnMode::Fresh)
            .cmd;
        assert_eq!(fresh, format!("export BAUDE_EVENT_URL={url}; exec claude"));

        let resumed = ClaudeBackend
            .spawn_plan("claude", Some(url), SpawnMode::ContinueLatest)
            .cmd;
        let prefix = format!("export BAUDE_EVENT_URL={url}; ");
        assert!(
            resumed.starts_with(&prefix),
            "resume command must export the var for the whole group, got: {resumed}"
        );
        // The fallback after `||` is inside the exported scope (no second
        // assignment prefix on `exec claude`), so it inherits the var too.
        assert!(resumed.contains("--continue 2>/dev/null || exec claude"));
        assert!(
            !resumed.contains("|| BAUDE_EVENT_URL="),
            "fallback must not re-prefix; export already covers it"
        );
    }

    #[test]
    fn spawn_plan_tui_path_has_no_export() {
        // TUI sessions get NO $BAUDE_EVENT_URL (only the daemon injects it),
        // which routes hook events to the /tmp append path. Exact strings the
        // TUI spawn used before the seam.
        assert_eq!(
            ClaudeBackend
                .spawn_plan("claude", None, SpawnMode::Fresh)
                .cmd,
            "exec claude"
        );
        assert_eq!(
            ClaudeBackend
                .spawn_plan("claude", None, SpawnMode::ContinueLatest)
                .cmd,
            "claude --continue 2>/dev/null || exec claude"
        );
    }

    #[test]
    fn targeted_resume_is_opaque_environment_data() {
        let hostile = "--help ; $(touch /tmp/baude-nope) ' quoted value";
        let plan = ClaudeBackend.spawn_plan(
            "claude --dangerously-skip-permissions",
            None,
            SpawnMode::ResumeId(hostile.into()),
        );

        assert_eq!(
            plan.cmd,
            "exec claude --dangerously-skip-permissions --resume \"$BAUDE_RESUME_ID\""
        );
        assert_eq!(
            plan.env,
            vec![("BAUDE_RESUME_ID".into(), hostile.to_string())]
        );
        assert!(!plan.cmd.contains(hostile));
        assert!(!plan.cmd.contains("--continue"));
    }

    #[test]
    fn permission_mode_default_skip_and_prompt_at_spawn_plan() {
        // PERM-01 (security-critical): the spawn command must carry
        // `--dangerously-skip-permissions` by default (BAUDE_PERMISSION_MODE
        // unset/skip) and `--permission-prompt-tool` ONLY in `prompt` mode.
        // Pins the exact composition both spawn paths use: base_cmd =
        // claude_cmd + permission_flag(claude_cmd), then spawn_plan wraps it.
        //
        // Exercises the env-free `resolve_claude_cmd` seam so the test never
        // mutates the process-global BAUDE_PERMISSION_MODE — which would race
        // concurrently-running tests that read it.
        let url = "http://127.0.0.1:8642/sessions/1/event";
        let flagged = |claude: &str, mode: Option<&str>| {
            crate::permission::resolve_claude_cmd(mode, claude).cmd
        };

        // Default (unset) and explicit skip and unrecognized -> skip flag,
        // never the prompt flag (fail-safe default).
        for mode in [None, Some("skip"), Some("bogus")] {
            let cmd = ClaudeBackend
                .spawn_plan(&flagged("claude", mode), Some(url), SpawnMode::Fresh)
                .cmd;
            assert!(
                cmd.contains("--dangerously-skip-permissions"),
                "mode {mode:?} must skip permissions, got: {cmd}"
            );
            assert!(
                !cmd.contains("--permission-prompt-tool"),
                "mode {mode:?} must NOT prompt, got: {cmd}"
            );
        }

        // prompt -> prompt flag present, skip flag absent; survives the resume
        // `--continue || exec` fallback (appended to the inner base cmd).
        let cmd = ClaudeBackend
            .spawn_plan(
                &flagged("claude", Some("prompt")),
                Some(url),
                SpawnMode::ContinueLatest,
            )
            .cmd;
        assert!(
            cmd.contains("--permission-prompt-tool mcp__baude__approve"),
            "prompt mode must wire the prompt tool, got: {cmd}"
        );
        assert!(
            !cmd.contains("--dangerously-skip-permissions"),
            "prompt mode must NOT also skip, got: {cmd}"
        );
        // The flag is on the base cmd so both `--continue` and the `exec`
        // fallback carry it.
        assert_eq!(
            cmd.matches("--permission-prompt-tool").count(),
            2,
            "resume path repeats the flagged base cmd on both sides of `||`: {cmd}"
        );

        // BL-04: a claude_cmd that already bakes in --dangerously-skip-permissions
        // must NOT suppress prompt mode — the skip is stripped and the prompt
        // flag wins (explicit opt-in), with no skip flag left in the spawn cmd.
        let cmd = ClaudeBackend
            .spawn_plan(
                &flagged("claude --dangerously-skip-permissions", Some("prompt")),
                Some(url),
                SpawnMode::Fresh,
            )
            .cmd;
        assert!(
            cmd.contains("--permission-prompt-tool mcp__baude__approve"),
            "BL-04: prompt must win over a baked-in skip flag, got: {cmd}"
        );
        assert!(
            !cmd.contains("--dangerously-skip-permissions"),
            "BL-04: the conflicting skip flag must be stripped, got: {cmd}"
        );
    }

    // ---- seed_mcp_config ------------------------------------------------

    #[test]
    fn seed_mcp_config_is_non_clobbering() {
        // PERM-01 / T-04-03: seeding `.mcp.json` in prompt mode must merge our
        // `baude` server without discarding a user's sibling MCP servers, and
        // must be idempotent across re-spawns (restore()). Moved from
        // bauded/src/manager.rs when the duplicated seeder collapsed here.
        let cwd = std::env::temp_dir().join(format!("baude-core-mcp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cwd);
        std::fs::create_dir_all(&cwd).unwrap();
        let path = crate::permission::mcp_config_path(&cwd);

        // Pre-existing user config with a sibling server.
        std::fs::write(
            &path,
            r#"{"mcpServers":{"other":{"command":"other-srv"}},"extra":true}"#,
        )
        .unwrap();

        seed_mcp_config(&cwd);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        // Sibling server + unrelated key preserved.
        assert_eq!(
            v["mcpServers"]["other"]["command"].as_str(),
            Some("other-srv")
        );
        assert_eq!(v["extra"].as_bool(), Some(true));
        // Our server registered with the permission-mcp arg.
        assert_eq!(
            v["mcpServers"]["baude"]["args"][0].as_str(),
            Some("permission-mcp")
        );

        // Idempotent: re-seeding leaves exactly one `baude` server, sibling intact.
        seed_mcp_config(&cwd);
        let v2: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            v2["mcpServers"]["other"]["command"].as_str(),
            Some("other-srv")
        );
        assert_eq!(
            v2["mcpServers"]["baude"]["args"][0].as_str(),
            Some("permission-mcp")
        );

        let _ = std::fs::remove_dir_all(&cwd);
    }

    // ---- seed_mcp_config guard (HREG-03 / D-03) --------------------------

    /// Unique per-test cwd fixture, same shape as hook.rs's `seed_guard_cwd`
    /// (temp_dir + pid-suffixed).
    fn mcp_guard_cwd(tag: &str) -> std::path::PathBuf {
        let cwd =
            std::env::temp_dir().join(format!("baude-mcp-guard-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cwd);
        std::fs::create_dir_all(&cwd).unwrap();
        cwd
    }

    #[test]
    fn mcp_guard_unparseable_left_untouched_and_warned() {
        // D-03: a `.mcp.json` that fails JSON parsing is user content — it
        // survives a prompt-mode seed attempt byte-identical and the caller
        // gets the identical warning shape `seed_settings` uses.
        let cwd = mcp_guard_cwd("unparseable");
        let path = crate::permission::mcp_config_path(&cwd);
        std::fs::write(&path, "{not json").unwrap();

        let warnings = seed_mcp_config(&cwd);

        assert_eq!(warnings.len(), 1, "expected exactly one SeedWarning");
        assert!(
            warnings[0].file.ends_with(".mcp.json"),
            "warning must name the mcp config file, got {:?}",
            warnings[0].file
        );
        assert!(
            matches!(
                warnings[0].reason,
                crate::hook::SeedWarningReason::Unparseable(_)
            ),
            "expected Unparseable, got {:?}",
            warnings[0].reason
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"{not json".to_vec(),
            "unparseable file must remain byte-identical"
        );

        let _ = std::fs::remove_dir_all(&cwd);
    }

    #[test]
    fn mcp_guard_non_object_root_left_untouched() {
        // D-03: valid JSON whose root is not an object is user content too —
        // never coerced to `{}` and overwritten.
        let cwd = mcp_guard_cwd("non-object");
        let path = crate::permission::mcp_config_path(&cwd);
        std::fs::write(&path, "[1,2]").unwrap();

        let warnings = seed_mcp_config(&cwd);

        assert_eq!(warnings.len(), 1, "expected exactly one SeedWarning");
        assert!(
            matches!(
                warnings[0].reason,
                crate::hook::SeedWarningReason::NonObjectRoot
            ),
            "expected NonObjectRoot, got {:?}",
            warnings[0].reason
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"[1,2]".to_vec(),
            "non-object file must remain byte-identical"
        );

        let _ = std::fs::remove_dir_all(&cwd);
    }

    #[test]
    fn mcp_guard_missing_file_fresh_seed() {
        // D-03: a missing `.mcp.json` stays the normal fresh-seed path — no
        // warning, and the written file registers baude's server.
        let cwd = mcp_guard_cwd("missing");

        let warnings = seed_mcp_config(&cwd);

        assert!(
            warnings.is_empty(),
            "fresh seed must not warn, got {warnings:?}"
        );
        let path = crate::permission::mcp_config_path(&cwd);
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap())
            .expect("fresh seed wrote valid JSON");
        assert!(
            v["mcpServers"]["baude"]["command"].is_string(),
            "fresh seed must register mcpServers.baude.command, got {v}"
        );

        let _ = std::fs::remove_dir_all(&cwd);
    }

    #[test]
    fn mcp_command_is_raw_argv_not_shell_quoted() {
        // D-09: the `.mcp.json` command field is argv data for a DIRECT
        // process spawn (Claude Code launches stdio MCP servers without a
        // shell), so it must equal `current_exe()` displayed EXACTLY — a
        // single-quoted form would make the spawn look for a file whose name
        // literally contains quote characters (self-inflicted DoS).
        let cwd = mcp_guard_cwd("raw-argv");

        let warnings = seed_mcp_config(&cwd);
        assert!(
            warnings.is_empty(),
            "fresh seed must not warn, got {warnings:?}"
        );

        let path = crate::permission::mcp_config_path(&cwd);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let command = v["mcpServers"]["baude"]["command"]
            .as_str()
            .expect("command field present");
        let exe = std::env::current_exe().unwrap().display().to_string();
        assert_eq!(
            command, exe,
            "command must equal current_exe() EXACTLY — raw argv, no quoting"
        );
        assert!(
            !command.starts_with('\''),
            "command must not start with a single-quote character, got: {command}"
        );

        let _ = std::fs::remove_dir_all(&cwd);
    }
}
