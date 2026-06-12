use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{inner, pane_rects, App, Focus, Modal};
use crate::meta::{human_tokens, short_mode, short_model};
use crate::pty::Pty;
use crate::session::{human_duration, Session, Status};

pub const SIDEBAR_WIDTH: u16 = 28;

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

fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(app.focus == Focus::Sidebar))
        .title(" baude ");
    let list_area = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for id in app.sorted_ids() {
        let Some(s) = app.session(id) else { continue };
        let selected = app.selected_id == Some(id);
        let (icon, icon_style, suffix) = match s.status() {
            Status::Waiting => (
                "●",
                Style::default().fg(Color::Yellow),
                format!(" {}", human_duration(s.waiting_for_ms())),
            ),
            Status::Busy => ("◐", Style::default().fg(Color::Blue), String::new()),
            Status::Exited => ("✗", Style::default().fg(Color::DarkGray), String::new()),
        };
        let name_style = if selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else if matches!(s.status(), Status::Exited) {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Gray)
        };
        let marker = if selected { "▸ " } else { "  " };
        let max_name = list_area.width.saturating_sub(6 + suffix.len() as u16) as usize;
        let mut name = s.name.clone();
        if name.chars().count() > max_name && max_name > 1 {
            name = name
                .chars()
                .take(max_name.saturating_sub(1))
                .collect::<String>()
                + "…";
        }
        lines.push(Line::from(vec![
            Span::styled(marker, name_style),
            Span::styled(icon.to_string(), icon_style),
            Span::raw(" "),
            Span::styled(name, name_style),
            Span::styled(suffix, Style::default().fg(Color::Yellow)),
        ]));
        lines.push(meta_line(s));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no sessions",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "  press n to add",
            Style::default().fg(Color::DarkGray),
        )));
    }
    frame.render_widget(Paragraph::new(lines), list_area);
}

/// Compact second sidebar line: model · context% · permission mode · gsd phase.
fn meta_line(s: &Session) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let mut spans: Vec<Span> = vec![Span::styled("  ", dim)];
    let push = |spans: &mut Vec<Span>, text: String, style: Style| {
        if spans.len() > 1 {
            spans.push(Span::styled(" ", dim));
        }
        spans.push(Span::styled(text, style));
    };
    if let Some(m) = &s.meta.model {
        push(&mut spans, short_model(m), dim);
    }
    if let Some(pct) = s.meta.context_used_pct {
        let style = if pct >= 80 {
            Style::default().fg(Color::Red)
        } else if pct >= 60 {
            Style::default().fg(Color::Yellow)
        } else {
            dim
        };
        push(&mut spans, format!("{pct}%"), style);
    }
    if let Some(mode) = &s.meta.permission_mode {
        let style = if mode == "bypassPermissions" {
            Style::default().fg(Color::Red)
        } else {
            dim
        };
        push(&mut spans, short_mode(mode).to_string(), style);
    }
    if let Some(gsd) = &s.meta.gsd {
        if let Some(phase) = &gsd.active_phase {
            push(
                &mut spans,
                format!("ph{phase}"),
                Style::default().fg(Color::Green),
            );
        }
    }
    if spans.len() == 1 {
        spans.push(Span::styled("—", dim));
    }
    Line::from(spans)
}

fn draw_content(frame: &mut Frame, app: &App, area: Rect) {
    let Some(s) = app.selected() else {
        let welcome = Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  baude — multiple claude sessions, one terminal",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::raw("  n  start a session in a repo folder"),
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

    let status_word = match s.status() {
        Status::Waiting => " waiting ",
        Status::Busy => " working ",
        Status::Exited => " exited — r to restart ",
    };
    let mode_word = s
        .meta
        .permission_mode
        .as_deref()
        .map(|m| format!("· {} ", short_mode(m)))
        .unwrap_or_default();
    let title = format!(" {} ·{}{}", s.name, status_word, mode_word);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style(app.focus == Focus::Claude))
        .title(title);
    frame.render_widget(block, claude_rect);
    draw_term(
        frame,
        inner(claude_rect),
        &s.claude,
        app.focus == Focus::Claude,
    );

    if let Some(sr) = shell_rect {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style(app.focus == Focus::Shell))
            .title(format!(" shell @ {} ", s.cwd.display()));
        frame.render_widget(block, sr);
        if let Some(shell) = &s.shell {
            draw_term(frame, inner(sr), shell, app.focus == Focus::Shell);
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

fn draw_term(frame: &mut Frame, area: Rect, pty: &Pty, focused: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Ok(parser) = pty.parser.lock() else {
        return;
    };
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
            target.modifier = mods;
            col += if cell.is_wide() { 2 } else { 1 };
        }
    }

    if focused && !screen.hide_cursor() {
        let (crow, ccol) = screen.cursor_position();
        if crow < rows && ccol < cols {
            let cell = &mut buf[(area.x + ccol, area.y + crow)];
            cell.modifier ^= Modifier::REVERSED;
        }
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let line = if let Some((msg, _)) = &app.message {
        Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ))
    } else {
        let hints = match app.focus {
            Focus::Sidebar => {
                " enter attach · j/k select · t shell · i info · g gsd · n new · w worktree · r restart · x close · ? help · q quit"
            }
            Focus::Claude => " ctrl+q sidebar · ctrl+\\ shell",
            Focus::Shell => " ctrl+q sidebar · ctrl+\\ close shell",
        };
        Line::from(Span::styled(hints, Style::default().fg(Color::DarkGray)))
    };
    frame.render_widget(Paragraph::new(line), area);
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
            let mut lines = vec![Line::from(vec![
                Span::raw(buf.clone()),
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
            let name = app.session(*id).map(|s| s.name.clone()).unwrap_or_default();
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
            let rect = centered(area, 58, 20);
            frame.render_widget(Clear, rect);
            let dim = Style::default().fg(Color::DarkGray);
            let p = Paragraph::new(vec![
                Line::from(Span::styled(
                    "sidebar (control mode)",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::raw("  j/k ↑/↓     select session"),
                Line::raw("  enter       attach to selected session"),
                Line::raw("  t           toggle shell pane"),
                Line::raw("  i           session info (model, tokens, context)"),
                Line::raw("  g           gsd project state"),
                Line::raw("  n           new session (repo path)"),
                Line::raw("  w           new worktree session for selected repo"),
                Line::raw("  r           restart exited claude"),
                Line::raw("  x           close session"),
                Line::raw("  q           quit (sessions resume next launch)"),
                Line::raw(""),
                Line::from(Span::styled(
                    "attached to a session",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::raw("  ctrl+q      back to sidebar"),
                Line::raw("  ctrl+\\      toggle shell pane (focuses it)"),
                Line::raw(""),
                Line::from(Span::styled(
                    "status (sidebar sort order)",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::raw("  ● waiting for your input — longest wait on top"),
                Line::raw("  ◐ working"),
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
