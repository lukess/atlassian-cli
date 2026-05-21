mod cli;
mod client;
mod config;
mod output;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use colored::Colorize;

use cli::{Cli, Commands, JiraCommands};
use cli::confluence::ConfluenceCommands;
use cli::confluence::{PageCommands, SpaceCommands};
use cli::epic::EpicCommands;
use cli::issue::IssueCommands;
use cli::issue::CommentCommands;
use cli::project::ProjectCommands;
use client::{Issue, JiraClient};
use client::confluence::ConfluenceClient;
use config::Config;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{} {}", "error:".red().bold(), e);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Jira { command } => handle_jira(command).await,
        Commands::Confluence { command } => handle_confluence(command).await,
        Commands::Issue { command } => {
            let config = Config::load()?;
            let jira = JiraClient::new(&config)?;
            handle_issue(command, &jira, &config).await
        }
        Commands::Completion { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "atlassian",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    }
}

async fn handle_jira(cmd: JiraCommands) -> Result<()> {
    let config = Config::load()?;
    let jira = JiraClient::new(&config)?;

    match cmd {
        JiraCommands::Issue { command } => handle_issue(command, &jira, &config).await,
        JiraCommands::Epic { command } => handle_epic(command, &jira, &config).await,
        JiraCommands::Project { command } => handle_project(command, &jira).await,
        JiraCommands::Me(args) => {
            let user = jira.myself().await?;
            if args.account_id {
                println!("{}", user.account_id);
            } else {
                println!("{}", user.display_name);
            }
            Ok(())
        }
    }
}

async fn handle_confluence(cmd: ConfluenceCommands) -> Result<()> {
    let config = Config::load()?;
    let confluence = ConfluenceClient::new(&config)?;

    match cmd {
        ConfluenceCommands::Space { command } => match command {
            SpaceCommands::List(args) => {
                let space_type = if args.space_type == "all" { None } else { Some(args.space_type.as_str()) };
                let mut result = confluence.list_spaces(args.limit, args.start, space_type).await?;
                // Client-side name filter
                if let Some(name_filter) = &args.name {
                    let lower = name_filter.to_lowercase();
                    result.results.retain(|s| s.name.to_lowercase().contains(&lower) || s.key.to_lowercase().contains(&lower));
                }
                let use_plain = args.plain || !output::is_tty();
                if use_plain {
                    output::confluence::print_spaces_table(&result.results, result.results.len() as u32);
                } else {
                    output::confluence::run_space_list_tui(&result.results, result.results.len() as u32, &config)?;
                }
                Ok(())
            }
        },
        ConfluenceCommands::Page { command } => match command {
            PageCommands::List(args) => {
                let use_plain = args.plain || !output::is_tty();
                if use_plain {
                    if let Some(ref space) = args.space {
                        let result = confluence.list_pages(space, args.limit, args.start).await?;
                        output::confluence::print_pages_table(&result.results, result.size);
                    } else {
                        eprintln!("Use --space KEY to list pages, or omit for TUI search mode.");
                    }
                } else {
                    use std::sync::{Arc, Mutex};
                    use std::sync::atomic::AtomicBool;
                    let initial = if let Some(ref space) = args.space {
                        confluence.list_pages(space, args.limit, args.start).await?.results
                    } else {
                        confluence.list_recent_pages(args.limit).await?.results
                    };
                    let pages_arc = Arc::new(Mutex::new(initial));
                    let loading = Arc::new(AtomicBool::new(false));
                    let space_opt = args.space.clone();

                    use output::confluence::PageTuiAction;
                    loop {
                        match output::confluence::run_page_list_tui(Arc::clone(&pages_arc), Arc::clone(&loading), &config)? {
                            PageTuiAction::Browse { space_key, page_id } => {
                                match config.confluence_browse_url(&space_key, &page_id) {
                                    Some(url) => open_browser(&url)?,
                                    None => eprintln!("No 'server' URL in config."),
                                }
                            }
                            PageTuiAction::View(id) => {
                                let (page, children) = tokio::try_join!(
                                    confluence.get_page(&id),
                                    confluence.get_child_pages(&id),
                                )?;
                                output::confluence::run_page_view(&page, &children, &config)?;
                            }
                            PageTuiAction::Search(query) => {
                                let result = confluence.search_pages(&query, space_opt.as_deref(), args.limit, 0).await?;
                                *pages_arc.lock().unwrap() = result.results;
                            }
                            PageTuiAction::Quit => break,
                        }
                    }
                }
                Ok(())
            }
            PageCommands::Get(args) => {
                let (page, children) = tokio::try_join!(
                    confluence.get_page(&args.id),
                    confluence.get_child_pages(&args.id),
                )?;
                output::confluence::print_page_detail(&page, &children);
                Ok(())
            }
            PageCommands::View(args) => {
                let (page, children) = tokio::try_join!(
                    confluence.get_page(&args.id),
                    confluence.get_child_pages(&args.id),
                )?;
                if args.raw {
                    let raw = page
                        .body
                        .as_ref()
                        .and_then(|b| b.atlas_doc_format.as_ref())
                        .map(|a| a.value.as_str())
                        .unwrap_or("{}");
                    println!("{}", raw);
                    return Ok(());
                }
                let use_plain = !output::is_tty();
                if use_plain {
                    output::confluence::print_page_detail(&page, &children);
                } else {
                    output::confluence::run_page_view(&page, &children, &config)?;
                }
                Ok(())
            }
        },
        ConfluenceCommands::Search(args) => {
            let result = confluence
                .search_pages(&args.query, args.space.as_deref(), args.limit, 0)
                .await?;
            let use_plain = args.plain || !output::is_tty();
            if use_plain {
                output::confluence::print_pages_table(&result.results, result.size);
            } else {
                use std::sync::{Arc, Mutex};
                use std::sync::atomic::AtomicBool;
                let pages_arc = Arc::new(Mutex::new(result.results));
                let loading = Arc::new(AtomicBool::new(false)); // already fully loaded
                use output::confluence::PageTuiAction;
                loop {
                    match output::confluence::run_page_list_tui(Arc::clone(&pages_arc), Arc::clone(&loading), &config)? {
                        PageTuiAction::Browse { space_key, page_id } => {
                            match config.confluence_browse_url(&space_key, &page_id) {
                                Some(url) => open_browser(&url)?,
                                None => eprintln!("No 'server' URL in config."),
                            }
                        }
                        PageTuiAction::View(id) => {
                            let (page, children) = tokio::try_join!(
                                confluence.get_page(&id),
                                confluence.get_child_pages(&id),
                            )?;
                            output::confluence::run_page_view(&page, &children, &config)?;
                        }
                        PageTuiAction::Search(_) | PageTuiAction::Quit => break,
                    }
                }
            }
            Ok(())
        }
    }
}

async fn fetch_children(jira: &JiraClient, issue: &Issue) -> Vec<Issue> {
    let itype = issue.fields.issue_type.as_ref().map(|t| t.name.as_str()).unwrap_or("");
    let jql = if itype.eq_ignore_ascii_case("epic") {
        format!("(parent = \"{}\" OR \"Epic Link\" = \"{}\") ORDER BY created DESC", issue.key, issue.key)
    } else {
        format!("parent = \"{}\" ORDER BY created DESC", issue.key)
    };
    jira.search_issues(&jql, None, 100).await
        .map(|r| r.issues)
        .unwrap_or_default()
}

async fn handle_issue(cmd: IssueCommands, jira: &JiraClient, config: &Config) -> Result<()> {
    match cmd {
        IssueCommands::List(args) => {
            let jql = build_issue_jql(&args, config);

            let (_, limit) = parse_paginate(&args);
            let result = jira.search_issues(&jql, None, limit).await?;

            if args.raw {
                let json = serde_json::to_string_pretty(&result.issues)?;
                println!("{}", json);
                return Ok(());
            }

            let cols: Option<Vec<String>> = args.columns.as_deref().map(|c| {
                c.split(',').map(|s| s.trim().to_string()).collect()
            });
            let col_refs: Option<Vec<&str>> = cols.as_deref().map(|v| v.iter().map(|s| s.as_str()).collect());

            let opts = output::table::OutputOptions {
                no_headers: args.no_headers,
                no_truncate: args.no_truncate,
                columns: col_refs,
                delimiter: &args.delimiter,
                csv: args.csv,
            };

            let use_plain = args.plain || args.csv || args.raw || !output::is_tty();
            if use_plain {
                output::table::print_issues_table(&result.issues, result.total, &opts);
            } else {
                use output::tui::TuiAction;
                let mut selected = 0usize;
                let mut issues = result.issues;
                let mut total = result.total;
                loop {
                    match output::tui::run_tui(&issues, total, selected, &config.custom_fields)? {
                        TuiAction::Browse(key) => {
                            match config.browse_url(&key) {
                                Some(url) => open_browser(&url)?,
                                None => {
                                    eprintln!("No 'server' URL in config. Add server = \"https://yourcompany.atlassian.net\" to open issues in the browser.");
                                    println!("{}", key);
                                }
                            }
                            // stay in list view after opening browser
                        }
                        TuiAction::Detail(key) => {
                            let idx = issues.iter().position(|i| i.key == key).unwrap_or(selected);
                            let issue = jira.get_issue(&key).await?;
                            let children = fetch_children(jira, &issue).await;
                            loop {
                                match output::tui::run_issue_view(&issue, &children, None, idx, &config.custom_fields)? {
                                    TuiAction::Browse(k) => {
                                        if let Some(url) = config.browse_url(&k) {
                                            open_browser(&url)?;
                                        }
                                        // stay in detail view after opening browser
                                    }
                                    TuiAction::Back(i) => { selected = i; break; }
                                    _ => { return Ok(()); }
                                }
                            }
                        }
                        TuiAction::Search(query) => {
                            let search_jql = build_search_jql(&query, &args, config);
                            match jira.search_issues(&search_jql, None, limit).await {
                                Ok(r) => { issues = r.issues; total = r.total; }
                                Err(e) => { eprintln!("Search error: {}", e); }
                            }
                            selected = 0;
                        }
                        TuiAction::Back(_) | TuiAction::Quit => break,
                    }
                }
            }
            Ok(())
        }

        IssueCommands::Get(args) => {
            let issue = jira.get_issue(&args.key).await?;
            output::print_issue_detail(&issue);
            Ok(())
        }

        IssueCommands::Create(args) => {
            let project = args.project
                .or_else(|| config.default_project.clone())
                .ok_or_else(|| anyhow::anyhow!("Project is required. Use -p/--project or set default_project in config."))?;

            let summary = if let Some(s) = args.summary {
                s
            } else {
                prompt("Summary: ")?
            };

            let issue = jira.create_issue(
                &project,
                &args.issue_type,
                &summary,
                args.description.as_deref(),
                args.assignee.as_deref(),
                args.priority.as_deref(),
                &args.labels,
                &args.components,
                args.parent.as_deref(),
            ).await?;

            println!("{} Created {}", "✓".green().bold(), issue.key.cyan().bold().to_string());
            println!("{}", issue.fields.summary);
            Ok(())
        }

        IssueCommands::Edit(args) => {
            let (add_labels, remove_labels) = split_add_remove(&args.labels);
            let (add_comps, remove_comps) = split_add_remove(&args.components);
            let (add_fv, remove_fv) = split_add_remove(&args.fix_versions);

            let has_changes = args.summary.is_some() || args.body.is_some()
                || args.priority.is_some() || args.assignee.is_some()
                || !args.labels.is_empty() || !args.components.is_empty()
                || !args.fix_versions.is_empty();
            if !has_changes {
                anyhow::bail!("No fields to update. Use -s, -b, -y, -l, -C, --fix-version, or -a.");
            }

            jira.edit_issue(
                &args.key,
                args.summary.as_deref(),
                args.body.as_deref(),
                args.priority.as_deref(),
                args.assignee.as_deref(),
                &add_labels,
                &remove_labels,
                &add_comps,
                &remove_comps,
                &add_fv,
                &remove_fv,
            ).await?;

            println!("{} Updated {}", "✓".green().bold(), args.key.cyan().bold().to_string());
            Ok(())
        }

        IssueCommands::Assign(args) => {
            let assignee_input = if let Some(a) = args.assignee {
                a
            } else {
                prompt(&format!("Assign {} to (name, account ID, 'x' to unassign): ", args.key))?
            };

            let account_id: Option<String> = match assignee_input.as_str() {
                "x" => None, // unassign
                "default" => Some("-1".to_string()),
                query => {
                    // If it looks like a raw account ID (no spaces, only alphanumeric/:/-),
                    // use it directly to avoid a failed search on an opaque ID string.
                    let looks_like_id = !query.contains(' ')
                        && query.len() > 16
                        && query.chars().all(|c| c.is_alphanumeric() || c == ':' || c == '-' || c == '_');
                    if looks_like_id {
                        Some(query.to_string())
                    } else {
                        let users = jira.search_assignable_users(&args.key, query).await?;
                        match users.len() {
                            0 => anyhow::bail!("No assignable user found matching '{}'", query),
                            1 => Some(users[0].account_id.clone()),
                            _ => {
                                println!("Multiple users found:");
                                for (i, u) in users.iter().enumerate() {
                                    println!("  {}. {} ({})", i + 1, u.display_name, u.email_address);
                                }
                                let sel = prompt("Select number: ")?;
                                let idx: usize = sel.trim().parse::<usize>()
                                    .map_err(|_| anyhow::anyhow!("Invalid selection"))?
                                    .checked_sub(1)
                                    .ok_or_else(|| anyhow::anyhow!("Invalid selection"))?;
                                let user = users.get(idx)
                                    .ok_or_else(|| anyhow::anyhow!("Selection out of range"))?;
                                Some(user.account_id.clone())
                            }
                        }
                    }
                }
            };

            jira.assign_issue(&args.key, account_id.as_deref()).await?;
            match account_id.as_deref() {
                None => println!("{} Unassigned {}", "✓".green().bold(), args.key.cyan().bold().to_string()),
                Some("-1") => println!("{} {} assigned to default assignee", "✓".green().bold(), args.key.cyan().bold().to_string()),
                _ => println!("{} {} assigned", "✓".green().bold(), args.key.cyan().bold().to_string()),
            }
            Ok(())
        }

        IssueCommands::Link(args) => {
            let inward = match args.inward {
                Some(k) => k,
                None => prompt("Inward issue key (e.g. PROJ-1): ")?,
            };
            let outward = match args.outward {
                Some(k) => k,
                None => prompt("Outward issue key (e.g. PROJ-2): ")?,
            };
            let link_type = match args.link_type {
                Some(t) => t,
                None => {
                    let types = jira.get_link_types().await?;
                    println!("Available link types:");
                    for (i, t) in types.iter().enumerate() {
                        println!("  {}. {} (inward: '{}', outward: '{}')", i + 1, t.name, t.inward, t.outward);
                    }
                    let sel = prompt("Select number: ")?;
                    let idx = sel.trim().parse::<usize>()
                        .map_err(|_| anyhow::anyhow!("Invalid selection"))?
                        .checked_sub(1)
                        .ok_or_else(|| anyhow::anyhow!("Invalid selection"))?;
                    types.get(idx)
                        .ok_or_else(|| anyhow::anyhow!("Selection out of range"))?
                        .name.clone()
                }
            };

            jira.create_issue_link(&inward, &outward, &link_type).await?;
            println!("{} Linked {} → {} ({})", "✓".green().bold(),
                inward.cyan().bold().to_string(), outward.cyan().bold().to_string(), link_type);
            Ok(())
        }

        IssueCommands::Unlink(args) => {
            let inward = match args.inward {
                Some(k) => k,
                None => prompt("First issue key: ")?,
            };
            let outward = match args.outward {
                Some(k) => k,
                None => prompt("Second issue key: ")?,
            };

            let count = jira.delete_issue_link(&inward, &outward).await?;
            println!("{} Removed {} link(s) between {} and {}",
                "✓".green().bold(), count,
                inward.cyan().bold().to_string(), outward.cyan().bold().to_string());
            Ok(())
        }

        IssueCommands::Move(args) => {
            let transitions = jira.get_transitions(&args.key).await?;

            let target = if let Some(state) = args.state {
                state
            } else {
                println!("Available transitions for {}:", args.key.cyan().to_string());
                for (i, t) in transitions.iter().enumerate() {
                    println!("  {}. {}", i + 1, t.name);
                }
                prompt("Transition to: ")?
            };

            let transition = transitions.iter()
                .find(|t| t.name.to_lowercase() == target.to_lowercase())
                .or_else(|| transitions.iter().find(|t| t.name.to_lowercase().contains(&target.to_lowercase())))
                .ok_or_else(|| anyhow::anyhow!(
                    "Transition '{}' not found. Available: {}",
                    target,
                    transitions.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
                ))?;

            jira.transition_issue(&args.key, &transition.id).await?;
            println!("{} {} → {}", "✓".green().bold(), args.key.cyan().to_string(), transition.name.green().to_string());
            Ok(())
        }

        IssueCommands::View(args) => {
            let issue = jira.get_issue(&args.key).await?;
            let children = fetch_children(jira, &issue).await;
            loop {
                match output::tui::run_issue_view(&issue, &children, args.comments, 0, &config.custom_fields)? {
                    output::tui::TuiAction::Browse(key) => {
                        match config.browse_url(&key) {
                            Some(url) => open_browser(&url)?,
                            None => eprintln!("No 'server' URL in config. Add server = \"https://yourcompany.atlassian.net\""),
                        }
                        // stay in detail view after opening browser
                    }
                    _ => break,
                }
            }
            Ok(())
        }

        IssueCommands::Attachment(args) => {
            let issue = jira.get_issue(&args.key).await?;
            let attachments = &issue.fields.attachment;

            if attachments.is_empty() {
                println!("No attachments on {}", args.key);
                return Ok(());
            }

            if let Some(idx) = args.open {
                if idx == 0 {
                    anyhow::bail!("Attachment index must be 1 or greater");
                }
                let att = attachments.get(idx - 1)
                    .ok_or_else(|| anyhow::anyhow!("Attachment index {} out of range (1–{})", idx, attachments.len()))?;

                // Sanitize filename: use only the final path component, no directory traversal
                let safe_name = std::path::Path::new(&att.filename)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|n| !n.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("Attachment has invalid filename"))?;

                let bytes = jira.download_attachment(&att.content).await?;

                if args.save {
                    let dest = std::path::Path::new(safe_name);
                    std::fs::write(dest, &bytes)?;
                    println!("{} Saved {} ({} bytes)", "✓".green().bold(), safe_name.cyan().to_string(), bytes.len());
                } else {
                    let tmp_path = std::env::temp_dir().join(safe_name);
                    std::fs::write(&tmp_path, &bytes)?;
                    println!("Opening {} …", safe_name.cyan().to_string());
                    open_browser(&tmp_path.to_string_lossy())?;
                    // Temp file left for OS to clean; the opener may read it asynchronously
                }
                return Ok(());
            }

            // List attachments
            println!("{} attachments on {}:", attachments.len(), args.key.cyan().bold().to_string());
            println!();
            for (i, att) in attachments.iter().enumerate() {
                let size_kb = att.size / 1024;
                println!("  {}. {} ({}, {} KB)",
                    (i + 1).to_string().dimmed(),
                    att.filename.cyan().to_string(),
                    att.mime_type.dimmed(),
                    size_kb,
                );
            }
            println!();
            println!("Use --open N to open an attachment (e.g. --open 1)");
            Ok(())
        }

        IssueCommands::Comment { command } => {
            match command {
                CommentCommands::Add(args) => {
                    // Resolve comment body: arg > template file > stdin
                    let body = if let Some(b) = args.body {
                        b
                    } else if let Some(tmpl) = args.template {
                        if tmpl == "-" {
                            use std::io::Read;
                            let mut s = String::new();
                            std::io::stdin().read_to_string(&mut s)?;
                            s.trim_end().to_string()
                        } else {
                            std::fs::read_to_string(&tmpl)
                                .map_err(|e| anyhow::anyhow!("Could not read template '{}': {}", tmpl, e))?
                                .trim_end()
                                .to_string()
                        }
                    } else {
                        // Check if stdin has data piped in
                        use std::io::IsTerminal;
                        if !std::io::stdin().is_terminal() {
                            use std::io::Read;
                            let mut s = String::new();
                            std::io::stdin().read_to_string(&mut s)?;
                            s.trim_end().to_string()
                        } else {
                            prompt("Comment: ")?
                        }
                    };

                    if body.trim().is_empty() {
                        anyhow::bail!("Comment body cannot be empty");
                    }

                    jira.add_comment(&args.key, &body).await?;
                    println!("{} Comment added to {}", "✓".green().bold(), args.key.cyan().bold().to_string());

                    if args.web {
                        match config.browse_url(&args.key) {
                            Some(url) => open_browser(&url)?,
                            None => eprintln!("No 'server' URL in config."),
                        }
                    }
                    Ok(())
                }

                CommentCommands::List(args) => {
                    let comments_field = jira.list_comments(&args.key).await?;
                    let all = &comments_field.comments;

                    if all.is_empty() {
                        println!("No comments on {}", args.key);
                        return Ok(());
                    }

                    let comments: &[_] = if let Some(n) = args.number {
                        let start = all.len().saturating_sub(n);
                        &all[start..]
                    } else {
                        all
                    };

                    println!("{} comment(s) on {} (showing {}):",
                        comments_field.total,
                        args.key.cyan().bold().to_string(),
                        comments.len(),
                    );

                    for c in comments {
                        let author = c.author.as_ref().map(|u| u.display_name.as_str()).unwrap_or("unknown");
                        let created = c.created.as_deref().unwrap_or("");
                        println!();
                        if args.plain {
                            println!("{} — {}", author, created);
                        } else {
                            println!("{} {}", author.cyan().to_string(), created.dimmed().to_string());
                        }
                        println!("{}", "─".repeat(60).dimmed().to_string());
                        if let Some(body) = &c.body {
                            print!("{}", output::adf_to_text(body));
                        }
                    }
                    Ok(())
                }
            }
        }
    }
}

async fn handle_epic(cmd: EpicCommands, jira: &JiraClient, config: &Config) -> Result<()> {
    match cmd {
        EpicCommands::Create(args) => {
            let project = args.project
                .or_else(|| config.default_project.clone())
                .ok_or_else(|| anyhow::anyhow!("Project is required. Use -p/--project or set default_project in config."))?;

            let summary = if let Some(s) = args.summary {
                s
            } else {
                prompt("Summary: ")?
            };

            let epic = jira.create_issue(
                &project,
                "Epic",
                &summary,
                args.description.as_deref(),
                args.assignee.as_deref(),
                args.priority.as_deref(),
                &args.labels,
                &args.components,
                None,
            ).await?;

            println!("{} Created epic {}", "✓".green().bold(), epic.key.cyan().bold().to_string());
            println!("{}", epic.fields.summary);
            Ok(())
        }

        EpicCommands::List(args) => {
            let jql = build_epic_jql(&args, config);
            let (_, limit) = parse_paginate_epic(&args);

            let result = jira.search_issues(&jql, None, limit).await?;

            if args.raw {
                println!("{}", serde_json::to_string_pretty(&result.issues)?);
                return Ok(());
            }

            let opts = output::table::OutputOptions {
                no_headers: args.no_headers,
                no_truncate: args.no_truncate,
                columns: args.columns.as_deref().map(|c| c.split(',').map(|s| s.trim()).collect()),
                delimiter: &args.delimiter,
                csv: args.csv,
            };

            let use_plain = args.plain || args.table || args.csv || !output::is_tty();
            if use_plain {
                output::table::print_issues_table(&result.issues, result.total, &opts);
            } else {
                use output::tui::TuiAction;
                let mut selected = 0usize;
                let mut issues = result.issues;
                let mut total = result.total;
                loop {
                    match output::tui::run_tui(&issues, total, selected, &config.custom_fields)? {
                        TuiAction::Browse(key) => {
                            match config.browse_url(&key) {
                                Some(url) => open_browser(&url)?,
                                None => {
                                    eprintln!("No 'server' URL in config.");
                                    println!("{}", key);
                                }
                            }
                            // stay in list view after opening browser
                        }
                        TuiAction::Detail(key) => {
                            let idx = issues.iter().position(|i| i.key == key).unwrap_or(selected);
                            let issue = jira.get_issue(&key).await?;
                            let children = fetch_children(jira, &issue).await;
                            loop {
                                match output::tui::run_issue_view(&issue, &children, None, idx, &config.custom_fields)? {
                                    TuiAction::Browse(k) => {
                                        if let Some(url) = config.browse_url(&k) {
                                            open_browser(&url)?;
                                        }
                                        // stay in detail view after opening browser
                                    }
                                    TuiAction::Back(i) => { selected = i; break; }
                                    _ => { return Ok(()); }
                                }
                            }
                        }
                        TuiAction::Search(query) => {
                            let search_jql = build_epic_search_jql(&query, &args, config);
                            match jira.search_issues(&search_jql, None, limit).await {
                                Ok(r) => { issues = r.issues; total = r.total; }
                                Err(e) => { eprintln!("Search error: {}", e); }
                            }
                            selected = 0;
                        }
                        TuiAction::Back(_) | TuiAction::Quit => break,
                    }
                }
            }
            Ok(())
        }
    }
}

fn build_epic_jql(args: &cli::epic::EpicListArgs, config: &Config) -> String {
    let mut conditions: Vec<String> = Vec::new();

    // Project scope
    if let Some(p) = &args.project {
        conditions.push(format!("project = \"{}\"", p));
    } else if let Some(p) = &config.default_project {
        conditions.push(format!("project = \"{}\"", p));
    }

    if let Some(epic_key) = &args.epic_key {
        // List child issues of a specific epic
        // Works for both Next-gen (parent =) and Classic (Epic Link =) projects
        conditions.push(format!("(parent = \"{}\" OR \"Epic Link\" = \"{}\")", epic_key, epic_key));
    } else {
        // List epics
        conditions.push("issuetype = Epic".to_string());
    }

    // Shared filters
    let (pos_status, neg_status) = split_negated(&args.status);
    if !pos_status.is_empty() {
        let vals = pos_status.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ");
        conditions.push(format!("status IN ({})", vals));
    }
    if !neg_status.is_empty() {
        let vals = neg_status.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ");
        conditions.push(format!("status NOT IN ({})", vals));
    }

    if let Some(a) = &args.assignee {
        let jql = match a.as_str() {
            "x"  => "assignee is EMPTY".to_string(),
            "~x" => "assignee is not EMPTY".to_string(),
            "me" => "assignee = currentUser()".to_string(),
            s if s.starts_with('~') => format!("assignee != \"{}\"", &s[1..]),
            s    => format!("assignee = \"{}\"", s),
        };
        conditions.push(jql);
    }

    if let Some(r) = &args.reporter {
        let jql = match r.as_str() {
            "x"  => "reporter is EMPTY".to_string(),
            "~x" => "reporter is not EMPTY".to_string(),
            "me" => "reporter = currentUser()".to_string(),
            s if s.starts_with('~') => format!("reporter != \"{}\"", &s[1..]),
            s    => format!("reporter = \"{}\"", s),
        };
        conditions.push(jql);
    }

    if let Some(p) = &args.priority {
        conditions.push(format!("priority = \"{}\"", p));
    }
    if let Some(r) = &args.resolution {
        conditions.push(format!("resolution = \"{}\"", r));
    }

    let (pos_labels, neg_labels) = split_negated(&args.labels);
    if !pos_labels.is_empty() {
        let vals = pos_labels.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ");
        conditions.push(format!("labels IN ({})", vals));
    }
    if !neg_labels.is_empty() {
        let vals = neg_labels.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ");
        conditions.push(format!("labels NOT IN ({})", vals));
    }

    if let Some(c) = &args.component {
        conditions.push(format!("component = \"{}\"", c));
    }

    if let Some(d) = &args.created {
        conditions.push(format!("created >= \"{}\"", normalize_date(d)));
    } else {
        if let Some(d) = &args.created_after  { conditions.push(format!("created > \"{}\"", d)); }
        if let Some(d) = &args.created_before { conditions.push(format!("created < \"{}\"", d)); }
    }
    if let Some(d) = &args.updated {
        conditions.push(format!("updated >= \"{}\"", normalize_date(d)));
    } else {
        if let Some(d) = &args.updated_after  { conditions.push(format!("updated > \"{}\"", d)); }
        if let Some(d) = &args.updated_before { conditions.push(format!("updated < \"{}\"", d)); }
    }

    if args.history  { conditions.push("issueFunction in issueHistory()".to_string()); }
    if args.watching { conditions.push("issue in watchedIssues()".to_string()); }
    if let Some(jql) = &args.jql { conditions.push(format!("({})", jql)); }

    let mut jql = conditions.join(" AND ");

    let direction = if args.reverse { "ASC" } else { "DESC" };
    let order_lower = args.order_by.to_lowercase();
    let order_field = match order_lower.as_str() {
        "created" => "created", "updated" => "updated", "rank" => "rank",
        "priority" => "priority", "status" => "status", other => other,
    };
    jql.push_str(&format!(" ORDER BY {} {}", order_field, direction));
    jql.trim().to_string()
}

fn parse_paginate_epic(args: &cli::epic::EpicListArgs) -> (u32, u32) {
    if let Some(p) = &args.paginate {
        if let Some((f, l)) = p.split_once(':') {
            return (f.parse().unwrap_or(0), l.parse().unwrap_or(100).min(100));
        }
        return (0, p.parse().unwrap_or(100).min(100));
    }
    (0, args.max_results.min(100))
}

async fn handle_project(cmd: ProjectCommands, jira: &JiraClient) -> Result<()> {
    match cmd {
        ProjectCommands::List(args) => {
            let projects = jira.list_projects(args.max_results).await?;
            output::print_projects_table(&projects);
            Ok(())
        }
        ProjectCommands::Get(args) => {
            let project = jira.get_project(&args.key).await?;
            println!("{} {}", project.key.cyan().bold().to_string(), project.name.bold());
            println!("  Type: {}", project.project_type);
            if let Some(lead) = &project.lead {
                println!("  Lead: {}", lead.display_name);
            }
            Ok(())
        }
    }
}

fn parse_paginate(args: &cli::issue::ListArgs) -> (u32, u32) {
    if let Some(p) = &args.paginate {
        if let Some((f, l)) = p.split_once(':') {
            let from = f.parse().unwrap_or(0);
            // Cloud v3 uses cursor-based pagination; offset (from) is not directly supported.
            // We pass only the limit and ignore from.
            let limit = l.parse().unwrap_or(100).min(100);
            return (from, limit);
        }
        let limit = p.parse().unwrap_or(100).min(100);
        return (0, limit);
    }
    (0, args.max_results.min(100))
}

fn build_issue_jql(args: &cli::issue::ListArgs, config: &Config) -> String {
    let mut conditions: Vec<String> = Vec::new();

    // Project
    if let Some(p) = &args.project {
        conditions.push(format!("project = \"{}\"", p));
    } else if let Some(p) = &config.default_project {
        conditions.push(format!("project = \"{}\"", p));
    }

    // Status (Vec, supports ~ negation)
    let (pos_status, neg_status) = split_negated(&args.status);
    if !pos_status.is_empty() {
        let vals = pos_status.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ");
        conditions.push(format!("status IN ({})", vals));
    }
    if !neg_status.is_empty() {
        let vals = neg_status.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ");
        conditions.push(format!("status NOT IN ({})", vals));
    }

    // Assignee ('x' = unassigned)
    if let Some(a) = &args.assignee {
        let jql = match a.as_str() {
            "x"           => "assignee is EMPTY".to_string(),
            "~x"          => "assignee is not EMPTY".to_string(),
            "me"          => "assignee = currentUser()".to_string(),
            "~me"         => "assignee != currentUser()".to_string(),
            s if s.starts_with('~') => format!("assignee != \"{}\"", &s[1..]),
            s             => format!("assignee = \"{}\"", s),
        };
        conditions.push(jql);
    }

    // Reporter (supports ~ negation and x/~x for empty/non-empty)
    if let Some(r) = &args.reporter {
        let jql = match r.as_str() {
            "x"           => "reporter is EMPTY".to_string(),
            "~x"          => "reporter is not EMPTY".to_string(),
            "me"          => "reporter = currentUser()".to_string(),
            "~me"         => "reporter != currentUser()".to_string(),
            s if s.starts_with('~') => format!("reporter != \"{}\"", &s[1..]),
            s             => format!("reporter = \"{}\"", s),
        };
        conditions.push(jql);
    }

    // Priority
    if let Some(p) = &args.priority {
        conditions.push(format!("priority = \"{}\"", p));
    }

    // Issue type
    if let Some(t) = &args.issue_type {
        conditions.push(format!("issuetype = \"{}\"", t));
    }

    // Resolution
    if let Some(r) = &args.resolution {
        conditions.push(format!("resolution = \"{}\"", r));
    }

    // Parent
    if let Some(p) = &args.parent {
        conditions.push(format!("parent = \"{}\"", p));
    }

    // Labels (Vec, supports ~ negation)
    let (pos_labels, neg_labels) = split_negated(&args.labels);
    if !pos_labels.is_empty() {
        let vals = pos_labels.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ");
        conditions.push(format!("labels IN ({})", vals));
    }
    if !neg_labels.is_empty() {
        let vals = neg_labels.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ");
        conditions.push(format!("labels NOT IN ({})", vals));
    }

    // Component
    if let Some(c) = &args.component {
        conditions.push(format!("component = \"{}\"", c));
    }

    // Fix version
    if let Some(fv) = &args.fix_version {
        conditions.push(format!("fixVersion = \"{}\"", fv));
    }

    // Created (has precedence over created-after/before)
    if let Some(d) = &args.created {
        conditions.push(format!("created >= \"{}\"", normalize_date(d)));
    } else {
        if let Some(d) = &args.created_after {
            conditions.push(format!("created > \"{}\"", d));
        }
        if let Some(d) = &args.created_before {
            conditions.push(format!("created < \"{}\"", d));
        }
    }

    // Updated
    if let Some(d) = &args.updated {
        conditions.push(format!("updated >= \"{}\"", normalize_date(d)));
    } else {
        if let Some(d) = &args.updated_after {
            conditions.push(format!("updated > \"{}\"", d));
        }
        if let Some(d) = &args.updated_before {
            conditions.push(format!("updated < \"{}\"", d));
        }
    }

    // History
    if args.history {
        conditions.push("issueFunction in issueHistory()".to_string());
    }

    // Watching
    if args.watching {
        conditions.push("issue in watchedIssues()".to_string());
    }

    // Raw JQL appended
    if let Some(jql) = &args.jql {
        conditions.push(format!("({})", jql));
    }

    // Free text query
    if let Some(q) = &args.query {
        conditions.push(format!("text ~ \"{}\"", q));
    }

    let mut jql = if conditions.is_empty() {
        "assignee = currentUser()".to_string()
    } else {
        conditions.join(" AND ")
    };

    // Order by
    let direction = if args.reverse { "ASC" } else { "DESC" };
    let order_lower = args.order_by.to_lowercase();
    let order_field = match order_lower.as_str() {
        "created" => "created",
        "updated" => "updated",
        "rank" => "rank",
        "priority" => "priority",
        "status" => "status",
        "assignee" => "assignee",
        "key" => "key",
        other => other,
    };
    jql.push_str(&format!(" ORDER BY {} {}", order_field, direction));
    jql.trim().to_string()
}

/// Build a JQL query for text search within the issue list context.
/// Preserves project scope from the original args, adds `text ~ "query"`.
fn build_search_jql(query: &str, args: &cli::issue::ListArgs, config: &Config) -> String {
    let mut conditions: Vec<String> = Vec::new();

    if let Some(p) = &args.project {
        conditions.push(format!("project = \"{}\"", p));
    } else if let Some(p) = &config.default_project {
        conditions.push(format!("project = \"{}\"", p));
    }

    conditions.push(format!("text ~ \"{}\"", query));
    conditions.join(" AND ") + " ORDER BY rank DESC"
}

/// Build a JQL query for text search within the epic list context.
fn build_epic_search_jql(query: &str, args: &cli::epic::EpicListArgs, config: &Config) -> String {
    let mut conditions: Vec<String> = Vec::new();

    if let Some(p) = &args.project {
        conditions.push(format!("project = \"{}\"", p));
    } else if let Some(p) = &config.default_project {
        conditions.push(format!("project = \"{}\"", p));
    }

    if let Some(epic_key) = &args.epic_key {
        conditions.push(format!("(parent = \"{}\" OR \"Epic Link\" = \"{}\")", epic_key, epic_key));
    } else {
        conditions.push("issuetype = Epic".to_string());
    }

    conditions.push(format!("text ~ \"{}\"", query));
    conditions.join(" AND ") + " ORDER BY rank DESC"
}

/// Split a list of values into (positive, negative) where negative ones have ~ prefix
fn split_negated(values: &[String]) -> (Vec<String>, Vec<String>) {
    let mut pos = Vec::new();
    let mut neg = Vec::new();
    for v in values {
        if let Some(stripped) = v.strip_prefix('~') {
            neg.push(stripped.to_string());
        } else {
            pos.push(v.clone());
        }
    }
    (pos, neg)
}

/// Split a list into add/remove sets: items prefixed with `-` go into remove.
fn split_add_remove(values: &[String]) -> (Vec<String>, Vec<String>) {
    let mut add = Vec::new();
    let mut remove = Vec::new();
    for v in values {
        if let Some(stripped) = v.strip_prefix('-') {
            remove.push(stripped.to_string());
        } else {
            add.push(v.clone());
        }
    }
    (add, remove)
}

/// Normalize date strings to JQL-compatible format
fn normalize_date(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "today" => "startOfDay()".to_string(),
        "week"  => "startOfWeek()".to_string(),
        "month" => "startOfMonth()".to_string(),
        "year"  => "startOfYear()".to_string(),
        other   => other.to_string(),
    }
}

fn prompt(msg: &str) -> Result<String> {
    use std::io::Write;
    print!("{}", msg);
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

/// Open a URL in the browser.
/// Respects JIRA_BROWSER and BROWSER env vars (same as jira-cli), falling back to native open.
fn open_browser(url: &str) -> Result<()> {
    // JIRA_BROWSER takes priority, then BROWSER, then platform default
    if let Ok(browser) = std::env::var("JIRA_BROWSER").or_else(|_| std::env::var("BROWSER")) {
        let mut parts = browser.split_whitespace();
        if let Some(bin) = parts.next() {
            let mut cmd = std::process::Command::new(bin);
            for arg in parts { cmd.arg(arg); }
            cmd.arg(url).spawn()?;
            return Ok(());
        }
    }

    // Platform native fallback
    if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()?;
    } else if cfg!(target_os = "windows") {
        // Use explorer.exe directly — avoids cmd.exe shell metacharacter interpretation
        std::process::Command::new("explorer").arg(url).spawn()?;
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}
