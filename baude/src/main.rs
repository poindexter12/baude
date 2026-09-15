mod app;
mod hierarchy;
mod keys;
mod notify_desktop;
mod remote;
mod ui;
mod usage;

use std::io::stdout;
use std::time::Duration;

use anyhow::Result;

fn daemon_is_up(url: &str) -> bool {
    ureq::get(&format!("{url}/sessions"))
        .timeout(Duration::from_millis(300))
        .call()
        .is_ok()
}

/// If `auto_daemon` is enabled (config or env) and no explicit daemon URL is
/// configured, ensure a local `bauded` is running FOR THIS WORKSPACE and
/// return its URL. Each workspace gets its own daemon on its own port
/// (claude 8642, opencode 8643, custom via `workspaces.<n>.daemon_port`) so
/// two workspaces running side-by-side never share a session pool.
/// Returns `None` when auto-daemon is disabled or already handled by config.
fn ensure_daemon(config: &baude_core::persist::Config) -> Option<String> {
    let ws = baude_core::workspace::active();
    // Explicit URL already configured — nothing to do.
    if std::env::var("BAUDE_DAEMON_URL").is_ok()
        || ws.daemon_url.is_some()
        || config.daemon_url.is_some()
    {
        return None;
    }
    let auto = config.auto_daemon
        || std::env::var("BAUDE_AUTO_DAEMON")
            .map(|v| matches!(v.as_str(), "1" | "true"))
            .unwrap_or(false);
    if !auto {
        return None;
    }
    let Some(port) = ws.auto_daemon_port() else {
        eprintln!(
            "baude: auto_daemon needs workspaces.{}.daemon_port in config — skipping",
            ws.name
        );
        return None;
    };
    let url = format!("http://127.0.0.1:{port}");
    if daemon_is_up(&url) {
        return Some(url);
    }
    // Locate bauded: same directory as this binary, then PATH.
    let bauded = std::env::current_exe()
        .ok()
        .map(|p| p.with_file_name("bauded"))
        .filter(|p| p.exists())
        .or_else(|| {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|dir| dir.join("bauded"))
                    .find(|p| p.exists())
            })
        })?;
    // Spawn detached; drop the handle — bauded outlives the TUI. Workspace
    // and bind are passed EXPLICITLY: env inheritance alone would miss a
    // workspace selected via config, and the per-workspace port must win
    // over any BAUDED_BIND in the environment.
    std::process::Command::new(&bauded)
        .env("BAUDE_WORKSPACE", &ws.name)
        .env("BAUDED_BIND", format!("127.0.0.1:{port}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    // Wait up to 2 s for bauded to bind.
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(200));
        if daemon_is_up(&url) {
            return Some(url);
        }
    }
    None
}
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};

use app::App;

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        stdout(),
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen
    );
}

/// `<binary> hook` — Claude Code lifecycle-event hook, no TUI. Claude invokes it
/// headless per event, piping the hook JSON to stdin. Reads stdin, then defers
/// to `baude_core::hook::dispatch_hook`, which normalizes the payload and routes
/// it: POST to `$BAUDE_EVENT_URL` (daemon transport) or append to
/// `/tmp/baude-events-<sid>.jsonl` (TUI-local). On a POST failure (wrong/dead
/// port, transport error, OR timeout) it falls back to the file-append so the
/// event is never silently lost — the daemon tails the same file (WR-02).
///
/// The POST uses a bounded agent (WR-04): the hook runs synchronously in Claude
/// Code's critical path and the contract is "ALWAYS exit 0 so a hook failure
/// never blocks Claude". A loopback peer that accepts then stalls would hang the
/// POST; the connect/read timeouts cap that, then the file-append fallback runs.
///
/// NOTE: `bauded` carries a byte-identical `run_hook` because `seed_settings`
/// seeds `current_exe()` and the daemon binary spawns its own sessions — keep
/// the two in sync (the shared normalization lives in `dispatch_hook`).
fn run_hook() -> ! {
    use std::io::Read;
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let url = std::env::var("BAUDE_EVENT_URL").ok();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(2))
        .build();
    baude_core::hook::dispatch_hook(&input, url.as_deref(), |url, line| {
        agent.post(url).send_string(line).is_ok()
    });
    std::process::exit(0);
}

/// `<binary> permission-mcp` — the blocking stdio JSON-RPC MCP server Claude
/// invokes (via `--permission-prompt-tool mcp__baude__approve`) for each
/// unresolved tool-permission decision in `prompt` mode. The BLOCKING inverse of
/// `run_hook`: where the hook is fire-and-forget exit-0, this sits on Claude's
/// critical path and each `tools/call` blocks until a human `allow`/`deny`
/// arrives OR the deadline denies (deny-on-timeout, never auto-allow — V4).
///
/// All framing/protocol lives in `baude_core::permission::run_permission_mcp`;
/// this binary owns only the env read + the `ureq` daemon round-trip (the
/// `dispatch_hook` split). The resolver POSTs the pending request to
/// `…/sessions/{id}/permission` then long-polls GET until a decision appears or
/// `$BAUDE_PERMISSION_TIMEOUT_S` (default 120s) elapses. If `$BAUDE_EVENT_URL`
/// is absent (no daemon), it fails CLOSED to `deny` (never allow).
///
/// NOTE: `bauded` carries a byte-identical `run_permission_mcp` because the
/// daemon seeds `current_exe()` (= `bauded`) as the `.mcp.json` command — WITHOUT
/// the arm, `bauded permission-mcp` would fall through and boot a *second
/// daemon* (the Phase-2 `bauded hook` trap; Pitfall 2). Keep the two in sync.
fn run_permission_mcp() -> ! {
    use baude_core::permission::{
        decide_with_timeout, permission_timeout_s, permission_url_from_event_url,
    };
    use std::time::{Duration, Instant};

    let timeout_s = permission_timeout_s();
    let perm_url = std::env::var("BAUDE_EVENT_URL")
        .ok()
        .and_then(|u| permission_url_from_event_url(&u));
    // WR-02: the client read timeout MUST be strictly greater than the server
    // long-poll window (`wait=5` below). When they are equal the GET frequently
    // times out at the exact boundary the daemon is still holding the poll open,
    // converting every long-poll into a spurious timeout-then-retry. 8s > 5s
    // leaves headroom while staying well under the deny-on-deadline window
    // (BAUDE_PERMISSION_TIMEOUT_S, default 120s, unchanged).
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(500))
        .timeout(Duration::from_secs(8))
        .build();
    let mut req_counter: u64 = 0;

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    baude_core::permission::run_permission_mcp(stdin.lock(), stdout.lock(), |tool, input| {
        // Fail closed: no daemon URL -> deny (never allow) — the no-daemon path.
        let Some(perm_url) = perm_url.as_deref() else {
            return "deny".to_string();
        };
        req_counter += 1;
        let request_id = format!("{}-{}", std::process::id(), req_counter);
        let req = serde_json::json!({ "request_id": request_id, "tool": tool, "input": input });
        // Register the pending request (best-effort POST). A failure still falls
        // into the poll loop, which denies on the deadline. The body IS JSON, so
        // declare `application/json` — the daemon's `Json<PermissionBody>`
        // extractor 415s a `text/plain` body (ureq's `send_string` default), which
        // would silently drop the registration and deny every tool (PERM-BUG).
        let _ = agent
            .post(perm_url)
            .set("Content-Type", "application/json")
            .send_string(&req.to_string());

        // Long-poll GET until a decision for THIS request appears or the
        // deadline passes -> deny (deny-on-timeout, security-critical, V4).
        let deadline = Instant::now() + Duration::from_secs(timeout_s);
        loop {
            let mut decision: Option<String> = None;
            if let Ok(resp) = agent.get(perm_url).query("wait", "5").call() {
                if let Ok(v) = resp.into_json::<serde_json::Value>() {
                    if v["request_id"].as_str() == Some(request_id.as_str()) {
                        decision = v["decision"].as_str().map(str::to_string);
                    }
                }
            }
            let passed = Instant::now() >= deadline;
            match decide_with_timeout(decision.as_deref(), passed) {
                "" => std::thread::sleep(Duration::from_millis(500)), // keep polling
                verdict => break verdict.to_string(),
            }
        }
    });
    std::process::exit(0);
}

/// The top-level `--help` body.
///
/// Extracted from `main` so the subcommand list is assertable: a verb that
/// dispatches but is undiscoverable is half-shipped, and the only way to keep
/// the list and the dispatch a matched pair is to test the list.
fn help_text() -> String {
    format!(
        "baude {} — multiple AI coding sessions in one terminal\n\n\
         usage: baude [<repo-dir>]\n\n\
         subcommands: statusline, hook, permission-mcp\n\
         options:     --version/-V, --help/-h",
        env!("CARGO_PKG_VERSION")
    )
}

fn main() -> Result<()> {
    // `baude statusline [--wrap <cmd>]` — statusline bridge mode, no TUI.
    // Must be dispatched before anything touches the terminal: Claude Code
    // invokes it headless on every statusline refresh.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("statusline") {
        let wrap = args
            .iter()
            .position(|a| a == "--wrap")
            .and_then(|i| args.get(i + 1))
            .cloned();
        std::process::exit(baude_core::bridge::run(wrap));
    }

    // `baude hook` — Claude Code lifecycle-event hook, no TUI. Claude invokes
    // it headless per event, piping the hook JSON to stdin. We normalize it to
    // one event line and route it: POST to `$BAUDE_EVENT_URL` (daemon
    // transport) or append to `/tmp/baude-events-<sid>.jsonl` (TUI-local).
    // Best-effort throughout — ALWAYS exit 0 so a hook failure never blocks
    // Claude (a non-zero exit is a blocking signal to the CLI).
    if args.get(1).map(String::as_str) == Some("hook") {
        run_hook();
    }

    // `baude permission-mcp` — the blocking stdio JSON-RPC permission bridge
    // Claude invokes in `prompt` mode. MUST be dispatched before the TUI
    // touches the terminal (it speaks MCP on stdio, no UI). Byte-identical to
    // the `bauded` arm (Pitfall 2). Blocks on Claude's critical path with
    // deny-on-timeout — contrast `hook`'s always-exit-0.
    if args.get(1).map(String::as_str) == Some("permission-mcp") {
        run_permission_mcp();
    }

    // `baude --version` / `--help` — print and exit BEFORE the launch-dir logic
    // below (which would otherwise treat the flag as a repo path and boot the
    // TUI without a TTY). Mirrors the `bauded` arms.
    match args.get(1).map(String::as_str) {
        Some("--version" | "-V") => {
            println!("baude {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!("{}", help_text());
            return Ok(());
        }
        _ => {}
    }

    let launch_dir = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let launch_dir = launch_dir.canonicalize().unwrap_or(launch_dir);

    let config = baude_core::persist::load_config();

    // Folder-workspace memory: consult this folder's remembered workspace and
    // pin the process-wide resolution BEFORE anything reads workspace::active()
    // (ensure_daemon below is the first reader — the auto-daemon must serve
    // the same workspace). Explicit BAUDE_WORKSPACE/BAUDE_BACKEND always win;
    // the folder_context kill switch disables both consulting and recording.
    // Only this TUI launch path passes a hint — the statusline/hook/
    // permission-mcp subcommands exited above and resolve untouched.
    let ws_env = std::env::var("BAUDE_WORKSPACE").ok();
    let backend_env = std::env::var("BAUDE_BACKEND").ok();
    let memory_root = baude_core::persist::config_dir();
    let plan = baude_core::folder_workspace::plan_launch(
        config.folder_context_enabled(),
        ws_env.as_deref(),
        backend_env.as_deref(),
        Some(&memory_root),
        &launch_dir,
    );
    let workspace = baude_core::workspace::initialize(&config, plan.hint.as_deref());

    // One writer per workspace. Claim the state lock BEFORE the terminal, the
    // daemon, or any folder-memory write: a second baude on a held lock used
    // to start in a degraded mode where every later action failed with
    // "persistence is blocked" and nothing named the real cause (#71). Refuse
    // here instead, once, while stderr is still a normal terminal.
    // A lock we cannot even open (unwritable config dir) is NOT a refusal:
    // that path still degrades through App::restore the way it always has.
    if let Err(baude_core::persist::StateLockError::Held { path, holder }) =
        baude_core::persist::claim_workspace_state_lock("state", workspace)
    {
        match holder {
            Some(pid) => eprintln!(
                "baude: workspace {} is already open in another baude (pid {pid}).",
                workspace.name
            ),
            None => eprintln!(
                "baude: workspace {} is already open in another baude.",
                workspace.name
            ),
        }
        eprintln!(
            "       Quit that instance, or run this one in another workspace: \
             BAUDE_WORKSPACE=<name> baude"
        );
        eprintln!("       lock: {}", path.display());
        std::process::exit(1);
    }

    let mut startup_notes = plan.notes;
    if config.folder_context_enabled() {
        startup_notes.extend(baude_core::folder_workspace::applied_note(
            &workspace.name,
            ws_env.as_deref(),
            backend_env.as_deref(),
            &config,
        ));
        baude_core::folder_workspace::record(
            Some(&memory_root),
            &launch_dir,
            &workspace.name,
            baude_core::pty::now_ms(),
        );
    }

    // Auto-start local bauded when auto_daemon is configured. Must run before
    // App::new() reads the env, and before any threads start (set_var is not
    // thread-safe, but we're still single-threaded here).
    if let Some(url) = ensure_daemon(&config) {
        std::env::set_var("BAUDE_DAEMON_URL", url);
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    enable_raw_mode()?;
    execute!(
        stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout()))?;

    let mut app = App::new(launch_dir);
    // Folder-memory notes go up first so a real restore error overwrites an
    // informational banner, never the other way around.
    for note in startup_notes {
        app.set_message(note);
    }
    app.restore();

    let result = run(&mut terminal, &mut app);

    app.save();
    app.kill_all();
    restore_terminal();
    result
}

fn run(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        app.tick();

        let area = terminal.get_frame().area();
        app.sync_sizes(area);

        terminal.draw(|frame| ui::draw(frame, app))?;

        // Drain pending events, then sleep briefly (the draw loop doubles as
        // the refresh tick for streaming PTY output and status timers).
        if event::poll(Duration::from_millis(50))? {
            loop {
                app.handle_event(event::read()?);
                if !event::poll(Duration::from_millis(0))? {
                    break;
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

// ---------------------------------------------------------------------------
// `baude worktrees scan` — the TISO-04 leak preview surface.
//
// `baude-core::worktree_scan` already enumerates, classifies, cross-references
// persisted state and re-verifies a prune. None of it is reachable by a
// developer, and TISO-04's deliverable is a preview they can *run*. This is
// that surface and nothing more: the default path reads, prints and exits.
//
// Two distinct opt-ins are required before anything is removed (`--prune` AND
// `--yes`), and the removal set comes from a report the operator previously
// saved and inspected — never from a fresh scan (D-15, T-08-18, T-08-25). The
// verb is deliberately absent from `bauded`: a headless daemon has no operator
// present to approve a deletion (T-08-17).
// ---------------------------------------------------------------------------

// RED scaffolding note: nothing in `main` dispatches here yet, so a non-test
// build sees every item below as dead. The dispatch arm — and the deletion of
// these `allow`s — ships in the GREEN commit; `-D warnings` would otherwise
// fail the RED commit for the very absence the RED tests are asserting.

/// Exit code when the command did what was asked.
#[allow(dead_code)]
const WORKTREES_EXIT_OK: i32 = 0;
/// Exit code when the command could not complete. Nothing was removed.
#[allow(dead_code)]
const WORKTREES_EXIT_FAILED: i32 = 1;
/// Exit code when the command line itself was wrong. Nothing was read or
/// removed — the arguments never reached the filesystem.
#[allow(dead_code)]
const WORKTREES_EXIT_USAGE: i32 = 2;

/// The parsed `baude worktrees scan` command line.
#[allow(dead_code)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct WorktreesOptions {
    json: bool,
    prune: bool,
    report: Option<std::path::PathBuf>,
    yes: bool,
}

/// RED stub — fail closed.
///
/// Reads nothing, prints no preview an operator could mistake for one, and
/// removes nothing. Replaced wholesale by the GREEN commit.
#[allow(dead_code)]
fn run_worktrees_at(
    _rest: &[String],
    _roots: &baude_core::worktree_scan::ScanRoots,
    _out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> i32 {
    let _ = writeln!(err, "baude worktrees: not implemented");
    WORKTREES_EXIT_FAILED
}

#[cfg(test)]
mod worktrees_cli_tests {
    use super::*;
    use baude_core::repository::{
        PersistedPath, RepositoryHealth, RepositoryState, SavedRepository,
    };
    use baude_core::worktree_scan::{ScanReport, ScanRoots, REPORT_FORMAT_VERSION};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

    /// A synthetic pair of scan roots plus a report directory that lies outside
    /// both of them, so saving a report can never be mistaken for a write into
    /// a root the read-only contract asserts is unchanged.
    struct CliFixture {
        root: PathBuf,
    }

    impl CliFixture {
        fn new(label: &str) -> Self {
            let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "baude-worktrees-cli-{label}-{}-{sequence}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("worktrees")).expect("create fixture worktrees base");
            std::fs::create_dir_all(root.join("config")).expect("create fixture config dir");
            std::fs::create_dir_all(root.join("reports")).expect("create fixture report dir");
            std::fs::create_dir_all(root.join("external")).expect("create fixture external dir");
            Self { root }
        }

        fn base(&self) -> PathBuf {
            self.root.join("worktrees")
        }

        fn config(&self) -> PathBuf {
            self.root.join("config")
        }

        fn roots(&self) -> ScanRoots {
            ScanRoots {
                worktrees_base: self.base(),
                config_dir: self.config(),
            }
        }

        /// A report path outside both scan roots.
        fn report_path(&self, name: &str) -> PathBuf {
            self.root.join("reports").join(name)
        }

        /// An empty, shaped, unclaimed directory — the shape the predicate
        /// clears.
        fn candidate(&self, workspace: &str, name: &str) -> PathBuf {
            let path = self.base().join(workspace).join(name);
            std::fs::create_dir_all(&path).expect("create fixture candidate");
            path
        }

        /// A shaped directory holding real content, which `ContainsCheckout`
        /// proves live.
        fn occupied(&self, workspace: &str, name: &str) -> PathBuf {
            let path = self.candidate(workspace, name).join("primary");
            std::fs::create_dir_all(&path).expect("create fixture checkout dir");
            std::fs::write(path.join("tracked.txt"), b"fixture\n").expect("write fixture content");
            self.base().join(workspace).join(name)
        }

        fn workspace(&self, workspace: &str) -> PathBuf {
            let path = self.base().join(workspace);
            std::fs::create_dir_all(&path).expect("create fixture workspace");
            path
        }

        fn write_state(&self, file: &str, state: RepositoryState) {
            let bytes = serde_json::to_vec_pretty(&baude_core::persist::StateFile::new(state))
                .expect("serialize fixture state");
            std::fs::write(self.config().join(file), bytes).expect("write fixture state file");
        }

        /// The readable, empty state file every clearing case needs: without a
        /// complete inventory no candidate is ever cleared.
        fn empty_state(&self) {
            self.write_state("state-claude.json", RepositoryState::default());
        }

        /// A state file recording repository `key`, which is an ownership claim
        /// on `<base>/claude/repository-<key>`.
        fn state_claiming(&self, key: u64) {
            let main = self.root.join("external").join("repo");
            std::fs::create_dir_all(&main).expect("create fixture external repo");
            let mut state = RepositoryState {
                next_repository_key: key,
                ..RepositoryState::default()
            };
            let allocated = state
                .allocate_repository_key()
                .expect("allocate fixture repository key");
            let order = state.next_first_seen_order;
            state.next_first_seen_order += 1;
            state.repositories.push(SavedRepository {
                key: allocated,
                observed_common_dir: PersistedPath::from_path(&main.join(".git")),
                observed_main_worktree: PersistedPath::from_path(&main),
                first_seen_order: order,
                health: RepositoryHealth::Available,
            });
            self.write_state("state-claude.json", state);
        }

        /// A sorted, recursive listing of both scan roots: every entry with its
        /// kind and size. Two of these bracket every non-destructive variant.
        fn snapshot(&self) -> Vec<String> {
            let mut entries = Vec::new();
            listing(&self.base(), "worktrees/", &mut entries);
            listing(&self.config(), "config/", &mut entries);
            entries
        }
    }

    impl Drop for CliFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn listing(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        let mut items: Vec<_> = match std::fs::read_dir(dir) {
            Ok(read) => read.filter_map(Result::ok).collect(),
            Err(error) => {
                out.push(format!("{prefix}<unreadable: {error}>"));
                return;
            }
        };
        items.sort_by_key(std::fs::DirEntry::file_name);
        for item in items {
            let name = item.file_name().to_string_lossy().to_string();
            let path = item.path();
            let meta = std::fs::symlink_metadata(&path).expect("stat fixture entry");
            let kind = if meta.file_type().is_symlink() {
                "link"
            } else if meta.is_dir() {
                "dir"
            } else {
                "file"
            };
            out.push(format!("{prefix}{name} [{kind} {}]", meta.len()));
            if meta.is_dir() {
                listing(&path, &format!("{prefix}{name}/"), out);
            }
        }
    }

    /// Drive the subcommand exactly as `main` does, with explicit roots and
    /// captured sinks. `args` are the tokens that follow `baude worktrees`.
    fn run(fixture: &CliFixture, args: &[&str]) -> (i32, String, String) {
        let argv: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        let code = run_worktrees_at(&argv, &fixture.roots(), &mut out, &mut err);
        (
            code,
            String::from_utf8(out).expect("stdout is utf-8"),
            String::from_utf8(err).expect("stderr is utf-8"),
        )
    }

    /// Save a preview the way the documented flow tells an operator to: capture
    /// `--json` stdout into a file outside both roots.
    fn save_preview(fixture: &CliFixture, name: &str) -> PathBuf {
        let (code, stdout, _) = run(fixture, &["scan", "--json"]);
        assert_eq!(code, WORKTREES_EXIT_OK, "a json scan must succeed");
        let path = fixture.report_path(name);
        std::fs::write(&path, stdout.as_bytes()).expect("save the preview");
        path
    }

    fn report_at(path: &Path) -> ScanReport {
        let bytes = std::fs::read(path).expect("read the saved preview");
        serde_json::from_slice(&bytes).expect("the saved preview parses as a core ScanReport")
    }

    // ---- the read-only contract ------------------------------------------

    #[test]
    fn a_plain_scan_prints_a_grouped_summary_and_changes_neither_root() {
        let fixture = CliFixture::new("plain-scan");
        fixture.empty_state();
        fixture.candidate("claude", "repository-9");
        fixture.occupied("claude", "repository-5");

        let before = fixture.snapshot();
        let (code, stdout, stderr) = run(&fixture, &["scan"]);
        let after = fixture.snapshot();

        assert_eq!(code, WORKTREES_EXIT_OK, "stderr: {stderr}");
        assert_eq!(before, after, "a scan must change neither root");
        assert!(
            stdout.contains("workspace claude"),
            "the summary groups by workspace: {stdout}"
        );
        assert!(
            stdout.contains("claude/repository-9") && stdout.contains("claude/repository-5"),
            "every candidate is named: {stdout}"
        );
        assert!(
            stdout.contains("removable") && stdout.contains("live"),
            "verdicts are grouped and counted: {stdout}"
        );
        assert!(
            stdout.contains("total: 2 candidate"),
            "the summary carries a total: {stdout}"
        );
        assert!(
            stdout.to_lowercase().contains("nothing was removed"),
            "the preview says so in words: {stdout}"
        );
    }

    #[test]
    fn a_json_scan_changes_neither_root_and_describes_the_same_candidate_set() {
        let fixture = CliFixture::new("json-scan");
        fixture.empty_state();
        fixture.candidate("claude", "repository-9");
        fixture.occupied("claude", "repository-5");

        let (_, plain, _) = run(&fixture, &["scan"]);
        let before = fixture.snapshot();
        let (code, stdout, stderr) = run(&fixture, &["scan", "--json"]);
        let after = fixture.snapshot();

        assert_eq!(code, WORKTREES_EXIT_OK, "stderr: {stderr}");
        assert_eq!(before, after, "a json scan must change neither root");
        let report: ScanReport =
            serde_json::from_str(&stdout).expect("stdout is exactly one core ScanReport");
        assert_eq!(report.candidates.len(), 2);
        for candidate in &report.candidates {
            assert!(
                plain.contains(&candidate.relative.join("/")),
                "both modes describe the same set: {plain}"
            );
        }
    }

    #[test]
    fn json_output_carries_diagnostics_on_stderr_only() {
        let fixture = CliFixture::new("json-clean");
        fixture.empty_state();
        fixture.candidate("claude", "repository-9");

        let (code, stdout, _) = run(&fixture, &["scan", "--json"]);

        assert_eq!(code, WORKTREES_EXIT_OK);
        assert!(
            stdout.trim_start().starts_with('{'),
            "stdout must be machine-readable from the first byte: {stdout}"
        );
        serde_json::from_str::<ScanReport>(&stdout)
            .expect("nothing but the report may reach stdout in json mode");
    }

    #[test]
    fn saved_json_round_trips_into_the_core_scan_report() {
        let fixture = CliFixture::new("round-trip");
        fixture.empty_state();
        fixture.candidate("claude", "repository-9");
        fixture.occupied("claude", "repository-5");

        let saved = save_preview(&fixture, "preview.json");
        let report = report_at(&saved);

        assert_eq!(report.format_version, REPORT_FORMAT_VERSION);
        assert_eq!(report.candidates.len(), 2);
        assert!(
            !report.state_inventory.workspaces_checked.is_empty(),
            "the saved report carries the inventory the verdicts rest on"
        );
        let removable = report
            .candidates
            .iter()
            .find(|candidate| candidate.relative == ["claude", "repository-9"])
            .expect("the empty candidate is in the saved report");
        assert!(
            matches!(
                removable.verdict,
                baude_core::worktree_scan::Verdict::Removable { .. }
            ),
            "the saved report carries the full verdict and its proof: {removable:?}"
        );
    }

    // ---- the two-invocation flow -----------------------------------------

    #[test]
    fn prune_acts_on_the_saved_report_rather_than_a_fresh_scan() {
        let fixture = CliFixture::new("saved-set");
        fixture.empty_state();
        let approved = fixture.candidate("claude", "repository-9");
        let saved = save_preview(&fixture, "preview.json");

        // Created AFTER the preview. A fresh scan would clear it; the approved
        // report does not name it, so it is not this run's to remove.
        let latecomer = fixture.candidate("claude", "repository-7");

        let (code, stdout, stderr) = run(
            &fixture,
            &[
                "scan",
                "--prune",
                "--report",
                saved.to_str().unwrap(),
                "--yes",
            ],
        );

        assert_eq!(code, WORKTREES_EXIT_OK, "stderr: {stderr}");
        assert!(!approved.exists(), "the approved candidate is removed");
        assert!(
            latecomer.exists(),
            "a candidate created after the preview is never pruned"
        );
        assert!(
            stdout.contains("claude/repository-7"),
            "the account is complete — the latecomer is reported: {stdout}"
        );
    }

    #[test]
    fn an_unchanged_previewed_candidate_is_removed_only_with_all_three_opt_ins() {
        let fixture = CliFixture::new("opt-ins");
        fixture.empty_state();
        let approved = fixture.candidate("claude", "repository-9");
        let saved = save_preview(&fixture, "preview.json");
        let path = saved.to_str().unwrap().to_string();

        let (code, stdout, stderr) = run(&fixture, &["scan", "--prune", "--report", &path]);
        assert_eq!(code, WORKTREES_EXIT_OK, "stderr: {stderr}");
        assert!(
            approved.exists(),
            "--prune without --yes re-verifies and removes nothing"
        );
        assert!(
            stdout.contains("would remove"),
            "the withheld path says what it would have done: {stdout}"
        );

        let (code, stdout, stderr) =
            run(&fixture, &["scan", "--prune", "--report", &path, "--yes"]);
        assert_eq!(code, WORKTREES_EXIT_OK, "stderr: {stderr}");
        assert!(!approved.exists(), "both opt-ins together remove it");
        assert!(
            stdout.contains("removed"),
            "the account names the removal: {stdout}"
        );
    }

    #[test]
    fn prune_without_yes_re_verifies_and_removes_nothing() {
        let fixture = CliFixture::new("no-yes");
        fixture.empty_state();
        fixture.candidate("claude", "repository-9");
        let saved = save_preview(&fixture, "preview.json");

        let before = fixture.snapshot();
        let (code, stdout, stderr) = run(
            &fixture,
            &["scan", "--prune", "--report", saved.to_str().unwrap()],
        );
        let after = fixture.snapshot();

        assert_eq!(code, WORKTREES_EXIT_OK, "stderr: {stderr}");
        assert_eq!(before, after, "re-verification alone changes nothing");
        assert!(
            stdout.contains("claude/repository-9"),
            "the re-verification result is reported: {stdout}"
        );
    }

    // ---- refusals, each naming its reason ---------------------------------

    #[test]
    fn a_candidate_whose_proof_changed_is_refused_and_names_the_reason() {
        let fixture = CliFixture::new("proof-changed");
        fixture.empty_state();
        let approved = fixture.candidate("claude", "repository-9");
        let saved = save_preview(&fixture, "preview.json");

        // A second workspace directory widens `workspaces_checked`, so the
        // candidate still clears — but not by the facts the operator approved.
        fixture.workspace("opencode");

        let (code, stdout, stderr) = run(
            &fixture,
            &[
                "scan",
                "--prune",
                "--report",
                saved.to_str().unwrap(),
                "--yes",
            ],
        );

        assert_eq!(code, WORKTREES_EXIT_OK, "stderr: {stderr}");
        assert!(approved.exists(), "a changed proof is not the approved one");
        assert!(
            stdout.contains("refused") && stdout.contains("proof"),
            "the refusal names the reason: {stdout}"
        );
    }

    #[test]
    fn a_candidate_newly_referenced_by_state_is_refused_and_names_the_reason() {
        let fixture = CliFixture::new("new-reference");
        fixture.empty_state();
        let approved = fixture.candidate("claude", "repository-9");
        let saved = save_preview(&fixture, "preview.json");

        // The operator re-admitted the repository between preview and prune.
        fixture.state_claiming(9);

        let (code, stdout, stderr) = run(
            &fixture,
            &[
                "scan",
                "--prune",
                "--report",
                saved.to_str().unwrap(),
                "--yes",
            ],
        );

        assert_eq!(code, WORKTREES_EXIT_OK, "stderr: {stderr}");
        assert!(approved.exists(), "a live candidate is never removed");
        assert!(
            stdout.contains("refused") && stdout.to_lowercase().contains("referenced by state"),
            "the refusal names the blocker: {stdout}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_candidate_replaced_by_a_symlink_is_refused_and_names_the_reason() {
        let fixture = CliFixture::new("symlink");
        fixture.empty_state();
        let approved = fixture.candidate("claude", "repository-9");
        let saved = save_preview(&fixture, "preview.json");

        let target = fixture.root.join("external").join("elsewhere");
        std::fs::create_dir_all(&target).expect("create the symlink target");
        std::fs::remove_dir(&approved).expect("clear the approved candidate");
        std::os::unix::fs::symlink(&target, &approved).expect("replace it with a symlink");

        let (code, stdout, stderr) = run(
            &fixture,
            &[
                "scan",
                "--prune",
                "--report",
                saved.to_str().unwrap(),
                "--yes",
            ],
        );

        assert_eq!(code, WORKTREES_EXIT_OK, "stderr: {stderr}");
        assert!(target.exists(), "the symlink target is untouched");
        assert!(
            std::fs::symlink_metadata(&approved)
                .expect("the link is still there")
                .file_type()
                .is_symlink(),
            "a symlink is refused without being resolved"
        );
        assert!(
            stdout.contains("refused") && stdout.to_lowercase().contains("symlink"),
            "the refusal names the reason: {stdout}"
        );
    }

    // ---- report input that cannot authorize anything ----------------------

    #[test]
    fn a_missing_report_file_removes_nothing() {
        let fixture = CliFixture::new("missing-report");
        fixture.empty_state();
        fixture.candidate("claude", "repository-9");
        let absent = fixture.report_path("never-written.json");

        let before = fixture.snapshot();
        let (code, _, stderr) = run(
            &fixture,
            &[
                "scan",
                "--prune",
                "--report",
                absent.to_str().unwrap(),
                "--yes",
            ],
        );
        let after = fixture.snapshot();

        assert_ne!(code, WORKTREES_EXIT_OK, "a missing report exits nonzero");
        assert_eq!(before, after, "nothing was removed");
        assert!(!stderr.is_empty(), "the failure is explained");
    }

    #[test]
    fn a_malformed_report_removes_nothing() {
        let fixture = CliFixture::new("malformed-report");
        fixture.empty_state();
        fixture.candidate("claude", "repository-9");
        let path = fixture.report_path("garbage.json");
        std::fs::write(&path, b"{not a report").expect("write a malformed report");

        let before = fixture.snapshot();
        let (code, _, stderr) = run(
            &fixture,
            &[
                "scan",
                "--prune",
                "--report",
                path.to_str().unwrap(),
                "--yes",
            ],
        );
        let after = fixture.snapshot();

        assert_ne!(code, WORKTREES_EXIT_OK, "a malformed report exits nonzero");
        assert_eq!(before, after, "nothing was removed");
        assert!(!stderr.is_empty(), "the failure is explained");
    }

    #[test]
    fn a_report_bound_to_a_foreign_root_removes_nothing() {
        let mine = CliFixture::new("foreign-mine");
        mine.empty_state();
        mine.candidate("claude", "repository-9");

        let theirs = CliFixture::new("foreign-theirs");
        theirs.empty_state();
        theirs.candidate("claude", "repository-9");
        let foreign = save_preview(&theirs, "preview.json");

        let before = mine.snapshot();
        let (code, _, stderr) = run(
            &mine,
            &[
                "scan",
                "--prune",
                "--report",
                foreign.to_str().unwrap(),
                "--yes",
            ],
        );
        let after = mine.snapshot();

        assert_ne!(code, WORKTREES_EXIT_OK, "a foreign report exits nonzero");
        assert_eq!(before, after, "nothing was removed");
        assert!(!stderr.is_empty(), "the failure is explained");
    }

    // ---- usage: a single mistyped argument cannot authorize a deletion -----

    #[test]
    fn yes_without_prune_is_a_usage_error() {
        let fixture = CliFixture::new("yes-alone");
        fixture.empty_state();
        fixture.candidate("claude", "repository-9");

        let before = fixture.snapshot();
        let (code, _, stderr) = run(&fixture, &["scan", "--yes"]);
        let after = fixture.snapshot();

        assert_eq!(code, WORKTREES_EXIT_USAGE, "stderr: {stderr}");
        assert_eq!(before, after, "nothing was removed");
    }

    #[test]
    fn report_without_prune_is_a_usage_error() {
        let fixture = CliFixture::new("report-alone");
        fixture.empty_state();
        fixture.candidate("claude", "repository-9");
        let saved = save_preview(&fixture, "preview.json");

        let (code, _, stderr) = run(&fixture, &["scan", "--report", saved.to_str().unwrap()]);

        assert_eq!(
            code, WORKTREES_EXIT_USAGE,
            "report input must never be confused with a new scan: {stderr}"
        );
    }

    #[test]
    fn prune_without_report_is_a_usage_error() {
        let fixture = CliFixture::new("prune-alone");
        fixture.empty_state();
        fixture.candidate("claude", "repository-9");

        let before = fixture.snapshot();
        let (code, _, stderr) = run(&fixture, &["scan", "--prune", "--yes"]);
        let after = fixture.snapshot();

        assert_eq!(code, WORKTREES_EXIT_USAGE, "stderr: {stderr}");
        assert_eq!(before, after, "nothing was removed");
    }

    #[test]
    fn an_unknown_option_is_a_usage_error() {
        let fixture = CliFixture::new("unknown-option");
        let (code, _, stderr) = run(&fixture, &["scan", "--force"]);
        assert_eq!(code, WORKTREES_EXIT_USAGE);
        assert!(
            stderr.contains("--force"),
            "the bad option is named: {stderr}"
        );
    }

    #[test]
    fn a_duplicate_option_is_a_usage_error() {
        let fixture = CliFixture::new("duplicate-option");
        let (code, _, stderr) = run(&fixture, &["scan", "--json", "--json"]);
        assert_eq!(code, WORKTREES_EXIT_USAGE, "stderr: {stderr}");
    }

    #[test]
    fn a_missing_option_value_is_a_usage_error() {
        let fixture = CliFixture::new("missing-value");
        let (code, _, stderr) = run(&fixture, &["scan", "--prune", "--report"]);
        assert_eq!(code, WORKTREES_EXIT_USAGE, "stderr: {stderr}");
    }

    #[test]
    fn an_unknown_verb_is_a_usage_error() {
        let fixture = CliFixture::new("unknown-verb");
        let (code, _, stderr) = run(&fixture, &["prune"]);
        assert_eq!(
            code, WORKTREES_EXIT_USAGE,
            "`prune` is an option, never a verb: {stderr}"
        );
    }

    // ---- discoverability --------------------------------------------------

    #[test]
    fn the_top_level_help_lists_the_worktrees_verb() {
        let help = help_text();
        assert!(
            help.contains("worktrees"),
            "a verb that dispatches but is undiscoverable is half-shipped: {help}"
        );
    }

    #[test]
    fn the_worktrees_help_documents_the_two_invocation_flow() {
        let fixture = CliFixture::new("verb-help");
        let (code, stdout, _) = run(&fixture, &["--help"]);

        assert_eq!(code, WORKTREES_EXIT_OK);
        assert!(stdout.contains("--json"), "flow step one: {stdout}");
        assert!(stdout.contains("--report"), "flow step two: {stdout}");
        assert!(
            stdout.contains("--yes"),
            "the separate confirmation: {stdout}"
        );
    }
}
