use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum ConfluenceCommands {
    /// Manage Confluence spaces
    Space {
        #[command(subcommand)]
        command: SpaceCommands,
    },
    /// Manage Confluence pages
    Page {
        #[command(subcommand)]
        command: PageCommands,
    },
    /// Search Confluence content using CQL
    Search(SearchArgs),
}

#[derive(Subcommand)]
pub enum SpaceCommands {
    /// List all spaces
    List(SpaceListArgs),
}

#[derive(Args)]
pub struct SpaceListArgs {
    /// Filter by space type: global, personal (default: global)
    #[arg(long, short = 't', default_value = "global")]
    pub space_type: String,
    /// Filter spaces by name (case-insensitive substring match)
    #[arg(long, short = 'n')]
    pub name: Option<String>,
    /// Maximum results to return
    #[arg(long, default_value = "50")]
    pub limit: u32,
    /// Pagination start offset
    #[arg(long, default_value = "0")]
    pub start: u32,
    /// Output as plain text table (no TUI)
    #[arg(long, short = 'p')]
    pub plain: bool,
}

#[derive(Subcommand)]
pub enum PageCommands {
    /// List pages in a space
    List(PageListArgs),
    /// Show page details (non-TUI)
    Get(PageGetArgs),
    /// View page in interactive TUI
    View(PageViewArgs),
}

#[derive(Args)]
pub struct PageListArgs {
    /// Space key to list pages from (e.g. PROJ); omit to start with an empty list and use / to search
    #[arg(short = 's', long)]
    pub space: Option<String>,
    /// Maximum results to return
    #[arg(long, default_value = "50")]
    pub limit: u32,
    /// Pagination start offset
    #[arg(long, default_value = "0")]
    pub start: u32,
    /// Output as plain text table (no TUI)
    #[arg(long, short = 'p')]
    pub plain: bool,
}

#[derive(Args)]
pub struct PageGetArgs {
    /// Page ID
    pub id: String,
}

#[derive(Args)]
pub struct PageViewArgs {
    /// Page ID
    pub id: String,
    /// Print raw ADF JSON body instead of rendering
    #[arg(long)]
    pub raw: bool,
}

#[derive(Args)]
pub struct SearchArgs {
    /// Search query — plain text (wrapped in CQL text~"…") or raw CQL expression
    pub query: String,
    /// Scope search to a specific space key
    #[arg(short = 's', long)]
    pub space: Option<String>,
    /// Maximum results to return
    #[arg(long, default_value = "20")]
    pub limit: u32,
    /// Output as plain text table (no TUI)
    #[arg(long, short = 'p')]
    pub plain: bool,
}
