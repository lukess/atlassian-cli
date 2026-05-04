use crate::client::Issue;
use crate::output::{color_issue_type, color_priority, color_status, format_date, truncate};
use colored::Colorize;

pub const ALL_COLUMNS: &[&str] = &[
    "TYPE", "KEY", "SUMMARY", "STATUS", "ASSIGNEE", "REPORTER",
    "PRIORITY", "RESOLUTION", "CREATED", "UPDATED", "LABELS",
];

pub struct OutputOptions<'a> {
    pub no_headers: bool,
    pub no_truncate: bool,
    /// None = all columns
    pub columns: Option<Vec<&'a str>>,
    pub delimiter: &'a str,
    pub csv: bool,
}

impl<'a> Default for OutputOptions<'a> {
    fn default() -> Self {
        Self {
            no_headers: false,
            no_truncate: false,
            columns: None,
            delimiter: "\t",
            csv: false,
        }
    }
}

fn get_column_value(issue: &Issue, col: &str) -> String {
    let f = &issue.fields;
    match col.to_uppercase().as_str() {
        "TYPE"       => f.issue_type.as_ref().map(|t| t.name.clone()).unwrap_or_else(|| "-".to_string()),
        "KEY"        => issue.key.clone(),
        "SUMMARY"    => f.summary.clone(),
        "STATUS"     => f.status.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "-".to_string()),
        "ASSIGNEE"   => f.assignee.as_ref().map(|u| u.display_name.clone()).unwrap_or_else(|| "Unassigned".to_string()),
        "REPORTER"   => f.reporter.as_ref().map(|u| u.display_name.clone()).unwrap_or_else(|| "-".to_string()),
        "PRIORITY"   => f.priority.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "-".to_string()),
        "RESOLUTION" => f.resolution.as_ref().map(|r| r.name.clone()).unwrap_or_else(|| "-".to_string()),
        "CREATED"    => f.created.as_deref().map(format_date).unwrap_or_default(),
        "UPDATED"    => f.updated.as_deref().map(format_date).unwrap_or_default(),
        "LABELS"     => if f.labels.is_empty() { "-".to_string() } else { f.labels.join(",") },
        _            => String::new(),
    }
}

/// CSV-quote a field if needed
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn print_issues_table(issues: &[Issue], total: u32, opts: &OutputOptions) {
    if issues.is_empty() {
        println!("{}", "No issues found.".dimmed());
        return;
    }

    let cols: Vec<&str> = opts.columns.as_deref()
        .unwrap_or(ALL_COLUMNS)
        .to_vec();

    // CSV mode
    if opts.csv {
        if !opts.no_headers {
            let header = cols.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(",");
            println!("{}", header);
        }
        for issue in issues {
            let row = cols.iter()
                .map(|c| csv_field(&get_column_value(issue, c)))
                .collect::<Vec<_>>()
                .join(",");
            println!("{}", row);
        }
        if total > issues.len() as u32 {
            eprintln!("Showing {}/{} issues.", issues.len(), total);
        }
        return;
    }

    // Delimiter mode — no padding, no colors (when user explicitly passes --delimiter)
    let plain_delimited = opts.delimiter != "\t";

    if plain_delimited {
        if !opts.no_headers {
            let header = cols.iter().copied().collect::<Vec<_>>().join(opts.delimiter);
            println!("{}", header);
        }
        for issue in issues {
            let row = cols.iter()
                .map(|c| get_column_value(issue, c))
                .collect::<Vec<_>>()
                .join(opts.delimiter);
            println!("{}", row);
        }
        if total > issues.len() as u32 {
            eprintln!("Showing {}/{} issues.", issues.len(), total);
        }
        return;
    }

    // Default: colored fixed-width table (mirrors jira-cli 11-column layout)
    // When --columns is set but no special delimiter/csv, still use fixed-width but only for chosen cols.
    let type_w     = issues.iter().map(|i| i.fields.issue_type.as_ref().map(|t| t.name.len()).unwrap_or(4)).max().unwrap_or(4).max(4);
    let key_w      = issues.iter().map(|i| i.key.len()).max().unwrap_or(8).max(8);
    let sum_w      = if opts.no_truncate { issues.iter().map(|i| i.fields.summary.len()).max().unwrap_or(40).max(7) } else { 40usize };
    let status_w   = issues.iter().map(|i| i.fields.status.as_ref().map(|s| s.name.len()).unwrap_or(6)).max().unwrap_or(6).max(6);
    let assignee_w = if opts.no_truncate { issues.iter().map(|i| i.fields.assignee.as_ref().map(|u| u.display_name.len()).unwrap_or(10)).max().unwrap_or(8).max(8) } else { 15usize };
    let reporter_w = if opts.no_truncate { issues.iter().map(|i| i.fields.reporter.as_ref().map(|u| u.display_name.len()).unwrap_or(8)).max().unwrap_or(8).max(8) } else { 15usize };
    let pri_w      = 8usize;
    let res_w      = 10usize;
    let date_w     = 10usize;

    fn col_width(col: &str, type_w: usize, key_w: usize, sum_w: usize, status_w: usize,
                 assignee_w: usize, reporter_w: usize, pri_w: usize, res_w: usize, date_w: usize) -> usize {
        match col.to_uppercase().as_str() {
            "TYPE"       => type_w,
            "KEY"        => key_w,
            "SUMMARY"    => sum_w,
            "STATUS"     => status_w,
            "ASSIGNEE"   => assignee_w,
            "REPORTER"   => reporter_w,
            "PRIORITY"   => pri_w,
            "RESOLUTION" => res_w,
            "CREATED"    => date_w,
            "UPDATED"    => date_w,
            "LABELS"     => 6,
            _            => 10,
        }
    }

    if !opts.no_headers {
        let header_parts: Vec<String> = cols.iter().map(|c| {
            let w = col_width(c, type_w, key_w, sum_w, status_w, assignee_w, reporter_w, pri_w, res_w, date_w);
            if *c == "LABELS" {
                c.bold().white().to_string()
            } else {
                pad_colored(&c.bold().white().to_string(), w)
            }
        }).collect();
        println!("{}", header_parts.join("  "));

        let total_w: usize = cols.iter().map(|c| {
            col_width(c, type_w, key_w, sum_w, status_w, assignee_w, reporter_w, pri_w, res_w, date_w)
        }).sum::<usize>() + (cols.len().saturating_sub(1)) * 2;
        println!("{}", "─".repeat(total_w).dimmed());
    }

    for issue in issues {
        let f = &issue.fields;
        let row_parts: Vec<String> = cols.iter().map(|c| {
            let w = col_width(c, type_w, key_w, sum_w, status_w, assignee_w, reporter_w, pri_w, res_w, date_w);
            match c.to_uppercase().as_str() {
                "TYPE" => {
                    let v = f.issue_type.as_ref().map(|t| t.name.as_str()).unwrap_or("-");
                    pad_colored(&color_issue_type(v), w)
                }
                "KEY" => pad_colored(&issue.key.cyan().to_string(), w),
                "SUMMARY" => {
                    let s = if opts.no_truncate { f.summary.clone() } else { truncate(&f.summary, sum_w) };
                    pad_str(&s, w)
                }
                "STATUS" => {
                    let v = f.status.as_ref().map(|s| s.name.as_str()).unwrap_or("-");
                    pad_colored(&color_status(v), w)
                }
                "ASSIGNEE" => {
                    let v = f.assignee.as_ref().map(|u| u.display_name.as_str()).unwrap_or("Unassigned");
                    let s = if opts.no_truncate { v.to_string() } else { truncate(v, assignee_w) };
                    pad_str(&s, w)
                }
                "REPORTER" => {
                    let v = f.reporter.as_ref().map(|u| u.display_name.as_str()).unwrap_or("-");
                    let s = if opts.no_truncate { v.to_string() } else { truncate(v, reporter_w) };
                    pad_str(&s, w)
                }
                "PRIORITY" => {
                    let v = f.priority.as_ref().map(|p| p.name.as_str()).unwrap_or("-");
                    pad_colored(&color_priority(v), w)
                }
                "RESOLUTION" => {
                    let v = f.resolution.as_ref().map(|r| r.name.as_str()).unwrap_or("-");
                    pad_str(&truncate(v, res_w), w)
                }
                "CREATED" => {
                    let v = f.created.as_deref().map(format_date).unwrap_or_default();
                    pad_str(&v, w)
                }
                "UPDATED" => {
                    let v = f.updated.as_deref().map(format_date).unwrap_or_default();
                    pad_str(&v, w)
                }
                "LABELS" => {
                    if f.labels.is_empty() { "-".to_string() } else { f.labels.join(",") }
                }
                _ => String::new(),
            }
        }).collect();
        println!("{}", row_parts.join("  "));
    }

    if total > issues.len() as u32 {
        println!();
        println!("{}", format!("Showing {}/{} issues. Use --max-results to see more.", issues.len(), total).dimmed());
    }
}

/// Pad a colored string (with ANSI codes) to visual width
pub fn pad_colored(s: &str, width: usize) -> String {
    let visible_len = strip_ansi(s).len();
    if visible_len < width {
        format!("{}{}", s, " ".repeat(width - visible_len))
    } else {
        s.to_string()
    }
}

/// Pad a plain string to width
fn pad_str(s: &str, width: usize) -> String {
    format!("{:<width$}", s, width = width)
}

fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if in_escape {
            if c == 'm' { in_escape = false; }
        } else if c == '\x1b' {
            in_escape = true;
        } else {
            result.push(c);
        }
    }
    result
}
