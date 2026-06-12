use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{inner, pane_rects, App, Focus, Modal};
use crate::pty::Pty;
use crate::session::{human_duration, Status};

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
    let title = format!(" {} ·{}", s.name, status_word);
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
                " enter attach · j/k select · t shell · n new · w worktree · r restart · x close · ? help · q quit"
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
        Modal::Input { title, buf, .. } => {
            let rect = centered(area, 64, 3);
            frame.render_widget(Clear, rect);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" {title} "));
            let input = Paragraph::new(Line::from(vec![
                Span::raw(buf.clone()),
                Span::styled("█", Style::default().fg(Color::Cyan)),
            ]))
            .block(block);
            frame.render_widget(input, rect);
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
