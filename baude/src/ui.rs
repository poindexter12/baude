use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use baude_core::meta::{
    human_tokens, human_until, now_unix_ms, short_mode, short_model, HookEvent, RateWindow,
};
use baude_core::pty::now_ms;
use baude_core::session::{human_duration, Session, StateSource, Status};
use baude_core::vt100;

use crate::app::{inner, pane_rects, App, Focus, Modal, SelId};
use crate::remote::RemoteInfo;
use crate::usage::human_cost;

pub const SIDEBAR_WIDTH: u16 = 42;

pub struct LayoutRects {
    pub sidebar: Rect,
    pub content: Rect,
    pub status: Rect,
}

pub fn layout(area: Rect) -> LayoutRects {
    let status = Rect {
        y: area.y + area.height.saturating_sub(1),
        height: 1,
        ..area
    };
    let body_h = area.height.saturating_sub(1);
    let sidebar = Rect {
        width: SIDEBAR_WIDTH.min(area.width / 3),
        height: body_h,
        ..area
    };
    let content = Rect {
        x: area.x + sidebar.width,
        width: area.width.saturating_sub(sidebar.width),
        height: body_h,
        ..area
    };
    LayoutRects {
        sidebar,
        content,
        status,
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let rects = layout(frame.area());
    draw_sidebar(frame, app, rects.sidebar);
    draw_content(frame, app, rects.content);
    draw_status_bar(frame, app, rects.status);
    draw_modal(frame, app);
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// A rotating quarter-circle spinner, advanced by the wall clock so "working"
/// sessions visibly animate.
fn spinner() -> &'static str {
    const FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
    FRAMES[((now_ms() / 130) % 4) as usize]
}

/// ~1.4 Hz on/off phase used to flash sessions that want your input.
fn flash_on() -> bool {
    (now_ms() / 360).is_multiple_of(2)
}

/// 2-column left gutter: an accent bar on the selected session, blank
/// otherwise. Keeps every row aligned whether selected or not. The bar is
/// cyan while the sidebar has focus and dims to gray when attention has
/// moved to the content pane (inactive-selection convention).
fn gutter(selected: bool, focused: bool) -> Span<'static> {
    if selected {
        let c = if focused {
            Color::Cyan
        } else {
            Color::DarkGray
        };
        Span::styled("▌ ", Style::default().fg(c))
    } else {
        Span::raw("  ")
    }
}

/// Background band painted across both lines of the selected session. A
/// shared background is the strongest grouping cue a terminal offers: it
/// marks the selection *and* binds the name line to its meta line.
fn selection_bg() -> Style {
    Style::default().bg(Color::Indexed(237))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".into();
    }
    s.chars().take(max - 1).collect::<String>() + "…"
}

fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(app.focus == Focus::Sidebar))
        .title(concat!(" baude v", env!("CARGO_PKG_VERSION"), " "));
    let list_area = block.inner(area);
    frame.render_widget(block, area);

    // Carve a usage footer off the bottom of the sidebar when there's room.
    const FOOTER_H: u16 = 6;
    let (list_area, footer_area) = if list_area.height >= FOOTER_H + 4 {
        let footer = Rect {
            y: list_area.y + list_area.height - FOOTER_H,
            height: FOOTER_H,
            ..list_area
        };
        let list = Rect {
            height: list_area.height - FOOTER_H,
            ..list_area
        };
        (list, Some(footer))
    } else {
        (list_area, None)
    };
    if let Some(fa) = footer_area {
        draw_usage_footer(frame, app, fa);
    }

    let ids = app.ordered_ids();
    if ids.is_empty() {
        let dim = Style::default().fg(Color::DarkGray);
        frame.render_widget(
            Paragraph::new(vec![
                Line::raw(""),
                Line::from(Span::styled("  no sessions yet", dim)),
                Line::from(Span::styled("  press n to add one", dim)),
            ]),
            list_area,
        );
        return;
    }

    let width = list_area.width as usize;
    let focused = app.focus == Focus::Sidebar;
    let mut lines: Vec<Line> = Vec::new();
    let mut remote_header_drawn = false;
    let mut archive_header_drawn = false;
    for id in &ids {
        let archived = app.is_archived(*id);
        if archived && !archive_header_drawn {
            archive_header_drawn = true;
            let dim = Style::default().fg(Color::DarkGray);
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("▼ archived", dim),
            ]));
        }
        match id {
            SelId::Local(lid) => {
                let Some(s) = app.session(*lid) else { continue };
                let selected = app.selected_id == Some(*id);
                session_row(
                    &mut lines,
                    selected,
                    focused,
                    s.status(),
                    &s.name,
                    s.waiting_for_ms(),
                    width,
                    archived,
                );
                lines.push(meta_line(s, selected, focused, width));
            }
            SelId::Remote(rid) => {
                if !remote_header_drawn && !archived {
                    remote_header_drawn = true;
                    lines.push(remote_header(app, width));
                }
                let Some(r) = app.remote_info(*rid) else {
                    continue;
                };
                let selected = app.selected_id == Some(*id);
                let status = remote_status(r);
                let waiting_ms = r
                    .waiting_for_ms
                    .map(|ms| ms + now_ms().saturating_sub(app.remote_snap.fetched_ms));
                session_row(
                    &mut lines,
                    selected,
                    focused,
                    status,
                    &r.name,
                    waiting_ms.unwrap_or(0),
                    width,
                    archived,
                );
                lines.push(remote_meta_line(r, selected, focused, width));
            }
        }
    }
    // Remote configured but no active sessions: still show the header.
    if app.remote.is_some() && !remote_header_drawn {
        lines.push(remote_header(app, width));
    }
    frame.render_widget(Paragraph::new(lines), list_area);
}

/// Map a daemon status word onto the local status enum for shared styling.
/// The `_ => Waiting` fallback is deliberate back-compat: an unknown word from
/// a newer daemon degrades to the urgent-idle state (never silently "calm").
fn remote_status(r: &RemoteInfo) -> Status {
    match r.status.as_str() {
        "busy" => Status::Busy,
        "completed" => Status::Completed,
        "exited" => Status::Exited,
        _ => Status::Waiting,
    }
}

fn remote_header(app: &App, width: usize) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let label = if app.remote_snap.ok {
        "⇄ remote".to_string()
    } else {
        "⇄ remote (offline)".to_string()
    };
    let label = truncate(&label, width.saturating_sub(3).max(1));
    Line::from(vec![Span::raw("  "), Span::styled(label, dim)])
}

/// Assemble a meta line from chips: a dim `↳` connector sits under the
/// status icon so every meta line visibly hangs off the session name above
/// it (an unadorned second line reads as ambiguous between neighbors),
/// `·` separators bind the chips into one annotation, and the selected row
/// gets the selection band + padding.
fn chips_line(
    chips: Vec<(String, Style)>,
    selected: bool,
    focused: bool,
    width: usize,
) -> Line<'static> {
    let sep = Style::default().fg(Color::DarkGray);
    let mut spans: Vec<Span> = vec![gutter(selected, focused), Span::styled("↳ ", sep)];
    let mut used = 4usize;
    for (i, (text, style)) in chips.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", sep));
            used += 3;
        }
        used += text.chars().count();
        spans.push(Span::styled(text, style));
    }
    if selected {
        spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
        Line::from(spans).style(selection_bg())
    } else {
        Line::from(spans)
    }
}

/// Baseline chip style: a whisper. Slightly brighter on the selected row so
/// it stays readable on the selection band.
fn chip_base(selected: bool) -> Style {
    Style::default().fg(if selected {
        Color::Gray
    } else {
        Color::DarkGray
    })
}

fn remote_meta_line(r: &RemoteInfo, selected: bool, focused: bool, width: usize) -> Line<'static> {
    let base = chip_base(selected);
    let mut chips: Vec<(String, Style)> = Vec::new();
    if let Some(m) = &r.model {
        chips.push((short_model(m), base));
    }
    if let Some(pct) = r.context_used_pct {
        let style = if pct >= 80 {
            Style::default().fg(Color::Red)
        } else if pct >= 60 {
            Style::default().fg(Color::Yellow)
        } else {
            base
        };
        chips.push((format!("{pct}%"), style));
    }
    // The 5h window is account-global (the footer gauge owns it); surface it
    // per-row only once it's hot enough to warrant attention.
    if let Some(pct) = r.rate_5h_used_pct {
        if pct >= 60 {
            let (text, style) = rate_5h_chip(pct, r.rate_5h_resets_at_unix_s, base);
            chips.push((text, style));
        }
    }
    if let Some(mode) = &r.permission_mode {
        let style = if mode == "bypassPermissions" {
            Style::default().fg(Color::Red)
        } else {
            base
        };
        chips.push((short_mode(mode).to_string(), style));
    }
    if let Some(phase) = &r.gsd_active_phase {
        // Identity metadata, not status — styled like the model name so
        // saturated color on this line always means "alarm".
        chips.push((format!("ph{phase}"), base));
    }
    if chips.is_empty() {
        chips.push(("—".into(), base));
    }
    chips_line(chips, selected, focused, width)
}

/// One sidebar session row: status icon, name, and (when waiting) a
/// right-aligned wait timer. Shared by local and remote sessions.
#[allow(clippy::too_many_arguments)]
fn session_row(
    lines: &mut Vec<Line<'static>>,
    selected: bool,
    focused: bool,
    status: Status,
    name: &str,
    waiting_ms: u64,
    width: usize,
    archived: bool,
) {
    // Archived rows never flash or color — they've stopped asking for you.
    let flash = !archived && flash_on();

    let (icon, icon_style) = match status {
        _ if archived => ("·", Style::default().fg(Color::DarkGray)),
        Status::Waiting => {
            // pulse the dot to pull the eye to a session that needs you
            let c = if flash {
                Color::Yellow
            } else {
                Color::DarkGray
            };
            ("●", Style::default().fg(c).add_modifier(Modifier::BOLD))
        }
        Status::Busy => (spinner(), Style::default().fg(Color::Blue)),
        // A finished turn is calm — a steady green check, never flashing. It's
        // your move, but nothing is blocked, so it must not pull the eye the
        // way a `Waiting` (needs-input) session does.
        Status::Completed => (
            "✓",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::DIM),
        ),
        Status::Exited => ("✗", Style::default().fg(Color::DarkGray)),
    };

    // The name never flashes: the pulsing icon and timer already carry the
    // needs-input signal, and a whole flashing word drowns out the selection
    // cue once a few sessions are waiting. A selected dead session caps at
    // Gray — findable, but never as alive-looking as a running one.
    let name_style = if archived || matches!(status, Status::Exited) {
        if selected {
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        }
    } else if selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    // Both a needs-input (`Waiting`) and a finished (`Completed`) session show
    // how long they've been idle; the color below tells them apart (urgent
    // yellow vs. calm gray "done Xm ago").
    let suffix = if status == Status::Waiting || status == Status::Completed {
        human_duration(waiting_ms)
    } else {
        String::new()
    };
    let suffix_w = suffix.chars().count();
    // gutter(2) + icon(1) + space(1) = 4; reserve a space before the timer.
    let reserve = if suffix.is_empty() { 0 } else { suffix_w + 1 };
    let name = truncate(name, width.saturating_sub(4 + reserve).max(1));
    let used = 4 + name.chars().count() + suffix_w;
    let pad = width.saturating_sub(used);

    let mut spans = vec![
        gutter(selected, focused),
        Span::styled(icon.to_string(), icon_style),
        Span::raw(" "),
        Span::styled(name, name_style),
    ];
    if !suffix.is_empty() {
        spans.push(Span::raw(" ".repeat(pad)));
        // Only a needs-input session flashes its timer yellow; a completed
        // session's "done Xm ago" stays calm gray.
        spans.push(Span::styled(
            suffix,
            Style::default().fg(if flash && status == Status::Waiting {
                Color::Yellow
            } else {
                Color::DarkGray
            }),
        ));
    } else if selected {
        // Pad so the selection band spans the full sidebar width.
        spans.push(Span::raw(" ".repeat(pad)));
    }
    let line = Line::from(spans);
    lines.push(if selected {
        line.style(selection_bg())
    } else {
        line
    });
}

/// Color a usage percentage: green → yellow (60%) → red (85%).
fn pct_style(pct: f64) -> Style {
    if pct >= 85.0 {
        Style::default().fg(Color::Red)
    } else if pct >= 60.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}

/// `label ▓▓▓▓░░░░░░ 47%` gauge row for a rate-limit window.
fn rate_line(label: &str, w: Option<RateWindow>, width: usize) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let Some(pct) = w.and_then(|w| w.used_pct) else {
        return Line::from(vec![
            Span::styled(format!(" {label} "), dim),
            Span::styled("—", dim),
        ]);
    };
    let bar_w = width.saturating_sub(label.len() + 7).clamp(4, 12);
    let filled = ((pct / 100.0) * bar_w as f64).round() as usize;
    Line::from(vec![
        Span::styled(format!(" {label} "), dim),
        Span::styled("▓".repeat(filled.min(bar_w)), pct_style(pct)),
        Span::styled("░".repeat(bar_w - filled.min(bar_w)), dim),
        Span::styled(format!(" {:>3.0}%", pct), pct_style(pct)),
    ])
}

/// Sidebar footer: session/today/week cost + account rate-limit gauges.
fn draw_usage_footer(frame: &mut Frame, app: &App, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let val = Style::default().fg(Color::Gray);
    let width = area.width as usize;

    let cost_row = |label: &str, cost: String| {
        let pad = width.saturating_sub(2 + label.len() + cost.len());
        Line::from(vec![
            Span::styled(format!(" {label}"), dim),
            Span::raw(" ".repeat(pad)),
            Span::styled(cost, val),
        ])
    };

    let costs = app.usage_costs();
    let session_cost = app.selected().and_then(|s| s.meta.session_cost_usd);
    let (r5h, rweek) = app.rate_limits();

    let lines = vec![
        Line::from(Span::styled("─".repeat(width), dim)),
        cost_row("sess", human_cost(session_cost)),
        cost_row("today", human_cost(costs.today_usd)),
        cost_row("week", human_cost(costs.week_usd)),
        rate_line("5h", r5h, width),
        rate_line("wk", rweek, width),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

/// Per-session 5h rate-limit chip: `5h 47% in 2h12m` (reset countdown dropped
/// when unknown). Colors the chip like context% — red ≥80, yellow ≥60, else
/// dim — so a near-exhausted window stands out in the sidebar. Shared by the
/// local and remote second-line builders so both render the window identically.
fn rate_5h_chip(pct: u8, resets_at: Option<u64>, dim: Style) -> (String, Style) {
    let text = match resets_at {
        Some(t) => format!("5h {pct}% {}", human_until(t)),
        None => format!("5h {pct}%"),
    };
    let style = if pct >= 80 {
        Style::default().fg(Color::Red)
    } else if pct >= 60 {
        Style::default().fg(Color::Yellow)
    } else {
        dim
    };
    (text, style)
}

/// Compact second sidebar line: model · context% · permission mode · gsd phase.
/// Deliberately a whisper: the baseline is dim and saturated color is reserved
/// for alarms (red bypass, hot context/rate windows), so color here always
/// means "look at me".
fn meta_line(s: &Session, selected: bool, focused: bool, width: usize) -> Line<'static> {
    let base = chip_base(selected);
    let mut chips: Vec<(String, Style)> = Vec::new();
    if let Some(m) = &s.meta.model {
        chips.push((short_model(m), base));
    }
    if let Some(pct) = s.meta.context_used_pct {
        let style = if pct >= 80 {
            Style::default().fg(Color::Red)
        } else if pct >= 60 {
            Style::default().fg(Color::Yellow)
        } else {
            base
        };
        chips.push((format!("{pct}%"), style));
    }
    // The 5h window is account-global (the footer gauge owns it); surface it
    // per-row only once it's hot enough to warrant attention.
    if let Some(w) = s.meta.rate_5h {
        if let Some(p) = w.used_pct {
            let pct = (p.round() as u64).min(100) as u8;
            if pct >= 60 {
                let (text, style) = rate_5h_chip(pct, w.resets_at_unix_s, base);
                chips.push((text, style));
            }
        }
    }
    // BL-02: fall back to baude's spawn-intended mode (skip→bypassPermissions)
    // when the transcript hasn't reported one yet, so every local session shows
    // its mode. A transcript-reported mode always wins.
    let mode = s
        .meta
        .permission_mode
        .clone()
        .or_else(|| baude_core::permission::spawn_permission_mode().map(str::to_string));
    if let Some(mode) = mode {
        let style = if mode == "bypassPermissions" {
            Style::default().fg(Color::Red)
        } else {
            base
        };
        chips.push((short_mode(&mode).to_string(), style));
    }
    if let Some(gsd) = &s.meta.gsd {
        if let Some(phase) = &gsd.active_phase {
            // Identity metadata, not status — styled like the model name so
            // saturated color on this line always means "alarm".
            chips.push((format!("ph{phase}"), base));
        }
    }
    if chips.is_empty() {
        chips.push(("—".into(), base));
    }
    chips_line(chips, selected, focused, width)
}

fn draw_content(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(r) = app.selected_remote() {
        draw_remote_content(frame, app, area, r);
        return;
    }
    let Some(s) = app.selected() else {
        let welcome = Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  baude — multiple claude sessions, one terminal",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::raw("  n  start a session in a repo folder"),
            Line::raw("  c  clone a github repo and start there"),
            Line::raw("  ?  help"),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style(false)),
        );
        frame.render_widget(welcome, area);
        return;
    };

    let (claude_rect, shell_rect) = pane_rects(area, s.shell_open);

    let scroll_hint = if app.claude_scroll > 0 {
        format!(" ↑{} ", app.claude_scroll)
    } else {
        String::new()
    };
    let status_word = match s.status() {
        Status::Waiting => " waiting ",
        Status::Busy => " working ",
        Status::Completed => " completed ",
        Status::Exited => " exited — r to restart ",
    };
    let mode_word = s
        .meta
        .permission_mode
        .as_deref()
        .map(|m| format!("· {} ", short_mode(m)))
        .unwrap_or_default();
    let title = format!(" {} ·{}{}{}", s.name, status_word, mode_word, scroll_hint);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(app.focus == Focus::Claude))
        .title(title);
    frame.render_widget(block, claude_rect);

    let claude_sel = app.selection.as_ref().filter(|s| !s.is_shell);
    draw_term(
        frame,
        inner(claude_rect),
        &s.claude.parser,
        app.focus == Focus::Claude,
        app.claude_scroll,
        claude_sel,
    );

    if let Some(sr) = shell_rect {
        let shell_scroll_hint = if app.shell_scroll > 0 {
            format!(" ↑{} ", app.shell_scroll)
        } else {
            String::new()
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style(app.focus == Focus::Shell))
            .title(format!(
                " shell @ {} {}",
                s.cwd.display(),
                shell_scroll_hint
            ));
        frame.render_widget(block, sr);
        if let Some(shell) = &s.shell {
            let shell_sel = app.selection.as_ref().filter(|s| s.is_shell);
            draw_term(
                frame,
                inner(sr),
                &shell.parser,
                app.focus == Focus::Shell,
                app.shell_scroll,
                shell_sel,
            );
        }
    }
}

fn draw_remote_content(frame: &mut Frame, app: &App, area: Rect, r: &RemoteInfo) {
    let status_word = match r.status.as_str() {
        "busy" => " working ",
        "completed" => " completed ",
        "exited" => " exited — r to restart ",
        _ => " waiting ",
    };
    let title = format!(" {} @ remote ·{status_word}", r.name);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(app.focus == Focus::Claude))
        .title(title);
    frame.render_widget(block, area);

    let attached = app
        .attach
        .as_ref()
        .filter(|a| a.remote_id == r.id && !a.is_closed());
    match attached {
        Some(a) => {
            let sel = app.selection.as_ref().filter(|s| !s.is_shell);
            draw_term(
                frame,
                inner(area),
                &a.parser,
                app.focus == Focus::Claude,
                app.claude_scroll,
                sel,
            );
        }
        None => {
            let dim = Style::default().fg(Color::DarkGray);
            let hint = Paragraph::new(vec![
                Line::raw(""),
                Line::from(Span::styled("  press enter to attach", dim)),
            ]);
            frame.render_widget(hint, inner(area));
        }
    }
}

fn vt_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn draw_term(
    frame: &mut Frame,
    area: Rect,
    parser: &std::sync::Mutex<vt100::Parser>,
    focused: bool,
    scroll_offset: usize,
    selection: Option<&crate::app::Selection>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Ok(mut parser) = parser.lock() else {
        return;
    };
    parser.set_scrollback(scroll_offset);
    {
        let screen = parser.screen();
        let (srows, scols) = screen.size();
        let rows = area.height.min(srows);
        let cols = area.width.min(scols);
        let buf = frame.buffer_mut();

        for row in 0..rows {
            let mut col = 0u16;
            while col < cols {
                let x = area.x + col;
                let y = area.y + row;
                let Some(cell) = screen.cell(row, col) else {
                    col += 1;
                    continue;
                };
                if cell.is_wide_continuation() {
                    col += 1;
                    continue;
                }
                let target = &mut buf[(x, y)];
                let contents = cell.contents();
                if contents.is_empty() {
                    target.set_symbol(" ");
                } else {
                    target.set_symbol(&contents);
                }
                target.set_fg(vt_color(cell.fgcolor()));
                target.set_bg(vt_color(cell.bgcolor()));
                let mut mods = Modifier::empty();
                if cell.bold() {
                    mods |= Modifier::BOLD;
                }
                if cell.italic() {
                    mods |= Modifier::ITALIC;
                }
                if cell.underline() {
                    mods |= Modifier::UNDERLINED;
                }
                if cell.inverse() {
                    mods |= Modifier::REVERSED;
                }
                if selection.map(|s| s.contains(row, col)) == Some(true) {
                    mods ^= Modifier::REVERSED;
                }
                target.modifier = mods;
                col += if cell.is_wide() { 2 } else { 1 };
            }
        }

        if scroll_offset == 0 && focused && !screen.hide_cursor() {
            let (crow, ccol) = screen.cursor_position();
            if crow < rows && ccol < cols {
                let cell = &mut buf[(area.x + ccol, area.y + crow)];
                cell.modifier ^= Modifier::REVERSED;
            }
        }
    }
    parser.set_scrollback(0);
}

/// Shorten a path for display: home → `~`.
fn tilde_path(p: &std::path::Path) -> String {
    let s = p.display().to_string();
    match dirs::home_dir() {
        Some(h) => {
            let h = h.display().to_string();
            s.strip_prefix(&h)
                .map(|rest| format!("~{rest}"))
                .unwrap_or(s)
        }
        None => s,
    }
}

/// Status bar: `hints │ ~/path ⎇ branch` with right-aligned session counts
/// and rate-limit reset times. Transient messages take over the whole bar.
fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    if let Some((msg, _)) = &app.message {
        let line = Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ));
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let dim = Style::default().fg(Color::DarkGray);
    let val = Style::default().fg(Color::Gray);
    let width = area.width as usize;

    let hints = match app.focus {
        Focus::Sidebar => " enter attach · n new · t shell · e edit · ? help",
        Focus::Claude => " ctrl+q sidebar · ctrl+\\ shell · alt+←/→ cycle",
        Focus::Shell => " ctrl+q sidebar · ctrl+\\ close shell · alt+←/→ cycle",
    };

    let mut spans: Vec<Span> = vec![Span::styled(hints.to_string(), dim)];
    let mut used = hints.chars().count();

    // Selected session's full path + branch — too wide for the sidebar.
    if let Some(s) = app.selected() {
        let mut loc = format!(" │ {}", tilde_path(&s.cwd));
        if let Some(b) = s.branch.as_deref().or(s.meta.git_branch.as_deref()) {
            loc.push_str(&format!(" ⎇ {b}"));
        }
        if used + loc.chars().count() <= width {
            used += loc.chars().count();
            spans.push(Span::styled(loc, val));
        }
    }

    // Right side: who needs you, and when the limits refill.
    let (waiting, busy, completed) = app.status_counts();
    let mut right: Vec<String> = Vec::new();
    if waiting > 0 {
        right.push(format!("● {waiting} waiting"));
    }
    if busy > 0 {
        right.push(format!("◐ {busy} busy"));
    }
    if completed > 0 {
        right.push(format!("✓ {completed} done"));
    }
    let (r5h, rweek) = app.rate_limits();
    if let Some(t) = r5h.and_then(|w| w.resets_at_unix_s) {
        right.push(format!("5h resets {}", human_until(t)));
    }
    if let Some(t) = rweek.and_then(|w| w.resets_at_unix_s) {
        right.push(format!("wk {}", human_until(t)));
    }
    let right = right.join(" · ");
    if !right.is_empty() && used + right.chars().count() + 2 <= width {
        let pad = width - used - right.chars().count() - 1;
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(right, dim));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Left-truncate an input buffer so its tail — where the cursor sits — stays
/// visible within `width` cells (one reserved for the cursor block), with a
/// leading `…` marking the cut.
fn input_tail(buf: &str, width: usize) -> String {
    let avail = width.saturating_sub(1);
    let n = buf.chars().count();
    if n <= avail {
        return buf.to_string();
    }
    let keep = avail.saturating_sub(1);
    let tail: String = buf.chars().skip(n - keep).collect();
    format!("…{tail}")
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Pick an icon for a hook event by kind/tool so the activity overlay reads at
/// a glance (mirrors the PWA strip's icon convention).
fn activity_icon(ev: &HookEvent) -> &'static str {
    match ev.event.as_str() {
        "PostToolUse" => "⚙",
        "UserPromptSubmit" => "›",
        "Stop" => "■",
        "Notification" => "🔔",
        _ => "·",
    }
}

/// Short human label for one activity event: the tool name for tool uses, the
/// notification type for notifications, else the event kind itself.
fn activity_label(ev: &HookEvent) -> String {
    match ev.event.as_str() {
        "PostToolUse" => ev.tool.clone().unwrap_or_else(|| "tool".into()),
        "Notification" => ev
            .notification_type
            .clone()
            .unwrap_or_else(|| "notification".into()),
        other => other.to_string(),
    }
}

/// Compact relative age (e.g. `now`, `12s`, `4m`, `2h`) for an event timestamp
/// in unix ms, matching the at-a-glance tone of the overlay.
fn activity_age(ts_ms: u64) -> String {
    let secs = now_unix_ms().saturating_sub(ts_ms) / 1000;
    if secs == 0 {
        "now".into()
    } else if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

/// Build the activity overlay body for a slice of events, newest-at-bottom.
/// Render-last-N-that-fit: clip to the most recent events that fit the box
/// (RESEARCH Open Q1 first-cut). Paging the full ~200 is deferred to the GET
/// `/sessions/{id}/activity` endpoint — no scroll-offset widget this phase.
fn activity_lines<'a, I>(events: I, max_rows: usize) -> Vec<Line<'a>>
where
    I: ExactSizeIterator<Item = &'a HookEvent>,
{
    let dim = Style::default().fg(Color::DarkGray);
    let val = Style::default().fg(Color::White);
    let total = events.len();
    if total == 0 {
        return vec![Line::from(Span::styled("  no activity yet", dim))];
    }
    // Take the last `max_rows` events (newest), then render oldest→newest so the
    // newest sits at the bottom (chronological, matches the chat view).
    let skip = total.saturating_sub(max_rows);
    let mut lines: Vec<Line> = events
        .skip(skip)
        .map(|ev| {
            Line::from(vec![
                Span::styled(format!("  {} ", activity_icon(ev)), val),
                Span::styled(format!("{:<14}", activity_label(ev)), val),
                Span::styled(format!("  {}", activity_age(ev.ts)), dim),
            ])
        })
        .collect();
    if skip > 0 {
        lines.insert(
            0,
            Line::from(Span::styled(format!("  … {skip} earlier"), dim)),
        );
    }
    lines
}

fn draw_modal(frame: &mut Frame, app: &App) {
    let area = frame.area();
    match &app.modal {
        Modal::None => {}
        Modal::Input {
            title,
            buf,
            candidates,
            ..
        } => {
            const MAX_SHOWN: usize = 6;
            let dim = Style::default().fg(Color::DarkGray);
            // Keep the tail (where the cursor sits) visible for long buffers.
            let inner_w = 64u16.min(area.width).saturating_sub(2) as usize;
            let mut lines = vec![Line::from(vec![
                Span::raw(input_tail(buf, inner_w)),
                Span::styled("█", Style::default().fg(Color::Cyan)),
            ])];
            for c in candidates.iter().take(MAX_SHOWN) {
                lines.push(Line::from(Span::styled(format!("  {c}/"), dim)));
            }
            if candidates.len() > MAX_SHOWN {
                lines.push(Line::from(Span::styled(
                    format!("  … {} more", candidates.len() - MAX_SHOWN),
                    dim,
                )));
            }
            let rect = centered(area, 64, lines.len() as u16 + 2);
            frame.render_widget(Clear, rect);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" {title} "));
            frame.render_widget(Paragraph::new(lines).block(block), rect);
        }
        Modal::ConfirmKill { id } => {
            let name = match id {
                SelId::Local(lid) => app.session(*lid).map(|s| s.name.clone()),
                SelId::Remote(rid) => app
                    .remote_info(*rid)
                    .map(|r| format!("{} (remote)", r.name)),
            }
            .unwrap_or_default();
            let rect = centered(area, 50, 4);
            frame.render_widget(Clear, rect);
            let p = Paragraph::new(vec![
                Line::raw(format!("close session \"{name}\"?")),
                Line::from(Span::styled(
                    "y close · n cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Red))
                    .title(" close session "),
            );
            frame.render_widget(p, rect);
        }
        Modal::ConfirmCloseWorktree { id } => {
            let name = app.session(*id).map(|s| s.name.clone()).unwrap_or_default();
            let rect = centered(area, 56, 4);
            frame.render_widget(Clear, rect);
            let p = Paragraph::new(vec![
                Line::raw(format!("close worktree session \"{name}\"?")),
                Line::from(Span::styled(
                    "k keep worktree · r remove worktree · esc cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Red))
                    .title(" close worktree session "),
            );
            frame.render_widget(p, rect);
        }
        Modal::Info => {
            if let Some(r) = app.selected_remote() {
                let dim = Style::default().fg(Color::DarkGray);
                let val = Style::default().fg(Color::White);
                let row = |label: &str, value: String| {
                    Line::from(vec![
                        Span::styled(format!("  {label:<16}"), dim),
                        Span::styled(value, val),
                    ])
                };
                let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".into());
                let mut lines = vec![
                    row("session", format!("{} (remote)", r.name)),
                    row("title", opt(&r.title)),
                    row("status", r.status.clone()),
                    row("state", opt(&r.state_source)),
                    row(
                        "model",
                        r.model
                            .as_deref()
                            .map(short_model)
                            .unwrap_or_else(|| "—".into()),
                    ),
                    row(
                        "permissions",
                        r.permission_mode
                            .as_deref()
                            .map(|p| format!("{p} ({})", short_mode(p)))
                            .unwrap_or_else(|| "—".into()),
                    ),
                    row(
                        "context used",
                        r.context_used_pct
                            .map(|p| format!("{p}%"))
                            .unwrap_or_else(|| "—".into()),
                    ),
                    row("branch", opt(&r.branch)),
                    row(
                        "session cost",
                        r.session_cost_usd
                            .map(|c| format!("${c:.2}"))
                            .unwrap_or_else(|| "—".into()),
                    ),
                ];
                if let Some(tool) = &r.last_tool {
                    lines.push(row("tool", tool.clone()));
                }
                // PERM-04: surface *why* the session is waiting ("permission"
                // flags a pending approve/deny request handled from the phone).
                if let Some(reason) = &r.waiting_reason {
                    lines.push(row("waiting", reason.clone()));
                }
                let rect = centered(frame.area(), 64, lines.len() as u16 + 2);
                frame.render_widget(Clear, rect);
                let p = Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Cyan))
                        .title(" remote session info "),
                );
                frame.render_widget(p, rect);
                return;
            }
            let Some(s) = app.selected() else { return };
            let dim = Style::default().fg(Color::DarkGray);
            let val = Style::default().fg(Color::White);
            let row = |label: &str, value: String| {
                Line::from(vec![
                    Span::styled(format!("  {label:<16}"), dim),
                    Span::styled(value, val),
                ])
            };
            let m = &s.meta;
            let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".into());
            let mut lines = vec![
                row("session", s.name.clone()),
                row("cwd", s.cwd.display().to_string()),
                row(
                    "model",
                    m.model
                        .as_deref()
                        .map(short_model)
                        .unwrap_or_else(|| "—".into()),
                ),
                row(
                    "permissions",
                    m.permission_mode
                        .as_deref()
                        .map(|p| format!("{p} ({})", short_mode(p)))
                        .unwrap_or_else(|| "—".into()),
                ),
                row(
                    "context used",
                    m.context_used_pct
                        .map(|p| format!("{p}%"))
                        .unwrap_or_else(|| "—".into()),
                ),
                row("claude session", opt(&m.session_id)),
                Line::raw(""),
            ];
            // Capture-but-render-lightly: surface which source decided the
            // state so a regression to the silence fallback is observable, and
            // the last tool Claude ran when the hook stream reported one.
            let source_label = match s.status_with_source().1 {
                StateSource::Hook => "hook",
                StateSource::SessionFile => "session-file",
                StateSource::Silence => "silence",
            };
            lines.push(row("state", source_label.into()));
            if let Some((tool, _)) = &m.last_tool {
                lines.push(row("tool", tool.clone()));
            }
            if let Some(e) = &m.effort {
                lines.push(row("effort", e.clone()));
            }
            if let Some(t) = m.thinking {
                lines.push(row("thinking", if t { "on".into() } else { "off".into() }));
            }
            if let Some(pr) = &m.pr {
                let n = pr
                    .number
                    .map(|n| format!("#{n}"))
                    .unwrap_or_else(|| "?".into());
                let st = pr.review_state.clone().unwrap_or_else(|| "—".into());
                lines.push(row("pr", format!("{n} ({st})")));
            }
            if let Some(u) = &m.last_usage {
                lines.push(row(
                    "last turn",
                    format!(
                        "in {} · out {} · cache r {} / w {}",
                        human_tokens(u.input),
                        human_tokens(u.output),
                        human_tokens(u.cache_read),
                        human_tokens(u.cache_create)
                    ),
                ));
            }
            let t = &m.totals;
            lines.push(row(
                "session total",
                format!(
                    "in {} · out {} · cache r {} / w {}",
                    human_tokens(t.input),
                    human_tokens(t.output),
                    human_tokens(t.cache_read),
                    human_tokens(t.cache_create)
                ),
            ));
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled("  press any key to close", dim)));
            let rect = centered(area, 76, lines.len() as u16 + 2);
            frame.render_widget(Clear, rect);
            frame.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Cyan))
                        .title(" session info "),
                ),
                rect,
            );
        }
        Modal::Activity => {
            let dim = Style::default().fg(Color::DarkGray);
            // Render-last-N-that-fit: cap rows to a fixed window; the box height
            // follows the rendered line count. Paging the full retained set is
            // deferred to the GET /sessions/{id}/activity endpoint (RESEARCH Q1).
            const MAX_ROWS: usize = 24;
            const WIDTH: u16 = 48;
            // Remote first (mirrors Modal::Info): read the bounded RemoteInfo
            // .activity bundled into the /sessions poll — no extra round-trip.
            if let Some(r) = app.selected_remote() {
                let mut lines = activity_lines(r.activity.iter(), MAX_ROWS);
                lines.push(Line::raw(""));
                lines.push(Line::from(Span::styled("  press any key to close", dim)));
                let rect = centered(frame.area(), WIDTH, lines.len() as u16 + 2);
                frame.render_widget(Clear, rect);
                frame.render_widget(
                    Paragraph::new(lines).block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(Color::Cyan))
                            .title(format!(" activity — {} (remote) ", r.name)),
                    ),
                    rect,
                );
                return;
            }
            // Local: read the ClaudeMeta ring directly.
            let Some(s) = app.selected() else { return };
            let mut lines = activity_lines(s.meta.activity().iter(), MAX_ROWS);
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled("  press any key to close", dim)));
            let rect = centered(area, WIDTH, lines.len() as u16 + 2);
            frame.render_widget(Clear, rect);
            frame.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Cyan))
                        .title(format!(" activity — {} ", s.name)),
                ),
                rect,
            );
        }
        Modal::Gsd => {
            let Some(s) = app.selected() else { return };
            let dim = Style::default().fg(Color::DarkGray);
            let val = Style::default().fg(Color::White);
            let row = |label: &str, value: String| {
                Line::from(vec![
                    Span::styled(format!("  {label:<16}"), dim),
                    Span::styled(value, val),
                ])
            };
            let lines = match &s.meta.gsd {
                Some(g) => {
                    let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".into());
                    let mut lines = vec![
                        row("milestone", opt(&g.milestone)),
                        row("status", opt(&g.status)),
                        row("active phase", opt(&g.active_phase)),
                        row("next action", opt(&g.next_action)),
                        row(
                            "progress",
                            g.percent
                                .map(|p| {
                                    let filled = (p as usize) / 10;
                                    format!(
                                        "[{}{}] {p}%",
                                        "█".repeat(filled),
                                        "░".repeat(10 - filled)
                                    )
                                })
                                .unwrap_or_else(|| "—".into()),
                        ),
                    ];
                    if let Some(pl) = &g.phase_line {
                        lines.push(row("state", pl.clone()));
                    }
                    lines
                }
                None => vec![Line::from(Span::styled(
                    "  no .planning/STATE.md in this repo",
                    dim,
                ))],
            };
            let mut lines = lines;
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled("  press any key to close", dim)));
            let rect = centered(area, 72, lines.len() as u16 + 2);
            frame.render_widget(Clear, rect);
            frame.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Green))
                        .title(format!(
                            " gsd — {} ",
                            s.repo_root
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default()
                        )),
                ),
                rect,
            );
        }
        Modal::Help => {
            let rect = centered(area, 60, 26);
            frame.render_widget(Clear, rect);
            let dim = Style::default().fg(Color::DarkGray);
            let p = Paragraph::new(vec![
                Line::from(Span::styled(
                    "sidebar (control mode)",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::raw("  j/k ↑/↓     select session"),
                Line::raw("  enter       attach to selected session"),
                Line::raw("  t           open shell pane (focuses it)"),
                Line::raw("  e           open folder in editor"),
                Line::raw("  i           session info (model, tokens, context)"),
                Line::raw("  v           activity timeline (recent tools)"),
                Line::raw("  g           gsd project state"),
                Line::raw("  n           new session (repo path; clones if missing)"),
                Line::raw("  c           clone a repo (github url) into a new session"),
                Line::raw("  w           new worktree session for selected repo"),
                Line::raw("  r           restart exited claude"),
                Line::raw("  a           archive/unarchive (auto after 30m idle)"),
                Line::raw("  x           close session"),
                Line::raw("  q           quit (sessions resume next launch)"),
                Line::raw(""),
                Line::from(Span::styled(
                    "global (any pane)",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::raw("  ctrl+q      back to sidebar"),
                Line::raw("  ctrl+\\      toggle shell pane (focuses it)"),
                Line::raw("  alt+←/→     cycle prev/next session"),
                Line::raw(""),
                Line::from(Span::styled(
                    "status (sidebar sort order)",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::raw("  ● waiting for your input — longest wait on top"),
                Line::raw("  ◐ working"),
                Line::raw("  ✓ completed — turn finished, your move"),
                Line::raw("  ✗ exited"),
                Line::from(Span::styled("press any key to close", dim)),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title(" help "),
            );
            frame.render_widget(p, rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rate_5h_chip;
    use ratatui::style::{Color, Style};

    #[test]
    fn rate_5h_chip_formats_and_colors() {
        let dim = Style::default().fg(Color::DarkGray);
        // No reset known -> bare "5h N%" with the dim color below 60%.
        let (text, style) = rate_5h_chip(47, None, dim);
        assert_eq!(text, "5h 47%");
        assert_eq!(style, dim);
        // >=60% is yellow, >=80% is red (mirrors context% thresholds).
        assert_eq!(
            rate_5h_chip(60, None, dim).1,
            Style::default().fg(Color::Yellow)
        );
        assert_eq!(
            rate_5h_chip(80, None, dim).1,
            Style::default().fg(Color::Red)
        );
        // A known reset appends the countdown ("in …" / "now").
        let (text, _) = rate_5h_chip(10, Some(0), dim);
        assert!(text.starts_with("5h 10% "), "got: {text}");
    }

    #[test]
    fn input_tail_shows_cursor_end() {
        use super::input_tail;
        // Fits (with room for the cursor cell): unchanged.
        assert_eq!(input_tail("short", 10), "short");
        assert_eq!(input_tail("123456789", 10), "123456789");
        // Too long: keep the tail, mark the cut, total = width - 1 cells.
        assert_eq!(input_tail("0123456789", 10), "…23456789");
        assert_eq!(input_tail("/very/long/path/to/repo", 10), "…/to/repo");
        // Degenerate widths never panic.
        assert_eq!(input_tail("abc", 0), "…");
        assert_eq!(input_tail("abc", 1), "…");
    }
}
