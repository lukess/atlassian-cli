/// CLI argument parsing tests — verify that commands, flags, and defaults parse correctly.
/// These tests do NOT make network calls; they only exercise clap argument parsing.
#[cfg(test)]
mod tests {
    use clap::Parser;
    use crate::cli::{Cli, Commands, JiraCommands};
    use crate::cli::issue::IssueCommands;
    use crate::cli::epic::EpicCommands;
    use crate::cli::confluence::{ConfluenceCommands, PageCommands, SpaceCommands};

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("failed to parse args")
    }

    fn parse_err(args: &[&str]) -> clap::Error {
        match Cli::try_parse_from(args) {
            Ok(_) => panic!("expected parse error but parsing succeeded"),
            Err(e) => e,
        }
    }

    // ── jira issue list ─────────────────────────────────────────────────────────

    #[test]
    fn issue_list_no_args() {
        let cli = parse(&["atlassian", "jira", "issue", "list"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert!(args.project.is_none());
        assert!(args.status.is_empty());
        assert_eq!(args.order_by, "created");
        assert_eq!(args.max_results, 100);
        assert!(!args.plain);
        assert!(!args.csv);
    }

    #[test]
    fn issue_list_short_flags() {
        let cli = parse(&["atlassian", "jira", "issue", "list", "-p", "PROJ", "-s", "open", "-y", "High", "-t", "Story", "-a", "me"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.project.as_deref(), Some("PROJ"));
        assert_eq!(args.status, vec!["open"]);
        assert_eq!(args.priority.as_deref(), Some("High"));
        assert_eq!(args.issue_type.as_deref(), Some("Story"));
        assert_eq!(args.assignee.as_deref(), Some("me"));
    }

    #[test]
    fn issue_list_long_flags() {
        let cli = parse(&["atlassian", "jira", "issue", "list", "--project", "ABC", "--status", "Done", "--order-by", "rank", "--reverse"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.project.as_deref(), Some("ABC"));
        assert_eq!(args.status, vec!["Done"]);
        assert_eq!(args.order_by, "rank");
        assert!(args.reverse);
    }

    #[test]
    fn issue_list_multiple_status() {
        let cli = parse(&["atlassian", "jira", "issue", "list", "-s", "open", "-s", "~Done"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.status, vec!["open", "~Done"]);
    }

    #[test]
    fn issue_list_hyphen_values() {
        // Negative date values (like -7d) must parse without being treated as flags
        let cli = parse(&["atlassian", "jira", "issue", "list", "--created", "-7d", "--created-before", "-2w"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.created.as_deref(), Some("-7d"));
        assert_eq!(args.created_before.as_deref(), Some("-2w"));
    }

    #[test]
    fn issue_list_tilde_negation_assignee() {
        let cli = parse(&["atlassian", "jira", "issue", "list", "-a", "~x"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.assignee.as_deref(), Some("~x"));
    }

    #[test]
    fn issue_list_output_flags() {
        let cli = parse(&["atlassian", "jira", "issue", "list", "--plain", "--csv", "--no-headers", "--no-truncate"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert!(args.plain);
        assert!(args.csv);
        assert!(args.no_headers);
        assert!(args.no_truncate);
    }

    #[test]
    fn issue_list_columns() {
        let cli = parse(&["atlassian", "jira", "issue", "list", "--columns", "KEY,SUMMARY,STATUS"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.columns.as_deref(), Some("KEY,SUMMARY,STATUS"));
    }

    #[test]
    fn issue_list_jql() {
        let cli = parse(&["atlassian", "jira", "issue", "list", "-q", "project = PROJ AND sprint in openSprints()"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.jql.as_deref(), Some("project = PROJ AND sprint in openSprints()"));
    }

    #[test]
    fn issue_list_watching() {
        let cli = parse(&["atlassian", "jira", "issue", "list", "-w"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert!(args.watching);
    }

    #[test]
    fn issue_list_multiple_labels() {
        let cli = parse(&["atlassian", "jira", "issue", "list", "-l", "frontend", "-l", "backend"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.labels, vec!["frontend", "backend"]);
    }

    #[test]
    fn issue_list_paginate() {
        let cli = parse(&["atlassian", "jira", "issue", "list", "--paginate", "50:25"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.paginate.as_deref(), Some("50:25"));
    }

    // ── jira issue get ──────────────────────────────────────────────────────────

    #[test]
    fn issue_get() {
        let cli = parse(&["atlassian", "jira", "issue", "get", "PROJ-123"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Get(args) } } = cli.command else { panic!() };
        assert_eq!(args.key, "PROJ-123");
        assert!(!args.plain);
    }

    #[test]
    fn issue_get_plain() {
        let cli = parse(&["atlassian", "jira", "issue", "get", "PROJ-1", "--plain"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Get(args) } } = cli.command else { panic!() };
        assert!(args.plain);
    }

    #[test]
    fn issue_get_missing_key_errors() {
        parse_err(&["atlassian", "jira", "issue", "get"]);
    }

    // ── jira issue create ───────────────────────────────────────────────────────

    #[test]
    fn issue_create_defaults() {
        let cli = parse(&["atlassian", "jira", "issue", "create"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Create(args) } } = cli.command else { panic!() };
        assert!(args.project.is_none());
        assert_eq!(args.issue_type, "Task");
        assert!(args.summary.is_none());
        assert!(args.parent.is_none());
    }

    #[test]
    fn issue_create_with_args() {
        let cli = parse(&["atlassian", "jira", "issue", "create", "-p", "PROJ", "-t", "Story", "-s", "My story", "--parent", "PROJ-1"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Create(args) } } = cli.command else { panic!() };
        assert_eq!(args.project.as_deref(), Some("PROJ"));
        assert_eq!(args.issue_type, "Story");
        assert_eq!(args.summary.as_deref(), Some("My story"));
        assert_eq!(args.parent.as_deref(), Some("PROJ-1"));
    }

    #[test]
    fn issue_create_multiple_labels() {
        let cli = parse(&["atlassian", "jira", "issue", "create", "-l", "a", "-l", "b"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Create(args) } } = cli.command else { panic!() };
        assert_eq!(args.labels, vec!["a", "b"]);
    }

    // ── jira issue edit ─────────────────────────────────────────────────────────

    #[test]
    fn issue_edit_key_only() {
        let cli = parse(&["atlassian", "jira", "issue", "edit", "PROJ-42"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Edit(args) } } = cli.command else { panic!() };
        assert_eq!(args.key, "PROJ-42");
        assert!(args.summary.is_none());
        assert!(args.labels.is_empty());
    }

    #[test]
    fn issue_edit_all_fields() {
        let cli = parse(&["atlassian", "jira", "issue", "edit", "PROJ-1", "-s", "New title", "-b", "New desc", "-y", "High", "-a", "user123"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Edit(args) } } = cli.command else { panic!() };
        assert_eq!(args.summary.as_deref(), Some("New title"));
        assert_eq!(args.body.as_deref(), Some("New desc"));
        assert_eq!(args.priority.as_deref(), Some("High"));
        assert_eq!(args.assignee.as_deref(), Some("user123"));
    }

    #[test]
    fn issue_edit_label_removal_hyphen() {
        // Labels starting with - must parse as values, not flags
        let cli = parse(&["atlassian", "jira", "issue", "edit", "PROJ-1", "-l", "-old-label", "-l", "new-label"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Edit(args) } } = cli.command else { panic!() };
        assert_eq!(args.labels, vec!["-old-label", "new-label"]);
    }

    #[test]
    fn issue_edit_component_removal_hyphen() {
        let cli = parse(&["atlassian", "jira", "issue", "edit", "PROJ-1", "-C", "-OldTeam", "-C", "NewTeam"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Edit(args) } } = cli.command else { panic!() };
        assert_eq!(args.components, vec!["-OldTeam", "NewTeam"]);
    }

    #[test]
    fn issue_edit_fix_version_removal_hyphen() {
        let cli = parse(&["atlassian", "jira", "issue", "edit", "PROJ-1", "--fix-version", "-1.0", "--fix-version", "2.0"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Edit(args) } } = cli.command else { panic!() };
        assert_eq!(args.fix_versions, vec!["-1.0", "2.0"]);
    }

    #[test]
    fn issue_edit_missing_key_errors() {
        parse_err(&["atlassian", "jira", "issue", "edit"]);
    }

    // ── jira issue assign ───────────────────────────────────────────────────────

    #[test]
    fn issue_assign_with_user() {
        let cli = parse(&["atlassian", "jira", "issue", "assign", "PROJ-1", "Jane Doe"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Assign(args) } } = cli.command else { panic!() };
        assert_eq!(args.key, "PROJ-1");
        assert_eq!(args.assignee.as_deref(), Some("Jane Doe"));
    }

    #[test]
    fn issue_assign_unassign() {
        let cli = parse(&["atlassian", "jira", "issue", "assign", "PROJ-1", "x"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Assign(args) } } = cli.command else { panic!() };
        assert_eq!(args.assignee.as_deref(), Some("x"));
    }

    #[test]
    fn issue_assign_no_user() {
        let cli = parse(&["atlassian", "jira", "issue", "assign", "PROJ-1"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Assign(args) } } = cli.command else { panic!() };
        assert!(args.assignee.is_none());
    }

    // ── jira issue move ─────────────────────────────────────────────────────────

    #[test]
    fn issue_move() {
        let cli = parse(&["atlassian", "jira", "issue", "move", "PROJ-1"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Move(args) } } = cli.command else { panic!() };
        assert_eq!(args.key, "PROJ-1");
        assert!(args.state.is_none());
    }

    #[test]
    fn issue_move_with_state() {
        let cli = parse(&["atlassian", "jira", "issue", "move", "PROJ-1", "In Progress"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Move(args) } } = cli.command else { panic!() };
        assert_eq!(args.state.as_deref(), Some("In Progress"));
    }

    // ── jira issue link / unlink ────────────────────────────────────────────────

    #[test]
    fn issue_link() {
        let cli = parse(&["atlassian", "jira", "issue", "link", "PROJ-1", "PROJ-2", "Blocks"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Link(args) } } = cli.command else { panic!() };
        assert_eq!(args.inward.as_deref(), Some("PROJ-1"));
        assert_eq!(args.outward.as_deref(), Some("PROJ-2"));
        assert_eq!(args.link_type.as_deref(), Some("Blocks"));
    }

    #[test]
    fn issue_link_no_type() {
        let cli = parse(&["atlassian", "jira", "issue", "link", "PROJ-1", "PROJ-2"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Link(args) } } = cli.command else { panic!() };
        assert!(args.link_type.is_none());
    }

    #[test]
    fn issue_unlink() {
        let cli = parse(&["atlassian", "jira", "issue", "unlink", "PROJ-1", "PROJ-2"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::Unlink(args) } } = cli.command else { panic!() };
        assert_eq!(args.inward.as_deref(), Some("PROJ-1"));
        assert_eq!(args.outward.as_deref(), Some("PROJ-2"));
    }

    // ── jira issue view ─────────────────────────────────────────────────────────

    #[test]
    fn issue_view() {
        let cli = parse(&["atlassian", "jira", "issue", "view", "PROJ-123"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::View(args) } } = cli.command else { panic!() };
        assert_eq!(args.key, "PROJ-123");
        assert!(args.comments.is_none());
    }

    #[test]
    fn issue_view_comments() {
        let cli = parse(&["atlassian", "jira", "issue", "view", "PROJ-1", "--comments", "5"]);
        let Commands::Jira { command: JiraCommands::Issue { command: IssueCommands::View(args) } } = cli.command else { panic!() };
        assert_eq!(args.comments, Some(5));
    }

    // ── jira epic list ──────────────────────────────────────────────────────────

    #[test]
    fn epic_list_no_args() {
        let cli = parse(&["atlassian", "jira", "epic", "list"]);
        let Commands::Jira { command: JiraCommands::Epic { command: EpicCommands::List(args) } } = cli.command else { panic!() };
        assert!(args.epic_key.is_none());
        assert!(args.project.is_none());
        assert_eq!(args.order_by, "created");
        assert_eq!(args.max_results, 100);
    }

    #[test]
    fn epic_list_with_epic_key() {
        let cli = parse(&["atlassian", "jira", "epic", "list", "PROJ-123"]);
        let Commands::Jira { command: JiraCommands::Epic { command: EpicCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.epic_key.as_deref(), Some("PROJ-123"));
    }

    #[test]
    fn epic_list_plain() {
        let cli = parse(&["atlassian", "jira", "epic", "list", "PROJ-1", "--plain"]);
        let Commands::Jira { command: JiraCommands::Epic { command: EpicCommands::List(args) } } = cli.command else { panic!() };
        assert!(args.plain);
    }

    #[test]
    fn epic_list_alias_ls() {
        let cli = parse(&["atlassian", "jira", "epic", "ls"]);
        let Commands::Jira { command: JiraCommands::Epic { command: EpicCommands::List(_) } } = cli.command else { panic!() };
    }

    #[test]
    fn epic_list_hyphen_dates() {
        let cli = parse(&["atlassian", "jira", "epic", "list", "--created", "-30d", "--updated-before", "-1w"]);
        let Commands::Jira { command: JiraCommands::Epic { command: EpicCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.created.as_deref(), Some("-30d"));
        assert_eq!(args.updated_before.as_deref(), Some("-1w"));
    }

    // ── jira epic create ────────────────────────────────────────────────────────

    #[test]
    fn epic_create_with_args() {
        let cli = parse(&["atlassian", "jira", "epic", "create", "-p", "PROJ", "-s", "My Epic"]);
        let Commands::Jira { command: JiraCommands::Epic { command: EpicCommands::Create(args) } } = cli.command else { panic!() };
        assert_eq!(args.project.as_deref(), Some("PROJ"));
        assert_eq!(args.summary.as_deref(), Some("My Epic"));
    }

    #[test]
    fn epic_create_defaults() {
        let cli = parse(&["atlassian", "jira", "epic", "create"]);
        let Commands::Jira { command: JiraCommands::Epic { command: EpicCommands::Create(args) } } = cli.command else { panic!() };
        assert!(args.project.is_none());
        assert!(args.summary.is_none());
    }

    // ── issue shortcut (top-level alias) ────────────────────────────────────────

    #[test]
    fn issue_shortcut_get() {
        let cli = parse(&["atlassian", "issue", "get", "PROJ-1"]);
        let Commands::Issue { command: IssueCommands::Get(args) } = cli.command else { panic!() };
        assert_eq!(args.key, "PROJ-1");
    }

    #[test]
    fn issue_shortcut_list() {
        let cli = parse(&["atlassian", "issue", "list", "-p", "PROJ"]);
        let Commands::Issue { command: IssueCommands::List(args) } = cli.command else { panic!() };
        assert_eq!(args.project.as_deref(), Some("PROJ"));
    }

    // ── confluence space list ───────────────────────────────────────────────────

    #[test]
    fn confluence_space_list_defaults() {
        let cli = parse(&["atlassian", "confluence", "space", "list"]);
        let Commands::Confluence { command: ConfluenceCommands::Space { command: SpaceCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.space_type, "global");
        assert_eq!(args.limit, 50);
        assert_eq!(args.start, 0);
        assert!(!args.plain);
        assert!(args.name.is_none());
    }

    #[test]
    fn confluence_space_list_personal() {
        let cli = parse(&["atlassian", "confluence", "space", "list", "-t", "personal"]);
        let Commands::Confluence { command: ConfluenceCommands::Space { command: SpaceCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.space_type, "personal");
    }

    #[test]
    fn confluence_space_list_name_filter() {
        let cli = parse(&["atlassian", "confluence", "space", "list", "-n", "Engineering"]);
        let Commands::Confluence { command: ConfluenceCommands::Space { command: SpaceCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.name.as_deref(), Some("Engineering"));
    }

    // ── confluence page list ────────────────────────────────────────────────────

    #[test]
    fn confluence_page_list_no_space() {
        let cli = parse(&["atlassian", "confluence", "page", "list"]);
        let Commands::Confluence { command: ConfluenceCommands::Page { command: PageCommands::List(args) } } = cli.command else { panic!() };
        assert!(args.space.is_none());
        assert_eq!(args.limit, 50);
    }

    #[test]
    fn confluence_page_list_with_space() {
        let cli = parse(&["atlassian", "confluence", "page", "list", "-s", "ENGR"]);
        let Commands::Confluence { command: ConfluenceCommands::Page { command: PageCommands::List(args) } } = cli.command else { panic!() };
        assert_eq!(args.space.as_deref(), Some("ENGR"));
    }

    // ── confluence page get ─────────────────────────────────────────────────────

    #[test]
    fn confluence_page_get() {
        let cli = parse(&["atlassian", "confluence", "page", "get", "12345"]);
        let Commands::Confluence { command: ConfluenceCommands::Page { command: PageCommands::Get(args) } } = cli.command else { panic!() };
        assert_eq!(args.id, "12345");
    }

    #[test]
    fn confluence_page_get_missing_id_errors() {
        parse_err(&["atlassian", "confluence", "page", "get"]);
    }

    // ── confluence page view ────────────────────────────────────────────────────

    #[test]
    fn confluence_page_view() {
        let cli = parse(&["atlassian", "confluence", "page", "view", "67890"]);
        let Commands::Confluence { command: ConfluenceCommands::Page { command: PageCommands::View(args) } } = cli.command else { panic!() };
        assert_eq!(args.id, "67890");
        assert!(!args.raw);
    }

    #[test]
    fn confluence_page_view_raw() {
        let cli = parse(&["atlassian", "confluence", "page", "view", "67890", "--raw"]);
        let Commands::Confluence { command: ConfluenceCommands::Page { command: PageCommands::View(args) } } = cli.command else { panic!() };
        assert!(args.raw);
    }

    #[test]
    fn confluence_page_view_missing_id_errors() {
        parse_err(&["atlassian", "confluence", "page", "view"]);
    }

    // ── confluence search ───────────────────────────────────────────────────────

    #[test]
    fn confluence_search() {
        let cli = parse(&["atlassian", "confluence", "search", "my query"]);
        let Commands::Confluence { command: ConfluenceCommands::Search(args) } = cli.command else { panic!() };
        assert_eq!(args.query, "my query");
        assert!(args.space.is_none());
        assert_eq!(args.limit, 20);
        assert!(!args.plain);
    }

    #[test]
    fn confluence_search_with_space() {
        let cli = parse(&["atlassian", "confluence", "search", "deploy", "-s", "ENGR", "--plain"]);
        let Commands::Confluence { command: ConfluenceCommands::Search(args) } = cli.command else { panic!() };
        assert_eq!(args.space.as_deref(), Some("ENGR"));
        assert!(args.plain);
    }

    #[test]
    fn confluence_search_missing_query_errors() {
        parse_err(&["atlassian", "confluence", "search"]);
    }
}
