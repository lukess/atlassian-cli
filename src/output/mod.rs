pub mod table;
pub mod tui;
pub mod confluence;

use crate::client::{Issue, Project};
use colored::Colorize;
use std::io::IsTerminal;

/// Returns true if we should use TUI (interactive terminal, not piped)
pub fn is_tty() -> bool {
    std::io::stdout().is_terminal()
}

/// Format a date string from ISO 8601 to a short human-readable form
pub fn format_date(s: &str) -> String {
    // Jira dates: "2026-05-06T14:05:14.698-0700" → "2026-05-06 14:05:14 -0700"
    let date = s.get(..10).unwrap_or(s);
    let time = s.get(11..19).unwrap_or("");
    // Timezone offset is after the fractional seconds (e.g. ".698-0700" or ".698+0530")
    let tz = s.rfind('+').or_else(|| s.rfind('-').filter(|&i| i > 19))
        .map(|i| &s[i..])
        .unwrap_or("");
    if time.is_empty() {
        date.to_string()
    } else if tz.is_empty() {
        format!("{} {}", date, time)
    } else {
        format!("{} {} {}", date, time, tz)
    }
}

/// Extract plain text from Jira ADF (Atlassian Document Format) description
pub fn adf_to_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 { return (f as i64).to_string(); }
                return format!("{}", f);
            }
            n.to_string()
        }
        serde_json::Value::Array(arr) => {
            arr.iter().map(adf_to_text).collect::<Vec<_>>().join("")
        }
        _ => adf_node(value, 0),
    }
}

fn adf_node(node: &serde_json::Value, depth: usize) -> String {
    let node_type = node.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match node_type {
        "doc" => adf_children(node, depth),

        "paragraph" => {
            let inner = adf_children(node, depth);
            if inner.trim().is_empty() { String::new() } else { format!("{}\n", inner) }
        }

        "heading" => {
            let level = node.get("attrs").and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64()).unwrap_or(1) as usize;
            format!("{} {}\n", "#".repeat(level), adf_children(node, depth))
        }

        "bulletList" => adf_list(node, depth, false),
        "orderedList" => adf_list(node, depth, true),

        "listItem" => {
            // Content is typically one or more paragraphs/nested lists
            adf_children(node, depth)
        }

        "taskList" => {
            let indent = "  ".repeat(depth);
            node.get("content").and_then(|c| c.as_array())
                .map(|items| items.iter().map(|item| {
                    let state = item.get("attrs").and_then(|a| a.get("state"))
                        .and_then(|s| s.as_str()).unwrap_or("TODO");
                    let box_char = if state == "DONE" { "☑" } else { "☐" };
                    let text = adf_children(item, depth + 1).trim_end_matches('\n').to_string();
                    format!("{}{} {}\n", indent, box_char, text)
                }).collect::<Vec<_>>().join(""))
                .unwrap_or_default()
        }

        "taskItem" => adf_children(node, depth),

        "blockquote" => {
            adf_children(node, depth).lines()
                .map(|l| format!("│ {}", l))
                .collect::<Vec<_>>().join("\n") + "\n"
        }

        "codeBlock" => {
            format!("  {}\n", adf_children(node, depth).trim())
        }

        "rule" => "─────────────────\n".to_string(),
        "hardBreak" => "\n".to_string(),

        "text" => {
            let text = node.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string();
            // Check for a link mark — show as "text (url)" or just url if text == url
            if let Some(marks) = node.get("marks").and_then(|m| m.as_array()) {
                for mark in marks {
                    if mark.get("type").and_then(|t| t.as_str()) == Some("link") {
                        if let Some(href) = mark.get("attrs")
                            .and_then(|a| a.get("href")).and_then(|h| h.as_str())
                        {
                            return if text.trim() == href { href.to_string() }
                                   else { format!("{} ({})", text, href) };
                        }
                    }
                }
            }
            text
        }

        "inlineCard" => {
            node.get("attrs").and_then(|a| a.get("url"))
                .and_then(|u| u.as_str()).unwrap_or("").to_string()
        }

        "mention" => {
            let name = node.get("attrs")
                .and_then(|a| a.get("text").or_else(|| a.get("displayName")))
                .and_then(|t| t.as_str()).unwrap_or("mention");
            format!("@{}", name)
        }

        "emoji" => {
            node.get("attrs").and_then(|a| a.get("text"))
                .and_then(|t| t.as_str()).unwrap_or("").to_string()
        }

        "image" | "media" | "mediaSingle" | "mediaGroup" => {
            let alt = node.get("attrs").and_then(|a| a.get("alt"))
                .and_then(|t| t.as_str()).unwrap_or("attachment");
            format!("[{}]\n", alt)
        }

        // Unknown node types: just recurse into children
        _ => adf_children(node, depth),
    }
}

fn adf_children(node: &serde_json::Value, depth: usize) -> String {
    node.get("content").and_then(|c| c.as_array())
        .map(|arr| arr.iter().map(|c| adf_node(c, depth)).collect::<Vec<_>>().join(""))
        .unwrap_or_default()
}

fn adf_list(node: &serde_json::Value, depth: usize, ordered: bool) -> String {
    let indent = "  ".repeat(depth);
    node.get("content").and_then(|c| c.as_array())
        .map(|items| items.iter().enumerate().map(|(i, item)| {
            let bullet = if ordered { format!("{}. ", i + 1) } else { "• ".to_string() };
            // Render item children; strip trailing newline added by paragraph
            let text = adf_children(item, depth + 1).trim_end_matches('\n').to_string();
            // Handle nested lists: indent them further
            let lines: Vec<&str> = text.lines().collect();
            if lines.len() <= 1 {
                format!("{}{}{}\n", indent, bullet, text)
            } else {
                let first = lines[0];
                let rest = lines[1..].iter()
                    .map(|l| format!("{}  {}\n", indent, l))
                    .collect::<String>();
                format!("{}{}{}\n{}", indent, bullet, first, rest)
            }
        }).collect::<Vec<_>>().join(""))
        .unwrap_or_default()
}

/// Format any JSON field value as a human-readable string.
/// Used for non-ADF fields: numbers (Story Points), booleans, plain strings, arrays.
pub fn format_field_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 { return (f as i64).to_string(); }
                return format!("{}", f);
            }
            n.to_string()
        }
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            arr.iter().map(format_field_value).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
        }
        serde_json::Value::Object(obj) => {
            for key in &["name", "value", "displayName", "accountName"] {
                if let Some(s) = obj.get(*key).and_then(|v| v.as_str()) {
                    return s.to_string();
                }
            }
            adf_to_text(&serde_json::Value::Object(obj.clone()))
        }
    }
}

/// Color an issue type name
pub fn color_issue_type(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "bug" => name.red().bold().to_string(),
        "story" => name.green().to_string(),
        "task" => name.blue().to_string(),
        "epic" => name.magenta().to_string(),
        "subtask" | "sub-task" => name.cyan().to_string(),
        _ => name.white().to_string(),
    }
}

/// Color a status name
pub fn color_status(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("done") || lower.contains("closed") || lower.contains("resolved") {
        name.green().to_string()
    } else if lower.contains("progress") || lower.contains("review") {
        name.yellow().to_string()
    } else if lower.contains("blocked") {
        name.red().to_string()
    } else {
        name.white().to_string()
    }
}

/// Color a priority name
pub fn color_priority(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "highest" | "critical" => name.red().bold().to_string(),
        "high" => name.red().to_string(),
        "medium" => name.yellow().to_string(),
        "low" => name.blue().to_string(),
        "lowest" => name.cyan().to_string(),
        _ => name.normal().to_string(),
    }
}

/// Print detailed issue view
pub fn print_issue_detail(issue: &Issue) {
    let f = &issue.fields;
    let type_name = f.issue_type.as_ref().map(|t| t.name.as_str()).unwrap_or("Issue");
    let status = f.status.as_ref().map(|s| s.name.as_str()).unwrap_or("Unknown");
    let priority = f.priority.as_ref().map(|p| p.name.as_str()).unwrap_or("-");
    let assignee = f.assignee.as_ref().map(|u| u.display_name.as_str()).unwrap_or("Unassigned");
    let reporter = f.reporter.as_ref().map(|u| u.display_name.as_str()).unwrap_or("-");
    let created = f.created.as_deref().map(format_date).unwrap_or_default();
    let updated = f.updated.as_deref().map(format_date).unwrap_or_default();

    println!("{} {}", color_issue_type(type_name), issue.key.bold().white());
    println!("{}", "─".repeat(70).dimmed());
    println!("{}", f.summary.bold());
    println!();
    println!("  {:<12} {}", "Status:".dimmed(), color_status(status));
    println!("  {:<12} {}", "Priority:".dimmed(), color_priority(priority));
    println!("  {:<12} {}", "Assignee:".dimmed(), assignee.cyan().to_string());
    println!("  {:<12} {}", "Reporter:".dimmed(), reporter);
    println!("  {:<12} {}", "Created:".dimmed(), created);
    println!("  {:<12} {}", "Updated:".dimmed(), updated);

    if !f.labels.is_empty() {
        println!("  {:<12} {}", "Labels:".dimmed(), f.labels.join(", "));
    }
    if !f.components.is_empty() {
        let names: Vec<_> = f.components.iter().map(|c| c.name.as_str()).collect();
        println!("  {:<12} {}", "Components:".dimmed(), names.join(", "));
    }
    if !f.fix_versions.is_empty() {
        let names: Vec<_> = f.fix_versions.iter().map(|v| v.name.as_str()).collect();
        println!("  {:<12} {}", "Fix Versions:".dimmed(), names.join(", "));
    }
    if let Some(parent) = &f.parent {
        println!("  {:<12} {} - {}", "Parent:".dimmed(), parent.key.cyan().to_string(), parent.fields.summary);
    }

    if let Some(desc) = &f.description {
        let text = adf_to_text(desc);
        if !text.trim().is_empty() {
            println!();
            println!("{}", "Description:".dimmed());
            for line in text.lines() {
                println!("  {}", line);
            }
        }
    }

    if let Some(subtasks) = &f.subtasks {
        if !subtasks.is_empty() {
            println!();
            println!("{}", "Subtasks:".dimmed());
            for st in subtasks {
                let st_status = st.fields.status.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
                println!("  {} {} - {}", st.key.cyan().to_string(), color_status(st_status), st.fields.summary);
            }
        }
    }
}

/// Print projects list (plain)
pub fn print_projects_table(projects: &[Project]) {
    if projects.is_empty() {
        println!("No projects found.");
        return;
    }
    let key_w = projects.iter().map(|p| p.key.len()).max().unwrap_or(4).max(4);
    let name_w = projects.iter().map(|p| p.name.len()).max().unwrap_or(20).max(20).min(40);
    let type_w = 12;

    println!(
        "{}  {:<name_w$}  {}  {}",
        table::pad_colored(&"KEY".bold().white().to_string(), key_w),
        "NAME".bold().white(),
        table::pad_colored(&"TYPE".bold().white().to_string(), type_w),
        "LEAD".bold().white(),
        name_w = name_w,
    );
    println!("{}", "─".repeat(key_w + name_w + type_w + 30).dimmed());

    for p in projects {
        let lead = p.lead.as_ref().map(|u| u.display_name.as_str()).unwrap_or("-");
        let type_display = match p.project_type.as_str() {
            "software" => "software".cyan().to_string(),
            "business" => "business".yellow().to_string(),
            "service_desk" => "service-desk".magenta().to_string(),
            t => t.normal().to_string(),
        };
        println!(
            "{}  {:<name_w$}  {}  {}",
            table::pad_colored(&p.key.cyan().to_string(), key_w),
            truncate(&p.name, name_w),
            table::pad_colored(&type_display, type_w),
            lead,
            name_w = name_w,
        );
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_adf_bullet_and_inlinecard() {
        let doc = json!({
            "type": "doc", "version": 1,
            "content": [
                {"type": "paragraph", "content": [
                    {"type": "inlineCard", "attrs": {"url": "https://example.com/page"}}
                ]},
                {"type": "bulletList", "content": [
                    {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "review the report"}]}]},
                    {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "fix or false positive"}]}]},
                    {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "defend if we need more time"}]}]}
                ]}
            ]
        });
        let text = adf_to_text(&doc);
        println!("OUTPUT:\n{}", text);
        assert!(text.contains("https://example.com/page"), "should contain URL");
        assert!(text.contains("• review the report"), "should have bullet");
        assert!(text.contains("• fix or false positive"), "should have bullet");
    }

    #[test]
    fn test_adf_task_list() {
        let doc = json!({
            "type": "doc", "version": 1,
            "content": [{"type": "taskList", "content": [
                {"type": "taskItem", "attrs": {"state": "DONE"}, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "done task"}]}]},
                {"type": "taskItem", "attrs": {"state": "TODO"}, "content": [{"type": "paragraph", "content": [{"type": "text", "text": "todo task"}]}]}
            ]}]
        });
        let text = adf_to_text(&doc);
        println!("OUTPUT:\n{}", text);
        assert!(text.contains("☑ done task"), "should have checked checkbox");
        assert!(text.contains("☐ todo task"), "should have unchecked checkbox");
    }
}
