use crate::client::Issue;
use crate::output::format_date;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
            KeyModifiers, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::io;

pub enum TuiAction {
    /// Open selected issue key in browser
    Browse(String),
    /// Show detail view for selected issue key
    Detail(String),
    /// Return to the list (from detail view); carries last selected index
    Back(usize),
    /// User typed a search query via `/`; caller should do a JQL text search and re-enter TUI
    Search(String),
    /// User quit
    Quit,
}

pub struct IssueList<'a> {
    issues: &'a [Issue],
    state: ListState,
    total: u32,
    show_help: bool,
}

impl<'a> IssueList<'a> {
    pub fn new(issues: &'a [Issue], total: u32, initial_index: usize) -> Self {
        let mut state = ListState::default();
        if !issues.is_empty() {
            state.select(Some(initial_index.min(issues.len() - 1)));
        }
        Self { issues, state, total, show_help: false }
    }

    pub fn next(&mut self) {
        if self.issues.is_empty() { return; }
        let i = match self.state.selected() {
            Some(i) => (i + 1) % self.issues.len(),
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.issues.is_empty() { return; }
        let i = match self.state.selected() {
            Some(0) => self.issues.len() - 1,
            Some(i) => i - 1,
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn selected(&self) -> Option<&Issue> {
        self.state.selected().and_then(|i| self.issues.get(i))
    }
}

/// Run the TUI. Returns an action to take after the TUI exits.
/// `initial_index`: which row to pre-select (used when returning from detail view).
/// `custom_fields`: list of custom fields to show in preview/detail (from config).
pub fn run_tui(issues: &[Issue], total: u32, initial_index: usize, custom_fields: &[crate::config::CustomField]) -> io::Result<TuiAction> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut list = IssueList::new(issues, total, initial_index);
    let mut preview_clickmap: Vec<Option<String>> = Vec::new();
    let mut last_selected: Option<usize> = None;
    let mut search_mode = false;
    let mut search_query = String::new();
    let action;

    loop {
        // Rebuild preview click map when selection changes
        let cur = list.state.selected();
        if cur != last_selected {
            last_selected = cur;
            preview_clickmap = cur
                .and_then(|i| issues.get(i))
                .map(|iss| build_preview_click_map(iss, custom_fields))
                .unwrap_or_default();
        }

        let term_size = terminal.size()?;
        let left_w = (term_size.width as u32 * 45 / 100) as u16;
        terminal.draw(|f| draw(f, &mut list, &search_query, search_mode, custom_fields))?;

        match event::read()? {
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press { continue; }

                if list.show_help {
                    list.show_help = false;
                    continue;
                }

                if search_mode {
                    match key.code {
                        KeyCode::Esc => {
                            search_mode = false;
                            search_query.clear();
                        }
                        KeyCode::Enter => {
                            if !search_query.is_empty() {
                                action = TuiAction::Search(search_query.clone());
                                break;
                            }
                            search_mode = false;
                        }
                        KeyCode::Backspace => { search_query.pop(); }
                        KeyCode::Char(c) => { search_query.push(c); }
                        _ => {}
                    }
                    continue;
                }

                match (key.modifiers, key.code) {
                    (_, KeyCode::Char('q')) | (_, KeyCode::Esc) => {
                        action = TuiAction::Quit;
                        break;
                    }
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                        action = TuiAction::Quit;
                        break;
                    }
                    (_, KeyCode::Down) | (_, KeyCode::Char('j')) => list.next(),
                    (_, KeyCode::Up)   | (_, KeyCode::Char('k')) => list.previous(),
                    (_, KeyCode::Char('g')) => { list.state.select(Some(0)); }
                    (_, KeyCode::Char('G')) => {
                        if !list.issues.is_empty() {
                            list.state.select(Some(list.issues.len() - 1));
                        }
                    }
                    (_, KeyCode::Enter) => {
                        if let Some(issue) = list.selected() {
                            action = TuiAction::Browse(issue.key.clone());
                            break;
                        }
                    }
                    (_, KeyCode::Char('v')) => {
                        if let Some(idx) = list.state.selected() {
                            if let Some(issue) = list.issues.get(idx) {
                                action = TuiAction::Detail(issue.key.clone());
                                break;
                            }
                        }
                    }
                    (_, KeyCode::Char('/')) => { search_mode = true; }
                    (_, KeyCode::Char('?')) => { list.show_help = true; }
                    _ => {}
                }
            }
            Event::Mouse(me) => {
                if me.kind == MouseEventKind::Down(MouseButton::Left) {
                    let col = me.column;
                    let row = me.row;
                    if col > left_w {
                        // Click in preview panel: row 0 is border, content starts at row 1
                        let line_idx = row.saturating_sub(1) as usize;
                        if let Some(Some(key)) = preview_clickmap.get(line_idx) {
                            action = TuiAction::Browse(key.clone());
                            break;
                        }
                    } else {
                        // Click in list panel: select the clicked row (row 1 = first item)
                        let item_idx = row.saturating_sub(1) as usize;
                        if item_idx < issues.len() {
                            list.state.select(Some(item_idx));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(action)
}

fn draw(f: &mut ratatui::Frame, list: &mut IssueList, search_query: &str, search_mode: bool, custom_fields: &[crate::config::CustomField]) {
    let area = f.size();

    let search_visible = !search_query.is_empty() || search_mode;
    let (main_area, search_area) = if search_visible {
        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(3)])
            .split(area);
        (v[0], Some(v[1]))
    } else {
        (area, None)
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(main_area);

    draw_list(f, list, chunks[0]);
    draw_detail(f, list, chunks[1], custom_fields);

    if let Some(sa) = search_area {
        let cursor = if search_mode { "█" } else { "" };
        let bar_title = if search_mode { " Search (Enter to search, Esc to cancel) " } else { " Search active " };
        let search_bar = Paragraph::new(format!("/{}{}", search_query, cursor))
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title(bar_title));
        f.render_widget(search_bar, sa);
    }

    if list.show_help {
        draw_help(f);
    }
}

fn draw_list(f: &mut ratatui::Frame, list: &mut IssueList, area: Rect) {
    let total_str = if list.total > list.issues.len() as u32 {
        format!(" [{}/{}]", list.issues.len(), list.total)
    } else {
        format!(" [{}]", list.issues.len())
    };

    let items: Vec<ListItem> = list.issues.iter().map(|issue| {
        let f = &issue.fields;
        let itype = f.issue_type.as_ref().map(|t| t.name.as_str()).unwrap_or("?");
        let status = f.status.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
        let summary = if f.summary.len() > 35 {
            format!("{}…", &f.summary[..34])
        } else {
            f.summary.clone()
        };

        let type_color = issue_type_color(itype);
        let status_color = status_color(status);

        let line = Line::from(vec![
            Span::styled(format!("{:<7}", itype), Style::default().fg(type_color)),
            Span::raw(" "),
            Span::styled(format!("{:<12}", issue.key), Style::default().fg(Color::Cyan)),
            Span::raw(" "),
            Span::raw(format!("{:<35}", summary)),
            Span::raw(" "),
            Span::styled(format!("{:<12}", status), Style::default().fg(status_color)),
        ]);
        ListItem::new(line)
    }).collect();

    let list_widget = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Issues{} — [/] search  [↑↓/jk] navigate  [Enter] browser  [v] detail  [?] help  [q] quit", total_str))
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list_widget, area, &mut list.state);
}

fn draw_detail(f: &mut ratatui::Frame, list: &IssueList, area: Rect, custom_fields: &[crate::config::CustomField]) {
    let Some(issue) = list.selected() else {
        let p = Paragraph::new("Select an issue to preview details.\n\nPress Enter to open in browser.\nPress v to open full detail view.")
            .block(Block::default().borders(Borders::ALL).title("Preview"))
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, area);
        return;
    };

    let flds = &issue.fields;
    let type_name = flds.issue_type.as_ref().map(|t| t.name.as_str()).unwrap_or("Issue");
    let status    = flds.status.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
    let priority  = flds.priority.as_ref().map(|p| p.name.as_str()).unwrap_or("-");
    let assignee  = flds.assignee.as_ref().map(|u| u.display_name.as_str()).unwrap_or("Unassigned");
    let reporter  = flds.reporter.as_ref().map(|u| u.display_name.as_str()).unwrap_or("-");
    let created   = flds.created.as_deref().map(format_date).unwrap_or_default();
    let updated   = flds.updated.as_deref().map(format_date).unwrap_or_default();

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(type_name, Style::default().fg(issue_type_color(type_name)).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(&issue.key, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(Span::styled("─".repeat(area.width.saturating_sub(2) as usize), Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(&flds.summary, Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Status:     ", Style::default().fg(Color::LightBlue)),
            Span::styled(status, Style::default().fg(status_color(status))),
        ]),
        Line::from(vec![
            Span::styled("  Priority:   ", Style::default().fg(Color::LightBlue)),
            Span::styled(priority, Style::default().fg(priority_color(priority))),
        ]),
        Line::from(vec![
            Span::styled("  Assignee:   ", Style::default().fg(Color::LightBlue)),
            Span::styled(assignee, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("  Reporter:   ", Style::default().fg(Color::LightBlue)),
            Span::raw(reporter),
        ]),
        Line::from(vec![
            Span::styled("  Created:    ", Style::default().fg(Color::LightBlue)),
            Span::raw(created),
        ]),
        Line::from(vec![
            Span::styled("  Updated:    ", Style::default().fg(Color::LightBlue)),
            Span::raw(updated),
        ]),
    ];

    if let Some(res) = &flds.resolution {
        lines.push(Line::from(vec![
            Span::styled("  Resolution: ", Style::default().fg(Color::LightBlue)),
            Span::raw(&res.name),
        ]));
    }
    if !flds.labels.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  Labels:     ", Style::default().fg(Color::LightBlue)),
            Span::raw(flds.labels.join(", ")),
        ]));
    }
    for cf in custom_fields {
        if let Some(val) = flds.custom.get(&cf.id) {
            let text = if cf.markdown {
                crate::output::format_field_value(val)
            } else {
                crate::output::adf_to_text(val)
            };
            if !text.trim().is_empty() {
                let preview_limit = cf.lines.unwrap_or(5);
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(format!("{}:", cf.name), Style::default().fg(Color::LightBlue))));
                for l in text.lines().take(preview_limit) {
                    lines.push(Line::from(format!("  {}", l)));
                }
            }
        }
    }
    if let Some(subtasks) = &flds.subtasks {
        if !subtasks.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("  Sub-tasks ({}):", subtasks.len()),
                Style::default().fg(Color::DarkGray),
            )));
            for st in subtasks {
                let st_status = st.fields.status.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(format!("{:<14}", st.key), Style::default().fg(Color::Cyan)),
                    Span::styled(st_status, Style::default().fg(status_color(st_status))),
                ]));
            }
        }
    }
    if let Some(desc) = &flds.description {
        let text = crate::output::adf_to_text(desc);
        if !text.trim().is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Description:", Style::default().fg(Color::LightBlue))));
            for l in text.lines().take(15) {
                lines.push(Line::from(format!("  {}", l)));
            }
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Preview"));

    f.render_widget(paragraph, area);
}

fn draw_help(f: &mut ratatui::Frame) {
    let area = f.size();
    let w = 44u16;
    let h = 17u16;
    let x = area.width.saturating_sub(w) / 2;
    let y = area.height.saturating_sub(h) / 2;
    let popup = Rect::new(x, y, w, h);

    let help_text = vec![
        Line::from(""),
        Line::from(vec![Span::styled("  Navigation", Style::default().add_modifier(Modifier::BOLD))]),
        Line::from("  ↑ / k       Move up"),
        Line::from("  ↓ / j       Move down"),
        Line::from("  g           Jump to top"),
        Line::from("  G           Jump to bottom"),
        Line::from(""),
        Line::from(vec![Span::styled("  Actions", Style::default().add_modifier(Modifier::BOLD))]),
        Line::from("  /           Search issues (JQL text)"),
        Line::from("  Enter       Open in browser"),
        Line::from("  v           View full detail"),
        Line::from("  q / Esc     Quit"),
        Line::from("  ?           Toggle this help"),
        Line::from(""),
        Line::from(vec![Span::styled("  Press any key to close", Style::default().fg(Color::DarkGray))]),
    ];

    f.render_widget(Clear, popup);
    f.render_widget(
        Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title(" Help "))
            .style(Style::default().bg(Color::DarkGray)),
        popup,
    );
}

/// Show a single issue in a full-screen detail TUI.
/// `comments_limit`: None = show all comments, Some(n) = show most-recent n.
/// `children`: work items (child issues fetched via JQL, not sub-tasks).
/// `back_index`: the list row to restore when the user presses q/Esc (Back).
pub fn run_issue_view(issue: &Issue, children: &[Issue], comments_limit: Option<usize>, back_index: usize, custom_fields: &[crate::config::CustomField]) -> io::Result<TuiAction> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let clickmap = build_click_map(issue, children, comments_limit, custom_fields);
    let mut scroll: u16 = 0;
    let action;
    loop {
        terminal.draw(|f| draw_single_issue(f, issue, children, comments_limit, scroll, custom_fields))?;

        match event::read()? {
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press { continue; }
                match (key.modifiers, key.code) {
                    (_, KeyCode::Char('q')) | (_, KeyCode::Esc) => {
                        action = TuiAction::Back(back_index);
                        break;
                    }
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                        action = TuiAction::Back(back_index);
                        break;
                    }
                    (_, KeyCode::Enter) => {
                        action = TuiAction::Browse(issue.key.clone());
                        break;
                    }
                    (_, KeyCode::Down) | (_, KeyCode::Char('j')) => { scroll = scroll.saturating_add(1); }
                    (_, KeyCode::Up)   | (_, KeyCode::Char('k')) => { scroll = scroll.saturating_sub(1); }
                    (_, KeyCode::Char('g')) => { scroll = 0; }
                    _ => {}
                }
            }
            Event::Mouse(me) => {
                if me.kind == MouseEventKind::Down(MouseButton::Left) {
                    // row 0 is the border; content starts at row 1
                    let line_idx = (me.row as usize).saturating_sub(1) + scroll as usize;
                    if let Some(Some(key)) = clickmap.get(line_idx) {
                        action = TuiAction::Browse(key.clone());
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(action)
}

fn draw_single_issue(f: &mut ratatui::Frame, issue: &Issue, children: &[Issue], comments_limit: Option<usize>, scroll: u16, custom_fields: &[crate::config::CustomField]) {
    let area = f.size();
    let flds = &issue.fields;
    let type_name = flds.issue_type.as_ref().map(|t| t.name.as_str()).unwrap_or("Issue");
    let status    = flds.status.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
    let priority  = flds.priority.as_ref().map(|p| p.name.as_str()).unwrap_or("-");
    let assignee  = flds.assignee.as_ref().map(|u| u.display_name.as_str()).unwrap_or("Unassigned");
    let reporter  = flds.reporter.as_ref().map(|u| u.display_name.as_str()).unwrap_or("-");
    let created   = flds.created.as_deref().map(format_date).unwrap_or_default();
    let updated   = flds.updated.as_deref().map(format_date).unwrap_or_default();

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(type_name, Style::default().fg(issue_type_color(type_name)).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(&issue.key, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(Span::styled("─".repeat(area.width.saturating_sub(2) as usize), Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(&flds.summary, Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Status:     ", Style::default().fg(Color::LightBlue)),
            Span::styled(status, Style::default().fg(status_color(status))),
        ]),
        Line::from(vec![
            Span::styled("  Priority:   ", Style::default().fg(Color::LightBlue)),
            Span::styled(priority, Style::default().fg(priority_color(priority))),
        ]),
        Line::from(vec![
            Span::styled("  Assignee:   ", Style::default().fg(Color::LightBlue)),
            Span::styled(assignee, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("  Reporter:   ", Style::default().fg(Color::LightBlue)),
            Span::raw(reporter),
        ]),
        Line::from(vec![
            Span::styled("  Created:    ", Style::default().fg(Color::LightBlue)),
            Span::raw(created),
        ]),
        Line::from(vec![
            Span::styled("  Updated:    ", Style::default().fg(Color::LightBlue)),
            Span::raw(updated),
        ]),
    ];

    if let Some(res) = &flds.resolution {
        lines.push(Line::from(vec![
            Span::styled("  Resolution: ", Style::default().fg(Color::LightBlue)),
            Span::raw(&res.name),
        ]));
    }
    if !flds.labels.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  Labels:     ", Style::default().fg(Color::LightBlue)),
            Span::raw(flds.labels.join(", ")),
        ]));
    }
    if let Some(subtasks) = &flds.subtasks {
        if !subtasks.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Sub-tasks ({}):", subtasks.len()),
                Style::default().fg(Color::DarkGray),
            )));
            for st in subtasks {
                let st_status = st.fields.status.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{:<14}", st.key), Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{:<40}  ", if st.fields.summary.len() > 40 {
                        format!("{}…", &st.fields.summary[..39])
                    } else {
                        st.fields.summary.clone()
                    })),
                    Span::styled(st_status, Style::default().fg(status_color(st_status))),
                ]));
            }
        }
    }
    // Work items (child issues fetched via JQL: parent = KEY or Epic Link = KEY)
    if !children.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Work Items ({}):", children.len()),
            Style::default().fg(Color::LightGreen),
        )));
        for child in children {
            let itype = child.fields.issue_type.as_ref().map(|t| t.name.as_str()).unwrap_or("?");
            let child_status = child.fields.status.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
            let assignee = child.fields.assignee.as_ref().map(|u| u.display_name.as_str()).unwrap_or("Unassigned");
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<8}", itype), Style::default().fg(issue_type_color(itype))),
                Span::styled(format!("{:<14}", child.key), Style::default().fg(Color::Cyan)),
                Span::raw(format!("{:<40}  ", if child.fields.summary.len() > 40 {
                    format!("{}…", &child.fields.summary[..39])
                } else {
                    child.fields.summary.clone()
                })),
                Span::styled(format!("{:<14}", child_status), Style::default().fg(status_color(child_status))),
                Span::styled(assignee.to_string(), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }
    // Linked issues (e.g. "blocks", "is blocked by", "relates to")
    if !flds.issue_links.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Linked Issues ({}):", flds.issue_links.len()),
            Style::default().fg(Color::LightMagenta),
        )));
        for link in &flds.issue_links {
            let (rel, linked) = if let Some(inward) = &link.inward_issue {
                (link.link_type.inward.as_str(), inward)
            } else if let Some(outward) = &link.outward_issue {
                (link.link_type.outward.as_str(), outward)
            } else {
                continue;
            };
            let itype = linked.fields.issue_type.as_ref().map(|t| t.name.as_str()).unwrap_or("?");
            let link_status = linked.fields.status.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
            let summary = &linked.fields.summary;
            let summary_display = if summary.len() > 40 {
                format!("{}…", &summary[..39])
            } else {
                summary.clone()
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<14}", rel), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<8}", itype), Style::default().fg(issue_type_color(itype))),
                Span::styled(format!("{:<14}", linked.key), Style::default().fg(Color::Cyan)),
                Span::raw(format!("{:<42}", summary_display)),
                Span::styled(link_status, Style::default().fg(status_color(link_status))),
            ]));
        }
    }
    for cf in custom_fields {
        if let Some(val) = flds.custom.get(&cf.id) {
            let text = if cf.markdown {
                crate::output::format_field_value(val)
            } else {
                crate::output::adf_to_text(val)
            };
            if !text.trim().is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(format!("{}:", cf.name), Style::default().fg(Color::LightBlue))));
                let iter: Box<dyn Iterator<Item = &str>> = if let Some(limit) = cf.lines {
                    Box::new(text.lines().take(limit))
                } else {
                    Box::new(text.lines())
                };
                for l in iter {
                    lines.push(Line::from(format!("  {}", l)));
                }
            }
        }
    }
    if let Some(desc) = &flds.description {
        let text = crate::output::adf_to_text(desc);
        if !text.trim().is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Description:", Style::default().fg(Color::LightBlue))));
            for l in text.lines() {
                lines.push(Line::from(format!("  {}", l)));
            }
        }
    }

    // Comments section
    if let Some(cf) = &flds.comment {
        let all = &cf.comments;
        let shown: &[_] = match comments_limit {
            Some(n) => {
                let start = all.len().saturating_sub(n);
                &all[start..]
            }
            None => all,
        };
        if !shown.is_empty() {
            let total = cf.total;
            let label = if comments_limit.is_some() || shown.len() < total as usize {
                format!("Comments ({} of {})", shown.len(), total)
            } else {
                format!("Comments ({})", total)
            };
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(label, Style::default().fg(Color::DarkGray))));
            lines.push(Line::from(Span::styled("─".repeat(area.width.saturating_sub(2) as usize), Style::default().fg(Color::DarkGray))));

            for comment in shown {
                let author = comment.author.as_ref().map(|u| u.display_name.as_str()).unwrap_or("?");
                let date   = comment.created.as_deref().map(format_date).unwrap_or_default();
                lines.push(Line::from(vec![
                    Span::styled(author, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("  {}", date), Style::default().fg(Color::DarkGray)),
                ]));
                if let Some(body) = &comment.body {
                    let text = crate::output::adf_to_text(body);
                    for l in text.lines() {
                        lines.push(Line::from(format!("  {}", l)));
                    }
                }
                lines.push(Line::from(""));
            }
        }
    }

    let title = format!(" {} — [↑↓/jk] scroll  [Enter] browser  [g] top  [q/Esc] back to list ", issue.key);
    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .scroll((scroll, 0));
    f.render_widget(paragraph, area);
}


fn issue_type_color(name: &str) -> Color {
    match name.to_lowercase().as_str() {
        "bug"               => Color::Red,
        "story"             => Color::Green,
        "task"              => Color::Blue,
        "epic"              => Color::Magenta,
        "subtask"|"sub-task"=> Color::Cyan,
        _                   => Color::White,
    }
}

fn status_color(name: &str) -> Color {
    let lower = name.to_lowercase();
    if lower.contains("done") || lower.contains("closed") || lower.contains("resolved") {
        Color::Green
    } else if lower.contains("progress") || lower.contains("review") {
        Color::Yellow
    } else if lower.contains("blocked") {
        Color::Red
    } else {
        Color::White
    }
}

fn priority_color(name: &str) -> Color {
    match name.to_lowercase().as_str() {
        "highest" | "critical" => Color::Red,
        "high"   => Color::LightRed,
        "medium" => Color::Yellow,
        "low"    => Color::Blue,
        _        => Color::White,
    }
}

/// Build a click map for the full-screen detail view.
/// Each entry corresponds to a line in draw_single_issue; Some(key) = clickable ticket.
fn build_click_map(issue: &Issue, children: &[Issue], comments_limit: Option<usize>, custom_fields: &[crate::config::CustomField]) -> Vec<Option<String>> {
    let flds = &issue.fields;
    let mut cm: Vec<Option<String>> = Vec::new();

    // Fixed header lines (lines 0-9)
    cm.extend([None, None, None, None, None, None, None, None, None, None]); // type/sep/summary/empty/status/priority/assignee/reporter/created/updated

    if flds.resolution.is_some()    { cm.push(None); }
    if !flds.labels.is_empty()      { cm.push(None); }

    if let Some(subtasks) = &flds.subtasks {
        if !subtasks.is_empty() {
            cm.push(None); // empty line
            cm.push(None); // "Sub-tasks (N):" header
            for st in subtasks { cm.push(Some(st.key.clone())); }
        }
    }
    if !children.is_empty() {
        cm.push(None); // empty line
        cm.push(None); // "Work Items (N):" header
        for child in children { cm.push(Some(child.key.clone())); }
    }
    if !flds.issue_links.is_empty() {
        cm.push(None); // empty line
        cm.push(None); // "Linked Issues (N):" header
        for link in &flds.issue_links {
            let linked_key = link.inward_issue.as_ref().map(|i| i.key.clone())
                .or_else(|| link.outward_issue.as_ref().map(|i| i.key.clone()));
            cm.push(linked_key);
        }
    }
    for cf in custom_fields {
        if let Some(val) = flds.custom.get(&cf.id) {
            let text = if cf.markdown { crate::output::format_field_value(val) } else { crate::output::adf_to_text(val) };
            if !text.trim().is_empty() {
                cm.push(None); // empty
                cm.push(None); // field label
                let count = if let Some(limit) = cf.lines { text.lines().take(limit).count() } else { text.lines().count() };
                for _ in 0..count { cm.push(None); }
            }
        }
    }
    if let Some(desc) = &flds.description {
        let text = crate::output::adf_to_text(desc);
        if !text.trim().is_empty() {
            cm.push(None); // empty
            cm.push(None); // "Description:"
            for _ in text.lines() { cm.push(None); }
        }
    }
    if let Some(cf) = &flds.comment {
        let all = &cf.comments;
        let shown: &[_] = match comments_limit {
            Some(n) => { let s = all.len().saturating_sub(n); &all[s..] }
            None => all,
        };
        if !shown.is_empty() {
            cm.push(None); // empty
            cm.push(None); // "Comments (N)" header
            cm.push(None); // separator
            for comment in shown {
                cm.push(None); // author + date
                if let Some(body) = &comment.body {
                    let text = crate::output::adf_to_text(body);
                    for _ in text.lines() { cm.push(None); }
                }
                cm.push(None); // empty line after comment
            }
        }
    }
    cm
}

/// Open a URL in the default browser without blocking the TUI (fire-and-forget).
pub fn open_url_background(url: &str) -> std::io::Result<()> {
    let url = url.to_string();
    if let Ok(browser) = std::env::var("JIRA_BROWSER").or_else(|_| std::env::var("BROWSER")) {
        let mut parts = browser.split_whitespace();
        if let Some(bin) = parts.next() {
            let mut cmd = std::process::Command::new(bin);
            for arg in parts { cmd.arg(arg); }
            cmd.arg(&url).spawn()?;
            return Ok(());
        }
    }
    if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(&url).spawn()?;
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd").args(["/C", "start", &url]).spawn()?;
    } else {
        std::process::Command::new("xdg-open").arg(&url).spawn()?;
    }
    Ok(())
}
fn build_preview_click_map(issue: &Issue, custom_fields: &[crate::config::CustomField]) -> Vec<Option<String>> {
    let flds = &issue.fields;
    let mut cm: Vec<Option<String>> = Vec::new();

    // type/sep/summary/empty/status/priority/assignee/reporter/created/updated
    cm.extend([None, None, None, None, None, None, None, None, None, None]);

    if flds.resolution.is_some()    { cm.push(None); }
    if !flds.labels.is_empty()      { cm.push(None); }

    for cf in custom_fields {
        if let Some(val) = flds.custom.get(&cf.id) {
            let text = if cf.markdown { crate::output::format_field_value(val) } else { crate::output::adf_to_text(val) };
            if !text.trim().is_empty() {
                cm.push(None); // empty
                cm.push(None); // field label
                let preview_limit = cf.lines.unwrap_or(5);
                for _ in text.lines().take(preview_limit) { cm.push(None); }
            }
        }
    }
    if let Some(subtasks) = &flds.subtasks {
        if !subtasks.is_empty() {
            cm.push(None); // "Sub-tasks (N):" header (preview has no leading empty)
            for st in subtasks { cm.push(Some(st.key.clone())); }
        }
    }
    if let Some(desc) = &flds.description {
        let text = crate::output::adf_to_text(desc);
        if !text.trim().is_empty() {
            cm.push(None); // empty
            cm.push(None); // "Description:"
            for _ in text.lines().take(15) { cm.push(None); }
        }
    }
    cm
}
