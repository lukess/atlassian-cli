use std::io;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, MouseButton, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;

use crate::client::confluence::{Page, Space};
use crate::config::Config;
use crate::output::adf_to_text;

// ── TUI action ─────────────────────────────────────────────────────────────────

pub enum PageTuiAction {
    Browse { space_key: String, page_id: String },
    View(String),
    Search(String),
    Quit,
}

// ── Plain output ───────────────────────────────────────────────────────────────

pub fn print_spaces_table(spaces: &[Space], total: u32) {
    use colored::Colorize;
    println!(
        "{:<12} {:<40} {:<10}  {}",
        "KEY".bold(),
        "NAME".bold(),
        "TYPE".bold(),
        "DESCRIPTION".bold()
    );
    println!("{}", "─".repeat(100).dimmed());
    for sp in spaces {
        let desc = sp
            .description
            .as_ref()
            .and_then(|d| d.plain.as_ref())
            .map(|p| p.value.replace('\n', " "))
            .unwrap_or_default();
        let desc_short = if desc.len() > 55 {
            format!("{}…", &desc[..54])
        } else {
            desc
        };
        println!(
            "{:<12} {:<40} {:<10}  {}",
            sp.key.cyan(),
            &sp.name,
            sp.space_type.dimmed(),
            desc_short
        );
    }
    println!();
    println!("{} spaces", total);
}

pub fn print_pages_table(pages: &[Page], total: u32) {
    use colored::Colorize;
    println!(
        "{:<12} {:<12} {:<5} {:<20} {}",
        "ID".bold(),
        "SPACE".bold(),
        "VER".bold(),
        "MODIFIED BY".bold(),
        "TITLE".bold()
    );
    println!("{}", "─".repeat(100).dimmed());
    for page in pages {
        let space_key = page.space.as_ref().map(|s| s.key.as_str()).unwrap_or("-");
        let ver = page
            .version
            .as_ref()
            .map(|v| v.number.to_string())
            .unwrap_or_else(|| "-".to_string());
        let author = page
            .version
            .as_ref()
            .and_then(|v| v.by.as_ref())
            .map(|b| b.display_name.as_str())
            .unwrap_or("-");
        let author_short = if author.len() > 18 {
            format!("{}…", &author[..17])
        } else {
            author.to_string()
        };
        println!(
            "{:<12} {:<12} {:<5} {:<20} {}",
            page.id.as_str(),
            space_key.cyan(),
            ver,
            author_short,
            &page.title,
        );
    }
    println!();
    println!("{} pages", total);
}

pub fn print_page_detail(page: &Page, children: &[Page]) {
    use colored::Colorize;
    let space = page.space.as_ref().map(|s| s.name.as_str()).unwrap_or("-");
    let space_key = page.space.as_ref().map(|s| s.key.as_str()).unwrap_or("-");
    let ver = page
        .version
        .as_ref()
        .map(|v| v.number.to_string())
        .unwrap_or_else(|| "-".to_string());
    let author = page
        .version
        .as_ref()
        .and_then(|v| v.by.as_ref())
        .map(|b| b.display_name.as_str())
        .unwrap_or("-");
    let when = page
        .version
        .as_ref()
        .and_then(|v| v.when.as_deref())
        .map(crate::output::format_date)
        .unwrap_or_default();

    println!("{} ({})", page.title.bold(), page.id);
    println!("{}", "─".repeat(80).dimmed());
    println!("{}  {}", "Space:".cyan().bold(), format!("{} ({})", space, space_key));
    println!("{}  v{} — {} — {}", "Version:".cyan().bold(), ver, author, when);
    if !page.ancestors.is_empty() {
        let path: Vec<&str> = page.ancestors.iter().map(|a| a.title.as_str()).collect();
        println!("{}  {}", "Path:".cyan().bold(), path.join(" › "));
    }

    if !children.is_empty() {
        println!();
        println!("{}  ({} pages)", "Sub-pages:".cyan().bold(), children.len());
        for child in children {
            println!("  • {} ({})", child.title, child.id.dimmed());
        }
    }

    if let Some(body) = &page.body {
        if let Some(adf) = &body.atlas_doc_format {
            let adf_val = serde_json::from_str::<serde_json::Value>(&adf.value).unwrap_or(serde_json::Value::Null);
            let text = adf_to_text(&adf_val);
            if !text.trim().is_empty() {
                println!();
                println!("{}", text);
            }
        }
    }
}

// ── TUI: space list ────────────────────────────────────────────────────────────

pub fn run_space_list_tui(spaces: &[Space], total: u32, config: &Config) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut selected = 0usize;
    let result = loop {
        terminal.draw(|f| draw_space_list(f, spaces, total, selected))?;
        match event::read()? {
            Event::Key(key) => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                KeyCode::Up | KeyCode::Char('k') => {
                    if selected > 0 { selected -= 1; }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if selected + 1 < spaces.len() { selected += 1; }
                }
                KeyCode::Char('g') => selected = 0,
                KeyCode::Char('G') => selected = spaces.len().saturating_sub(1),
                KeyCode::Enter => {
                    if let Some(sp) = spaces.get(selected) {
                        // open space in browser: {server}/wiki/spaces/{key}
                        if let Some(base) = std::env::var("CONFLUENCE_BROWSER_SERVER")
                            .ok()
                            .or_else(|| config.server.clone())
                        {
                            let url = format!(
                                "{}/wiki/spaces/{}",
                                base.trim_end_matches('/'),
                                sp.key
                            );
                            let _ = crate::output::tui::open_url_background(&url);
                        }
                    }
                }
                _ => {}
            },
            Event::Mouse(me) => match me.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let row = me.row as usize;
                    // header + border = 2 lines offset
                    if row >= 2 && row - 2 < spaces.len() {
                        selected = row - 2;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    };

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}

fn draw_space_list(f: &mut ratatui::Frame, spaces: &[Space], total: u32, selected: usize) {
    let area = f.area();
    let title = format!(
        " Confluence Spaces ({}) — [↑↓/jk] navigate  [Enter] open  [q] quit ",
        total
    );

    let items: Vec<ListItem> = spaces
        .iter()
        .map(|sp| {
            let desc = sp
                .description
                .as_ref()
                .and_then(|d| d.plain.as_ref())
                .map(|p| p.value.replace('\n', " "))
                .unwrap_or_default();
            let desc_short = if desc.len() > 50 {
                format!("{}…", &desc[..49])
            } else {
                desc
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<12}", sp.key), Style::default().fg(Color::Cyan)),
                Span::raw(format!("{:<35} ", sp.name)),
                Span::styled(desc_short, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(selected));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut state);
}

// ── TUI: page list ─────────────────────────────────────────────────────────────

pub fn run_page_list_tui(
    pages_arc: Arc<Mutex<Vec<Page>>>,
    loading: Arc<AtomicBool>,
    _config: &Config,
) -> io::Result<PageTuiAction> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut selected = 0usize;
    let mut scroll_offset = 0usize;
    let mut search_mode = false;
    let mut search_query = String::new();
    let mut filtered_indices: Vec<usize> = vec![];
    let mut known_len = 0usize;

    let result = loop {
        // Snapshot visible window (lock held briefly, then released before draw)
        let (window, preview, total, is_loading) = {
            let pages = pages_arc.lock().unwrap();
            let n = pages.len();
            if n != known_len {
                known_len = n;
                filtered_indices = (0..n).collect();
            }
            let height = terminal.size().map(|s| s.height as usize).unwrap_or(40);
            let search_vis = search_mode;
            let list_h = height.saturating_sub(if search_vis { 5 } else { 2 });
            let window: Vec<Page> = filtered_indices[scroll_offset..]
                .iter().take(list_h)
                .filter_map(|&i| pages.get(i).cloned())
                .collect();
            let preview: Option<Page> = filtered_indices.get(selected)
                .and_then(|&i| pages.get(i)).cloned();
            (window, preview, n as u32, loading.load(Ordering::Relaxed))
        };

        terminal.draw(|f| {
            let local_sel = selected.saturating_sub(scroll_offset);
            draw_page_list(
                f, &window, filtered_indices.len(), total,
                local_sel, preview.as_ref(), &search_query, search_mode, is_loading,
            );
        })?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) => {
                if search_mode {
                    match key.code {
                        KeyCode::Esc => {
                            search_mode = false;
                            search_query.clear();
                            // Restore full list
                            let pages = pages_arc.lock().unwrap();
                            filtered_indices = (0..pages.len()).collect();
                            known_len = pages.len();
                            selected = 0; scroll_offset = 0;
                        }
                        KeyCode::Enter => {
                            if !search_query.is_empty() {
                                break Ok(PageTuiAction::Search(search_query.clone()));
                            }
                            search_mode = false;
                        }
                        KeyCode::Backspace => { search_query.pop(); }
                        KeyCode::Char(c) => { search_query.push(c); }
                        _ => {}
                    }
                } else {
                    let visible_len = filtered_indices.len();
                    let term_h = terminal.size()?.height as usize;
                    let search_vis = search_mode;
                    let list_h = term_h.saturating_sub(if search_vis { 5 } else { 2 });
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            break Ok(PageTuiAction::Quit);
                        }
                        KeyCode::Char('/') => { search_mode = true; }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if selected > 0 {
                                selected -= 1;
                                if selected < scroll_offset { scroll_offset = selected; }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if selected + 1 < visible_len {
                                selected += 1;
                                if selected >= scroll_offset + list_h {
                                    scroll_offset = selected + 1 - list_h;
                                }
                            }
                        }
                        KeyCode::Char('g') => { selected = 0; scroll_offset = 0; }
                        KeyCode::Char('G') => {
                            selected = visible_len.saturating_sub(1);
                            scroll_offset = selected.saturating_sub(list_h.saturating_sub(1));
                        }
                        KeyCode::Enter => {
                            let pages = pages_arc.lock().unwrap();
                            if let Some(&orig) = filtered_indices.get(selected) {
                                if let Some(page) = pages.get(orig) {
                                    let space_key = page.space.as_ref().map(|s| s.key.clone()).unwrap_or_default();
                                    break Ok(PageTuiAction::Browse { space_key, page_id: page.id.clone() });
                                }
                            }
                        }
                        KeyCode::Char('v') => {
                            let pages = pages_arc.lock().unwrap();
                            if let Some(&orig) = filtered_indices.get(selected) {
                                if let Some(page) = pages.get(orig) {
                                    break Ok(PageTuiAction::View(page.id.clone()));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::Mouse(me) => match me.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let row = me.row as usize;
                    if row >= 2 {
                        let clicked = scroll_offset + row - 2;
                        if clicked < filtered_indices.len() { selected = clicked; }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, crossterm::event::DisableMouseCapture)?;
    terminal.show_cursor()?;
    result
}

fn draw_page_list(
    f: &mut ratatui::Frame,
    window: &[Page],
    filtered_count: usize,
    total: u32,
    local_selected: usize,
    preview: Option<&Page>,
    search_query: &str,
    search_mode: bool,
    is_loading: bool,
) {
    let area = f.area();

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
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(main_area);

    let loading_indicator = if is_loading { " ⟳ loading…" } else { "" };
    let title = if total == 0 {
        format!(" Pages — [/] to search across all spaces  [q] quit ")
    } else if search_query.is_empty() {
        format!(" Pages ({}){} — [/] search  [↑↓/jk] nav  [Enter] browser  [v] view  [q] quit ", total, loading_indicator)
    } else {
        format!(" Pages ({}/{}){} — [Esc] clear  [↑↓/jk] nav  [Enter] browser  [v] view ", filtered_count, total, loading_indicator)
    };

    let items: Vec<ListItem> = window.iter().map(|page| {
        let space_key = page.space.as_ref().map(|s| s.key.as_str()).unwrap_or("-");
        ListItem::new(Line::from(vec![
            Span::styled(format!("{:<10}", space_key), Style::default().fg(Color::Cyan)),
            Span::raw(page.title.clone()),
        ]))
    }).collect();

    let mut state = ListState::default();
    state.select(Some(local_selected));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, chunks[0], &mut state);

    if let Some(page) = preview {
        draw_page_preview(f, page, chunks[1]);
    }

    if let Some(sa) = search_area {
        let cursor = if search_mode { "█" } else { "" };
        let bar_title = if search_mode { " Search (Enter to confirm, Esc to keep) " } else { " Filter active " };
        let search_bar = Paragraph::new(format!("/{}{}", search_query, cursor))
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title(bar_title));
        f.render_widget(search_bar, sa);
    }
}

fn draw_page_preview(f: &mut ratatui::Frame, page: &Page, area: Rect) {
    use crate::output::format_date;

    let space_name = page.space.as_ref().map(|s| s.name.as_str()).unwrap_or("-");
    let space_key = page.space.as_ref().map(|s| s.key.as_str()).unwrap_or("-");
    let ver = page
        .version
        .as_ref()
        .map(|v| v.number.to_string())
        .unwrap_or_else(|| "-".to_string());
    let author = page
        .version
        .as_ref()
        .and_then(|v| v.by.as_ref())
        .map(|b| b.display_name.as_str())
        .unwrap_or("-");
    let when = page
        .version
        .as_ref()
        .and_then(|v| v.when.as_deref())
        .map(format_date)
        .unwrap_or_default();

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(&page.title, Style::default().add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(
            "─".repeat(area.width.saturating_sub(2) as usize),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(vec![
            Span::styled("  Space:    ", Style::default().fg(Color::LightBlue)),
            Span::styled(
                format!("{} ({})", space_name, space_key),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Version:  ", Style::default().fg(Color::LightBlue)),
            Span::raw(format!("v{}", ver)),
        ]),
        Line::from(vec![
            Span::styled("  Author:   ", Style::default().fg(Color::LightBlue)),
            Span::styled(author, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("  Modified: ", Style::default().fg(Color::LightBlue)),
            Span::raw(when),
        ]),
        Line::from(vec![
            Span::styled("  ID:       ", Style::default().fg(Color::LightBlue)),
            Span::styled(&page.id, Style::default().fg(Color::Gray)),
        ]),
    ];

    // Show ancestors (breadcrumb path)
    if !page.ancestors.is_empty() {
        let path: Vec<&str> = page.ancestors.iter().map(|a| a.title.as_str()).collect();
        let breadcrumb = path.join(" › ");
        let breadcrumb = if breadcrumb.len() > area.width.saturating_sub(14) as usize {
            format!("…{}", &breadcrumb[breadcrumb.len().saturating_sub(area.width.saturating_sub(15) as usize)..])
        } else {
            breadcrumb
        };
        lines.push(Line::from(vec![
            Span::styled("  Path:     ", Style::default().fg(Color::LightBlue)),
            Span::styled(breadcrumb, Style::default().fg(Color::DarkGray)),
        ]));
    }

    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Preview "));
    f.render_widget(para, area);
}

// ── TUI: page full-screen view ─────────────────────────────────────────────────

pub fn run_page_view(page: &Page, children: &[Page], config: &Config) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let lines = build_page_lines(page, children);
    let total_lines = lines.len() as u16;
    let mut scroll: u16 = 0;

    let result = loop {
        let height = terminal.size()?.height.saturating_sub(2);
        terminal.draw(|f| draw_page_view(f, page, &lines, scroll))?;
        match event::read()? {
            Event::Key(key) => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                KeyCode::Up | KeyCode::Char('k') => {
                    scroll = scroll.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    scroll = scroll.saturating_add(1).min(total_lines.saturating_sub(height));
                }
                KeyCode::PageUp => {
                    scroll = scroll.saturating_sub(height);
                }
                KeyCode::PageDown => {
                    scroll = scroll.saturating_add(height).min(total_lines.saturating_sub(height));
                }
                KeyCode::Char('g') => scroll = 0,
                KeyCode::Char('G') => scroll = total_lines.saturating_sub(height),
                KeyCode::Enter => {
                    let space_key = page.space.as_ref().map(|s| s.key.as_str()).unwrap_or("");
                    if let Some(url) = config.confluence_browse_url(space_key, &page.id) {
                        let _ = crate::output::tui::open_url_background(&url);
                    }
                }
                _ => {}
            },
            Event::Mouse(me) => match me.kind {
                MouseEventKind::ScrollUp => { scroll = scroll.saturating_sub(3); }
                MouseEventKind::ScrollDown => {
                    scroll = scroll.saturating_add(3).min(total_lines.saturating_sub(height));
                }
                _ => {}
            },
            _ => {}
        }
    };

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}

fn build_page_lines(page: &Page, children: &[Page]) -> Vec<Line<'static>> {
    use crate::output::format_date;

    let space_name = page.space.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "-".to_string());
    let space_key = page.space.as_ref().map(|s| s.key.clone()).unwrap_or_else(|| "-".to_string());
    let ver = page
        .version
        .as_ref()
        .map(|v| v.number.to_string())
        .unwrap_or_else(|| "-".to_string());
    let author = page
        .version
        .as_ref()
        .and_then(|v| v.by.as_ref())
        .map(|b| b.display_name.clone())
        .unwrap_or_else(|| "-".to_string());
    let when = page
        .version
        .as_ref()
        .and_then(|v| v.when.as_deref())
        .map(format_date)
        .unwrap_or_default();

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            page.title.clone(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled("─".repeat(80), Style::default().fg(Color::DarkGray))),
        Line::from(vec![
            Span::styled("  Space:    ".to_string(), Style::default().fg(Color::LightBlue)),
            Span::styled(
                format!("{} ({})", space_name, space_key),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Version:  ".to_string(), Style::default().fg(Color::LightBlue)),
            Span::raw(format!("v{}", ver)),
        ]),
        Line::from(vec![
            Span::styled("  Author:   ".to_string(), Style::default().fg(Color::LightBlue)),
            Span::styled(author, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("  Modified: ".to_string(), Style::default().fg(Color::LightBlue)),
            Span::raw(when),
        ]),
    ];

    if !page.ancestors.is_empty() {
        let path: Vec<String> = page.ancestors.iter().map(|a| a.title.clone()).collect();
        lines.push(Line::from(vec![
            Span::styled("  Path:     ".to_string(), Style::default().fg(Color::LightBlue)),
            Span::styled(path.join(" › "), Style::default().fg(Color::DarkGray)),
        ]));
    }

    // Sub-pages
    if !children.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  Sub-pages ({}):", children.len()),
            Style::default().fg(Color::LightBlue),
        )));
        for child in children {
            lines.push(Line::from(vec![
                Span::raw("    • "),
                Span::styled(child.title.clone(), Style::default().fg(Color::Cyan)),
                Span::styled(format!("  ({})", child.id), Style::default().fg(Color::Gray)),
            ]));
        }
    }

    // Body
    if let Some(body) = &page.body {
        if let Some(adf) = &body.atlas_doc_format {
            let adf_val = serde_json::from_str::<serde_json::Value>(&adf.value).unwrap_or(serde_json::Value::Null);
            let text = adf_to_text(&adf_val);
            if !text.trim().is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "─".repeat(80),
                    Style::default().fg(Color::DarkGray),
                )));
                for l in text.lines() {
                    lines.push(Line::from(format!("  {}", l)));
                }
            }
        }
    }

    lines
}

fn draw_page_view(f: &mut ratatui::Frame, page: &Page, lines: &[Line<'static>], scroll: u16) {
    let area = f.area();
    let title = format!(
        " {} — [↑↓/jk] scroll  [PgUp/Dn] page  [Enter] browser  [g] top  [q] back ",
        page.id
    );
    let para = Paragraph::new(lines.to_vec())
        .block(Block::default().borders(Borders::ALL).title(title))
        .scroll((scroll, 0));
    f.render_widget(para, area);
}
