use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum IssueCommands {
    /// List issues
    List(ListArgs),
    /// Get an issue by key
    Get(GetArgs),
    /// Create a new issue
    Create(CreateArgs),
    /// Edit an issue
    Edit(EditArgs),
    /// Assign an issue to a user
    Assign(AssignArgs),
    /// Transition an issue to a new status
    Move(MoveArgs),
    /// Link two issues
    Link(LinkArgs),
    /// Remove a link between two issues
    Unlink(UnlinkArgs),
    /// View issue in browser
    View(ViewArgs),
}

#[derive(Args)]
pub struct ListArgs {
    /// Optional free-text search query (appended as text ~ "..." in JQL)
    pub query: Option<String>,

    #[arg(short = 'p', long = "project")]
    pub project: Option<String>,

    /// Filter by status; prefix with ~ to negate e.g. -s~Done (can repeat)
    #[arg(short = 's', long = "status")]
    pub status: Vec<String>,

    /// Filter by assignee; 'x' = unassigned, '~x' = assigned to someone, '~name' = not this person
    #[arg(short = 'a', long = "assignee", allow_hyphen_values = true)]
    pub assignee: Option<String>,

    /// Filter by reporter; supports same ~negation as assignee
    #[arg(short = 'r', long = "reporter", allow_hyphen_values = true)]
    pub reporter: Option<String>,

    #[arg(short = 'y', long = "priority")]
    pub priority: Option<String>,

    #[arg(short = 't', long = "type", name = "type")]
    pub issue_type: Option<String>,

    /// Filter by resolution
    #[arg(short = 'R', long = "resolution")]
    pub resolution: Option<String>,

    /// Filter by parent issue key
    #[arg(short = 'P', long = "parent")]
    pub parent: Option<String>,

    /// Filter by label (can repeat; prefix with ~ to negate)
    #[arg(short = 'l', long = "label")]
    pub labels: Vec<String>,

    #[arg(short = 'C', long = "component")]
    pub component: Option<String>,

    #[arg(long = "fix-version")]
    pub fix_version: Option<String>,

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

    /// Raw JQL (used in addition to project context, same as jira-cli -q)
    #[arg(short = 'q', long = "jql")]
    pub jql: Option<String>,

    /// Paginate: <limit> or <from>:<limit> (default 0:100)
    #[arg(long = "paginate")]
    pub paginate: Option<String>,

    /// Maximum results (ignored if --paginate is set)
    #[arg(long = "max-results", default_value = "100")]
    pub max_results: u32,

    /// Plain table output (no TUI)
    #[arg(long = "plain")]
    pub plain: bool,

    /// Hide table headers in plain mode
    #[arg(long = "no-headers")]
    pub no_headers: bool,

    /// Show all columns without truncation in plain mode
    #[arg(long = "no-truncate")]
    pub no_truncate: bool,

    /// Comma-separated columns to display: TYPE,KEY,SUMMARY,STATUS,ASSIGNEE,REPORTER,PRIORITY,RESOLUTION,CREATED,UPDATED,LABELS
    #[arg(long = "columns")]
    pub columns: Option<String>,

    /// Column delimiter in plain mode
    #[arg(long = "delimiter", default_value = "\t")]
    pub delimiter: String,

    /// Print raw JSON output
    #[arg(long = "raw")]
    pub raw: bool,

    /// Print output in CSV format
    #[arg(long = "csv")]
    pub csv: bool,
}

#[derive(Args)]
pub struct GetArgs {
    /// Issue key (e.g. PROJ-123)
    pub key: String,

    /// Plain output (no color)
    #[arg(long = "plain")]
    pub plain: bool,
}

#[derive(Args)]
pub struct CreateArgs {
    /// Project key
    #[arg(short = 'p', long = "project")]
    pub project: Option<String>,

    /// Issue type (e.g. Bug, Story, Task)
    #[arg(short = 't', long = "type", default_value = "Task")]
    pub issue_type: String,

    /// Summary / title
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

    /// Parent issue / epic key (e.g. PROJ-1); sets the issue's parent
    #[arg(long = "parent")]
    pub parent: Option<String>,
}

#[derive(Args)]
pub struct EditArgs {
    /// Issue key (e.g. PROJ-123)
    pub key: String,

    /// New summary / title
    #[arg(short = 's', long = "summary")]
    pub summary: Option<String>,

    /// New description
    #[arg(short = 'b', long = "body")]
    pub body: Option<String>,

    /// Priority (e.g. High, Low)
    #[arg(short = 'y', long = "priority")]
    pub priority: Option<String>,

    /// Add or remove labels; prefix with - to remove (e.g. --label -old --label new)
    #[arg(short = 'l', long = "label", allow_hyphen_values = true)]
    pub labels: Vec<String>,

    /// Add or remove components; prefix with - to remove
    #[arg(short = 'C', long = "component", allow_hyphen_values = true)]
    pub components: Vec<String>,

    /// Add or remove fix versions; prefix with - to remove
    #[arg(long = "fix-version", allow_hyphen_values = true)]
    pub fix_versions: Vec<String>,

    /// Assignee account ID (use 'x' to unassign, 'default' for default assignee)
    #[arg(short = 'a', long = "assignee")]
    pub assignee: Option<String>,
}

#[derive(Args)]
pub struct AssignArgs {
    /// Issue key (e.g. PROJ-123)
    pub key: String,

    /// User to assign: display name, account ID, 'x' to unassign, 'default' for default assignee
    pub assignee: Option<String>,
}

#[derive(Args)]
pub struct LinkArgs {
    /// Inward issue key (e.g. PROJ-1)
    pub inward: Option<String>,

    /// Outward issue key (e.g. PROJ-2)
    pub outward: Option<String>,

    /// Link type name (e.g. Blocks, Cloners, Duplicate); omit to choose interactively
    pub link_type: Option<String>,
}

#[derive(Args)]
pub struct UnlinkArgs {
    /// First issue key
    pub inward: Option<String>,

    /// Second issue key
    pub outward: Option<String>,
}

#[derive(Args)]
pub struct MoveArgs {
    /// Issue key (e.g. PROJ-123)
    pub key: String,

    /// Target status/transition name (e.g. "In Progress", "Done")
    pub state: Option<String>,
}

#[derive(Args)]
pub struct ViewArgs {
    /// Issue key (e.g. PROJ-123)
    pub key: String,

    /// Number of most-recent comments to show (default: all)
    #[arg(long = "comments")]
    pub comments: Option<usize>,
}
