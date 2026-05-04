use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum EpicCommands {
    /// List epics, or list issues within an epic
    #[command(alias = "ls", alias = "lists")]
    List(EpicListArgs),
    /// Create a new epic
    Create(EpicCreateArgs),
}

#[derive(Args)]
pub struct EpicCreateArgs {
    /// Project key
    #[arg(short = 'p', long = "project")]
    pub project: Option<String>,

    /// Epic summary / title
    #[arg(short = 's', long = "summary")]
    pub summary: Option<String>,

    /// Description
    #[arg(short = 'd', long = "description")]
    pub description: Option<String>,

    /// Assignee account ID
    #[arg(short = 'a', long = "assignee")]
    pub assignee: Option<String>,

    /// Priority (e.g. High, Low)
    #[arg(short = 'y', long = "priority")]
    pub priority: Option<String>,

    /// Labels (can be specified multiple times)
    #[arg(short = 'l', long = "label")]
    pub labels: Vec<String>,

    /// Components (can be specified multiple times)
    #[arg(long = "component")]
    pub components: Vec<String>,
}

#[derive(Args)]
pub struct EpicListArgs {
    /// Epic key to list child issues for (e.g. PROJ-123).
    /// Omit to list all epics in the project.
    pub epic_key: Option<String>,

    #[arg(short = 'p', long = "project")]
    pub project: Option<String>,

    /// Filter by status; prefix with ~ to negate (can repeat)
    #[arg(short = 's', long = "status")]
    pub status: Vec<String>,

    /// Filter by assignee; 'x' = unassigned, '~x' = assigned to someone
    #[arg(short = 'a', long = "assignee", allow_hyphen_values = true)]
    pub assignee: Option<String>,

    /// Filter by reporter; supports ~ negation
    #[arg(short = 'r', long = "reporter", allow_hyphen_values = true)]
    pub reporter: Option<String>,

    #[arg(short = 'y', long = "priority")]
    pub priority: Option<String>,

    /// Filter by resolution
    #[arg(short = 'R', long = "resolution")]
    pub resolution: Option<String>,

    /// Filter by label (can repeat; prefix with ~ to negate)
    #[arg(short = 'l', long = "label")]
    pub labels: Vec<String>,

    #[arg(short = 'C', long = "component")]
    pub component: Option<String>,

    #[arg(long = "order-by", default_value = "created")]
    pub order_by: String,

    #[arg(long = "reverse")]
    pub reverse: bool,

    /// Filter by created date: today, week, month, year, -7d, yyyy-mm-dd
    #[arg(long = "created", allow_hyphen_values = true)]
    pub created: Option<String>,

    #[arg(long = "created-after", allow_hyphen_values = true)]
    pub created_after: Option<String>,

    #[arg(long = "created-before", allow_hyphen_values = true)]
    pub created_before: Option<String>,

    #[arg(long = "updated", allow_hyphen_values = true)]
    pub updated: Option<String>,

    #[arg(long = "updated-after", allow_hyphen_values = true)]
    pub updated_after: Option<String>,

    #[arg(long = "updated-before", allow_hyphen_values = true)]
    pub updated_before: Option<String>,

    /// Issues you accessed recently
    #[arg(long = "history")]
    pub history: bool,

    #[arg(short = 'w', long = "watching")]
    pub watching: bool,

    /// Raw JQL (used in addition to project context)
    #[arg(short = 'q', long = "jql")]
    pub jql: Option<String>,

    /// Paginate: <limit> or <from>:<limit>
    #[arg(long = "paginate")]
    pub paginate: Option<String>,

    #[arg(long = "max-results", default_value = "100")]
    pub max_results: u32,

    /// Display in table view (same as --plain for epic key mode)
    #[arg(long = "table")]
    pub table: bool,

    /// Plain table output (no TUI)
    #[arg(long = "plain")]
    pub plain: bool,

    #[arg(long = "no-headers")]
    pub no_headers: bool,

    #[arg(long = "no-truncate")]
    pub no_truncate: bool,

    /// Comma-separated columns: TYPE,KEY,SUMMARY,STATUS,ASSIGNEE,REPORTER,PRIORITY,RESOLUTION,CREATED,UPDATED,LABELS
    #[arg(long = "columns")]
    pub columns: Option<String>,

    #[arg(long = "delimiter", default_value = "\t")]
    pub delimiter: String,

    #[arg(long = "raw")]
    pub raw: bool,

    #[arg(long = "csv")]
    pub csv: bool,
}
