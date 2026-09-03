# Graph Report - .  (2026-08-30)

## Corpus Check
- 147 files · ~181,020 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1328 nodes · 2713 edges · 69 communities (61 shown, 8 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 86 edges (avg confidence: 0.84)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]
- [[_COMMUNITY_Community 6|Community 6]]
- [[_COMMUNITY_Community 7|Community 7]]
- [[_COMMUNITY_Community 8|Community 8]]
- [[_COMMUNITY_Community 9|Community 9]]
- [[_COMMUNITY_Community 10|Community 10]]
- [[_COMMUNITY_Community 11|Community 11]]
- [[_COMMUNITY_Community 12|Community 12]]
- [[_COMMUNITY_Community 13|Community 13]]
- [[_COMMUNITY_Community 14|Community 14]]
- [[_COMMUNITY_Community 15|Community 15]]
- [[_COMMUNITY_Community 16|Community 16]]
- [[_COMMUNITY_Community 17|Community 17]]
- [[_COMMUNITY_Community 18|Community 18]]
- [[_COMMUNITY_Community 19|Community 19]]
- [[_COMMUNITY_Community 20|Community 20]]
- [[_COMMUNITY_Community 21|Community 21]]
- [[_COMMUNITY_Community 22|Community 22]]
- [[_COMMUNITY_Community 23|Community 23]]
- [[_COMMUNITY_Community 24|Community 24]]
- [[_COMMUNITY_Community 25|Community 25]]
- [[_COMMUNITY_Community 26|Community 26]]
- [[_COMMUNITY_Community 27|Community 27]]
- [[_COMMUNITY_Community 28|Community 28]]
- [[_COMMUNITY_Community 29|Community 29]]
- [[_COMMUNITY_Community 30|Community 30]]
- [[_COMMUNITY_Community 31|Community 31]]
- [[_COMMUNITY_Community 32|Community 32]]
- [[_COMMUNITY_Community 33|Community 33]]
- [[_COMMUNITY_Community 34|Community 34]]
- [[_COMMUNITY_Community 35|Community 35]]
- [[_COMMUNITY_Community 36|Community 36]]
- [[_COMMUNITY_Community 37|Community 37]]
- [[_COMMUNITY_Community 38|Community 38]]
- [[_COMMUNITY_Community 39|Community 39]]
- [[_COMMUNITY_Community 40|Community 40]]
- [[_COMMUNITY_Community 41|Community 41]]
- [[_COMMUNITY_Community 42|Community 42]]
- [[_COMMUNITY_Community 43|Community 43]]
- [[_COMMUNITY_Community 44|Community 44]]
- [[_COMMUNITY_Community 45|Community 45]]
- [[_COMMUNITY_Community 46|Community 46]]
- [[_COMMUNITY_Community 47|Community 47]]
- [[_COMMUNITY_Community 48|Community 48]]
- [[_COMMUNITY_Community 49|Community 49]]
- [[_COMMUNITY_Community 50|Community 50]]
- [[_COMMUNITY_Community 51|Community 51]]
- [[_COMMUNITY_Community 52|Community 52]]
- [[_COMMUNITY_Community 53|Community 53]]
- [[_COMMUNITY_Community 54|Community 54]]
- [[_COMMUNITY_Community 55|Community 55]]
- [[_COMMUNITY_Community 56|Community 56]]
- [[_COMMUNITY_Community 57|Community 57]]
- [[_COMMUNITY_Community 58|Community 58]]
- [[_COMMUNITY_Community 59|Community 59]]
- [[_COMMUNITY_Community 62|Community 62]]
- [[_COMMUNITY_Community 63|Community 63]]
- [[_COMMUNITY_Community 64|Community 64]]
- [[_COMMUNITY_Community 65|Community 65]]
- [[_COMMUNITY_Community 66|Community 66]]

## God Nodes (most connected - your core abstractions)
1. `App` - 70 edges
2. `Manager` - 41 edges
3. `ClaudeMeta` - 25 edges
4. `Result` - 24 edges
5. `Shared` - 22 edges
6. `mgr()` - 22 edges
7. `Pty` - 21 edges
8. `ApiError` - 20 edges
9. `State` - 20 edges
10. `Result` - 19 edges

## Surprising Connections (you probably didn't know these)
- `PWA Permission Card Plan` --semantically_similar_to--> `Activity Feed XSS Escaping`  [INFERRED] [semantically similar]
  04-remote-permission-approval/04-04-PLAN.md → 03-tool-activity-timeline/03-03-SUMMARY.md
- `route_event()` --calls--> `post()`  [INFERRED]
  baude-core/src/hook.rs → baude/src/notify_desktop.rs
- `handle_ask()` --calls--> `now_unix_ms()`  [INFERRED]
  bauded/src/permission_bridge.rs → baude-core/src/meta.rs
- `activity_age()` --calls--> `now_unix_ms()`  [INFERRED]
  baude/src/ui.rs → baude-core/src/meta.rs
- `session_row()` --calls--> `human_duration()`  [INFERRED]
  baude/src/ui.rs → baude-core/src/session.rs

## Import Cycles
- 1-file cycle: `baude-core/src/backend/claude.rs -> baude-core/src/backend/claude.rs`
- 1-file cycle: `baude-core/src/backend/opencode.rs -> baude-core/src/backend/opencode.rs`
- 1-file cycle: `bauded/src/api.rs -> bauded/src/api.rs`
- 1-file cycle: `baude-core/src/git.rs -> baude-core/src/git.rs`
- 1-file cycle: `baude-core/src/pty.rs -> baude-core/src/pty.rs`
- 1-file cycle: `baude-core/src/meta.rs -> baude-core/src/meta.rs`
- 1-file cycle: `baude-core/src/permission.rs -> baude-core/src/permission.rs`
- 1-file cycle: `baude-core/src/persist.rs -> baude-core/src/persist.rs`
- 1-file cycle: `bauded/src/transcript.rs -> bauded/src/transcript.rs`
- 1-file cycle: `baude-core/src/session.rs -> baude-core/src/session.rs`
- 1-file cycle: `baude/src/app.rs -> baude/src/app.rs`
- 1-file cycle: `bauded/src/manager.rs -> bauded/src/manager.rs`
- 1-file cycle: `baude-core/src/workspace.rs -> baude-core/src/workspace.rs`
- 1-file cycle: `baude/src/main.rs -> baude/src/main.rs`
- 1-file cycle: `baude/src/remote.rs -> baude/src/remote.rs`
- 1-file cycle: `baude/src/ui.rs -> baude/src/ui.rs`
- 1-file cycle: `bauded/src/main.rs -> bauded/src/main.rs`
- 1-file cycle: `bauded/src/permission_bridge.rs -> bauded/src/permission_bridge.rs`
- 1-file cycle: `bauded/src/notify.rs -> bauded/src/notify.rs`
- 1-file cycle: `bauded/src/push.rs -> bauded/src/push.rs`

## Hyperedges (group relationships)
- **Tiered Release Automation Flow** — workflows_release_please_release_please_automation, workflows_release_please_tiered_release_policy, workflows_release_automerge_minor_release_soak_gate, workflows_release_release_asset_pipeline, workflows_ci_required_checks [EXTRACTED 1.00]
- **Phase 1 Status-line Data Flow** — 01_full_status_line_capture_01_01_summary_schema_2_bridge_writer, 01_full_status_line_capture_01_02_summary_additive_claudemeta_reader, 01_full_status_line_capture_01_03_summary_conditional_metadata_rows [EXTRACTED 1.00]
- **baude Multi-surface Session Orchestration** — planning_project_baude_platform, planning_project_multi_surface_architecture, planning_project_session_attention_core_value, planning_project_vpn_only_security_model [EXTRACTED 1.00]
- **Hook Event State Pipeline** — 02_hook_driven_status_02_01_plan_hook_event_schema, 02_hook_driven_status_02_01_summary_dual_transport_runtime_selection, 02_hook_driven_status_02_02_plan_offset_tracked_event_tail, 02_hook_driven_status_02_02_plan_state_source_precedence [EXTRACTED 1.00]
- **Dual Transport Convergence** — 02_hook_driven_status_02_01_summary_dual_transport_runtime_selection, 02_hook_driven_status_02_03_plan_daemon_hook_environment_injection, 02_hook_driven_status_02_03_plan_daemon_event_ingest_endpoint, 02_hook_driven_status_02_02_plan_offset_tracked_event_tail [EXTRACTED 1.00]
- **Activity Timeline Delivery Flow** — 03_tool_activity_timeline_03_01_plan_hookevent, 03_tool_activity_timeline_03_01_plan_activity_ring_buffer, 03_tool_activity_timeline_03_02_plan_activity_snapshot_api, 03_tool_activity_timeline_03_02_plan_activity_sse_channel, 03_tool_activity_timeline_03_03_plan_pwa_activity_strip [EXTRACTED 1.00]
- **Phase 3 Activity Delivery Flow** — 03_tool_activity_timeline_03_context_hookevent_model, 03_tool_activity_timeline_03_context_standalone_activity_sse, 03_tool_activity_timeline_03_03_summary_pwa_activity_strip, 03_tool_activity_timeline_03_04_summary_tui_activity_overlay [EXTRACTED 1.00]
- **Phase 4 Remote Approval Flow** — 04_remote_permission_approval_04_01_plan_permission_mode_selector, 04_remote_permission_approval_04_02_plan_permission_mcp_bridge, 04_remote_permission_approval_04_02_plan_daemon_pending_permission_api, 04_remote_permission_approval_04_03_plan_distinct_permission_push, 04_remote_permission_approval_04_04_plan_pwa_permission_card [EXTRACTED 1.00]
- **Permission Fail-Safe Controls** — 04_remote_permission_approval_04_01_plan_permission_mode_selector, 04_remote_permission_approval_04_context_deny_on_timeout, 04_remote_permission_approval_04_04_plan_single_call_decision_semantics [EXTRACTED 1.00]
- **Remote Permission Approval Flow** — 04_remote_permission_approval_04_verification_permission_mode, 04_remote_permission_approval_04_verification_pending_permission_api, 04_remote_permission_approval_04_verification_permission_card, 04_remote_permission_approval_04_verification_distinct_permission_push, 04_remote_permission_approval_04_verification_fail_closed_policy [EXTRACTED 1.00]
- **opencode Permission Reply Flow** — 001_a_permission_reply_server_api_readme_permission_asked_event, 001_a_permission_reply_server_api_readme_permission_reply_endpoint, 001_a_permission_reply_server_api_readme_deferred_tool_execution, 001_a_permission_reply_server_api_readme_server_api_permission_reply [EXTRACTED 1.00]
- **Multi-Backend Workspace Architecture** — readme_baude_tui, readme_claude_code_backend, readme_opencode_backend, readme_workspace_isolation, readme_bauded_remote_daemon [EXTRACTED 1.00]
- **Native Session Observability Pipeline** — plans_tier_1_native_claude_integration_full_status_line_capture, plans_tier_1_native_claude_integration_hook_driven_status_events, plans_tier_1_native_claude_integration_tool_activity_timeline, plans_tier_1_native_claude_integration_remote_permission_approval [EXTRACTED 1.00]
- **Cross-Frontend Diff Review** — plans_tier_2_diff_review_loop_git_read_surface, plans_tier_2_diff_review_loop_unified_diff_parser, plans_tier_2_diff_review_loop_pwa_diff_viewer, plans_tier_2_diff_review_loop_tui_diff_viewer, plans_tier_2_diff_review_loop_inline_review_comments [EXTRACTED 1.00]
- **Remote Session Stack** — docs_deploy_tailscale_sidecar, docs_remote_daemon_plan_baude_core, docs_remote_daemon_plan_bauded, docs_remote_daemon_plan_rest_sse_api, docs_remote_daemon_plan_phone_pwa [EXTRACTED 1.00]
- **Geometric Brand Mark** — web_apple_touch_icon_baude_app_icon, web_apple_touch_icon_cyan_angular_form, web_apple_touch_icon_yellow_square, web_apple_touch_icon_dark_background [EXTRACTED 1.00]
- **Geometric Icon Composition** — web_icon_192_cyan_right_angle, web_icon_192_yellow_square, web_icon_192_dark_background [INFERRED 0.95]
- **Abstract Geometric Composition** — web_icon_512_cyan_angular_frame, web_icon_512_yellow_square, web_icon_512_dark_background [EXTRACTED 1.00]
- **Token Refresh Failure Diagnosis** — img_pwa_chat_auth_test_flakiness, img_pwa_chat_mock_token_fixed_expiry, img_pwa_chat_jest_fake_timers, img_pwa_chat_retry_timing, img_pwa_chat_third_retry_deadline_crossing [EXTRACTED 1.00]
- **In-Conversation Debugging Flow** — img_pwa_chat_api_chat_session, img_pwa_chat_refresh_test_file, img_pwa_chat_serial_auth_refresh_test_run, img_pwa_chat_pin_expiry_to_fake_clock [INFERRED 0.85]
- **Visible Job Queue Entries** — img_pwa_list_api_job, img_pwa_list_webapp_job, img_pwa_list_infra_job [EXTRACTED 1.00]

## Communities (69 total, 8 thin omitted)

### Community 0 - "Community 0"
Cohesion: 0.07
Nodes (57): Arc, HashMap, HookEvent, MutexGuard, Option, PathBuf, Receiver, Result (+49 more)

### Community 1 - "Community 1"
Cohesion: 0.07
Nodes (35): Config, Event, KeyEvent, Option, PathBuf, RateWindow, Receiver, Rect (+27 more)

### Community 2 - "Community 2"
Cohesion: 0.09
Nodes (78): ApiError, Error, Event, HookEvent, Option, Path, Result, Screenshot (+70 more)

### Community 3 - "Community 3"
Cohesion: 0.09
Nodes (58): App, HookEvent, Mutex, Option, Parser, Path, RateWindow, Rect (+50 more)

### Community 4 - "Community 4"
Cohesion: 0.09
Nodes (44): Option, Path, PathBuf, String, Value, activity_ring_caps_drop_oldest(), activity_ring_clears_on_path_rotation(), activity_ring_records_each_event_in_order() (+36 more)

### Community 5 - "Community 5"
Cohesion: 0.08
Nodes (49): activityAge(), activityIcon(), activityLabel(), activityRowHtml(), addMsg(), api(), $app, approve() (+41 more)

### Community 6 - "Community 6"
Cohesion: 0.05
Nodes (52): Activity EventSource Lifecycle, Activity Feed XSS Escaping, PWA Activity Strip, Snapshot-to-Tail Buffering, RemoteInfo Activity Transport, TUI Activity Overlay Plan, Render Last N Activity Events, Implemented TUI Activity Overlay (+44 more)

### Community 7 - "Community 7"
Cohesion: 0.05
Nodes (49): Deferred Tool Execution, Live Wire Schema Drift, permission.asked SSE Event, Permission Reply HTTP Endpoint, opencode Server API Permission Reply, Generic Plugin Event Hook, permission.ask Plugin Hook, Server API Over Plugin Decision (+41 more)

### Community 8 - "Community 8"
Cohesion: 0.06
Nodes (43): Authoritative Status-Line Bridge, Bridge Model Precedence, Stale Bridge Field Clearing, Local Info Overlay UAT, Phase 1 Validation Strategy, Full Status-Line Capture, Normalized Hook Event Schema, Hook Foundation (+35 more)

### Community 9 - "Community 9"
Cohesion: 0.09
Nodes (23): ClaudeMeta, Option, PathBuf, Result, String, Value, Pty, decide_live() (+15 more)

### Community 10 - "Community 10"
Cohesion: 0.11
Nodes (24): alloc_port(), apply_session(), apply_session_fills_meta(), apply_status(), apply_status_stamps_only_on_transition(), compose_spawn_cmd(), fixture(), is_busy() (+16 more)

### Community 11 - "Community 11"
Cohesion: 0.13
Nodes (32): F, Option, Path, Result, String, Value, append_event(), append_event_appends_two_lines() (+24 more)

### Community 12 - "Community 12"
Cohesion: 0.09
Nodes (22): Path, PathBuf, dispatch_initialize(), dispatch_rpc(), dispatch_tools_call_allow_echoes_input(), dispatch_tools_call_deny(), dispatch_tools_call_unknown_decision_coerces_to_deny(), dispatch_tools_list_has_one_approve_tool() (+14 more)

### Community 13 - "Community 13"
Cohesion: 0.11
Nodes (18): Arc, AtomicBool, Error, HookEvent, Mutex, Option, Parser, Result (+10 more)

### Community 14 - "Community 14"
Cohesion: 0.12
Nodes (24): HookEvent, Option, Path, String, Value, Vec, after(), after_cursor() (+16 more)

### Community 15 - "Community 15"
Cohesion: 0.16
Nodes (20): MutexGuard, PathBuf, Result, SharedPush, String, Vec, SecretKey, config_base() (+12 more)

### Community 16 - "Community 16"
Cohesion: 0.10
Nodes (19): AtomicU64, Arc, AtomicBool, Mutex, Option, Parser, Path, Receiver (+11 more)

### Community 17 - "Community 17"
Cohesion: 0.17
Nodes (22): HashMap, Option, PathBuf, Result, String, Vec, WorkspaceConfig, SavedSession (+14 more)

### Community 18 - "Community 18"
Cohesion: 0.18
Nodes (21): Option, Path, PathBuf, Result, String, browser_url_extra_segments_and_query(), clone_repo(), CloneTarget (+13 more)

### Community 19 - "Community 19"
Cohesion: 0.18
Nodes (19): Backend, Config, Option, String, WorkspaceConfig, FnMut, active(), backend_env_lands_in_implicit_workspace() (+11 more)

### Community 20 - "Community 20"
Cohesion: 0.14
Nodes (11): ClaudeBackend, permission_mode_default_skip_and_prompt_at_spawn_plan(), seed_mcp_config(), seed_mcp_config_is_non_clobbering(), spawn_plan_exports_event_url_on_both_paths(), Backend, ClaudeMeta, Option (+3 more)

### Community 21 - "Community 21"
Cohesion: 0.20
Nodes (17): HashMap, HashSet, Option, SessionInfo, String, Vec, archived_sessions_are_muted(), completed_fires_gentle_finished_once_on_edge_from_busy() (+9 more)

### Community 22 - "Community 22"
Cohesion: 0.12
Nodes (20): Status-line Bridge Writer Plan, Schema 2 Bridge Writer, Additive ClaudeMeta Reader Plan, Additive ClaudeMeta Reader, Info Overlay Extension Plan, Conditional Metadata Rows, In-file Analog Extension Pattern, Value-accessor Compatibility Pattern (+12 more)

### Community 23 - "Community 23"
Cohesion: 0.28
Nodes (15): Option, String, Value, bridge_path(), build_bridge(), full_payload_captured(), minimal_payload_ok(), model_falls_back_to_id() (+7 more)

### Community 24 - "Community 24"
Cohesion: 0.17
Nodes (11): api(), events, HERE, LOG, pluginLogEntries(), round(), SANDBOX, serve (+3 more)

### Community 25 - "Community 25"
Cohesion: 0.20
Nodes (11): active(), Backend, backend_for(), command_for(), resolve_command(), SpawnPlan, Config, Option (+3 more)

### Community 26 - "Community 26"
Cohesion: 0.24
Nodes (14): App, Config, Option, Result, main(), run_hook(), String, CrosstermBackend (+6 more)

### Community 27 - "Community 27"
Cohesion: 0.27
Nodes (13): HashMap, HashSet, Option, String, Vec, archived_is_muted_and_unarchive_does_not_false_fire(), Banner, completed_only_on_busy_edge_and_exited_only_if_seen_alive() (+5 more)

### Community 28 - "Community 28"
Cohesion: 0.15
Nodes (14): Release Event Build Handoff, Tiered Auto-release Gating Design, Workspace Version Updater Validation Risk, Cross-platform CI Pipeline, Daemon Image Smoke Test, Required Release Checks, Minor Release Soak Gate, Cross-platform Binary Tarballs (+6 more)

### Community 29 - "Community 29"
Cohesion: 0.18
Nodes (9): api(), events, HERE, LOG, round(), SANDBOX, serve, waiters (+1 more)

### Community 30 - "Community 30"
Cohesion: 0.26
Nodes (10): Agent, HashSet, Mutex, Shared, Value, decision_to_reply(), handle_ask(), watch_if_needed() (+2 more)

### Community 31 - "Community 31"
Cohesion: 0.27
Nodes (10): Arc, Mutex, Option, String, fetch(), human_cost(), local_today(), total_cost() (+2 more)

### Community 32 - "Community 32"
Cohesion: 0.19
Nodes (13): API Chat Session, Authentication Test Flakiness, Inline Tool Activity, Jest Fake Timers, Mobile PWA Chat Interface, Mock Clock, Mock Token Fixed 30-Second Expiry, Pin Token Expiry to Fake Clock (+5 more)

### Community 33 - "Community 33"
Cohesion: 0.15
Nodes (12): models, name, npm, options, model, gpt-oss-120b, Laguna-XS-2.1, baseURL (+4 more)

### Community 34 - "Community 34"
Cohesion: 0.22
Nodes (11): Option, find_separator(), is_prompt_mode(), parse_content_length_frame(), parse_frame(), parse_frame_consumes_only_one_frame(), parse_frame_content_length(), parse_frame_content_length_lf_only_separator() (+3 more)

### Community 35 - "Community 35"
Cohesion: 0.20
Nodes (11): ResolvedCmd, String, never_resolve(), resolve_claude_cmd(), resolve_claude_cmd_bl04_prompt_strips_conflicting_skip(), resolve_claude_cmd_env(), resolve_claude_cmd_never_both_flags(), resolve_claude_cmd_no_double_add() (+3 more)

### Community 36 - "Community 36"
Cohesion: 0.18
Nodes (11): PWA Push Notifications, Session Restore Across Updates, Shared Network Namespace, Tailscale Serve HTTPS, baude-core Session Engine, baude TUI, bauded Daemon, Phone PWA (+3 more)

### Community 37 - "Community 37"
Cohesion: 0.20
Nodes (11): Claude Hook Set, File-Tail Event Transport, Hook-Driven Status Events, BAUDE Permission Mode, PTY Silence Heuristic, Remote Permission Approval, Tool-Activity Timeline, Daemon State Persistence (+3 more)

### Community 38 - "Community 38"
Cohesion: 0.33
Nodes (9): run_permission_mcp(), Result, main(), run_hook(), run_permission_mcp(), shutdown_signal(), decide_with_timeout(), permission_timeout_s() (+1 more)

### Community 39 - "Community 39"
Cohesion: 0.20
Nodes (9): bump-minor-pre-major, changelog-path, draft, extra-files, include-component-in-tag, packages, prerelease, release-type (+1 more)

### Community 40 - "Community 40"
Cohesion: 0.25
Nodes (9): F, Result, R, run_permission_mcp(), run_permission_mcp_content_length_framed_input(), run_permission_mcp_full_session_over_mock_io(), run_permission_mcp_skips_notifications_and_eofs_clean(), write_frame() (+1 more)

### Community 41 - "Community 41"
Cohesion: 0.36
Nodes (9): Value, approve_tool_descriptor(), build_approve_result(), build_approve_result_allow_echoes_input(), build_approve_result_allow_without_input_uses_empty_object(), build_approve_result_deny(), build_approve_result_deny_custom_message(), build_approve_result_unknown_behavior_coerces_to_deny() (+1 more)

### Community 42 - "Community 42"
Cohesion: 0.25
Nodes (9): Integration-clean v0.7 Audit, First-party Session State Requirements, Native Integration Dependency Chain, Spawn Permission Mode Fallback, Status Capture Follow-ups, First-party Claude Data Preference, v0.7 Native Claude Integration, v0.7 Four-phase Delivery Chain (+1 more)

### Community 43 - "Community 43"
Cohesion: 0.54
Nodes (7): KeyEvent, Vec, KeyModifiers, cursor_key(), encode_key(), modifier_code(), tilde_key()

### Community 44 - "Community 44"
Cohesion: 0.25
Nodes (8): Diff Size Pagination, Git Read Surface, Inline Review Comments, PWA Diff Viewer, TUI Diff Viewer, Unified Diff Parser, Bootstrap Safety Gate, Worktree Bootstrap Script

### Community 45 - "Community 45"
Cohesion: 0.29
Nodes (7): bauded Deployment, Tailnet Security Boundary, Tailscale Auth Key, Tailscale Sidecar, Message-Posting Model, Remote Daemon Architecture, Terminal Peek Escape Hatch

### Community 46 - "Community 46"
Cohesion: 0.38
Nodes (7): Add Job Control, API Job: Fix the flaky auth test, Baude Job Queue, Compact Mobile Queue List UI, Infra Job: Tighten compose healthchecks, 2 Waiting Summary, Webapp Job: Migrate webpack config to vite

### Community 47 - "Community 47"
Cohesion: 0.33
Nodes (6): Race Aggregate, Command Palette, Frecency and Quick Switch, Fuzzy Session Switcher, Stable-Order Grouping, Tags Grouping Sort and Filter

### Community 48 - "Community 48"
Cohesion: 0.33
Nodes (6): mcp_server_config(), mcp_server_config_never_panics_on_odd_exe(), mcp_server_config_shape(), merge_mcp_config(), merge_mcp_config_never_panics_on_odd_inputs(), merge_mcp_config_preserves_siblings_and_is_idempotent()

### Community 49 - "Community 49"
Cohesion: 0.40
Nodes (5): Claude Config Volume, Full Status-Line Capture, Session Observability Contract, Statusline Bridge, PR Lifecycle Surface

### Community 50 - "Community 50"
Cohesion: 0.50
Nodes (5): Native Claude Integration, Diff Review Loop, Autonomy Budget Cap, Session Orchestration, Race N Sessions

### Community 51 - "Community 51"
Cohesion: 0.60
Nodes (5): Application Icon, Cyan Angular Frame, Dark Background, Geometric Brand Mark, Yellow Square

### Community 52 - "Community 52"
Cohesion: 0.50
Nodes (3): permission, bash, $schema

### Community 53 - "Community 53"
Cohesion: 0.50
Nodes (3): permission, bash, $schema

### Community 54 - "Community 54"
Cohesion: 0.50
Nodes (4): Baude App Icon, Cyan Angular Form, Dark Background, Yellow Square

### Community 55 - "Community 55"
Cohesion: 0.50
Nodes (4): Geometric Application Icon, Cyan Right-Angle Form, Dark Background, Yellow Square

### Community 56 - "Community 56"
Cohesion: 0.50
Nodes (4): Baude App Icon, Cyan TUI Corner-Block Glyph, Dark Rounded-Square Background, Yellow Waiting Pulse Dot

## Knowledge Gaps
- **203 isolated node(s):** `HERE`, `SANDBOX`, `LOG`, `events`, `serve` (+198 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **8 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Manager` connect `Community 0` to `Community 2`, `Community 38`, `Community 30`?**
  _High betweenness centrality (0.153) - this node is a cross-community bridge._
- **Why does `now_unix_ms()` connect `Community 0` to `Community 1`, `Community 3`, `Community 4`, `Community 9`, `Community 10`, `Community 30`?**
  _High betweenness centrality (0.137) - this node is a cross-community bridge._
- **What connects `HERE`, `SANDBOX`, `LOG` to the rest of the system?**
  _227 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.07032474804031355 - nodes in this community are weakly interconnected._
- **Should `Community 1` be split into smaller, more focused modules?**
  _Cohesion score 0.06816479400749063 - nodes in this community are weakly interconnected._
- **Should `Community 2` be split into smaller, more focused modules?**
  _Cohesion score 0.08795845504706264 - nodes in this community are weakly interconnected._
- **Should `Community 3` be split into smaller, more focused modules?**
  _Cohesion score 0.08531073446327683 - nodes in this community are weakly interconnected._