use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum ProjectCommands {
    /// List all projects
    List(ProjectListArgs),
    /// Get project details
    Get(ProjectGetArgs),
}

#[derive(Args)]
pub struct ProjectListArgs {
    /// Maximum results
    #[arg(long = "max-results", default_value = "50")]
    pub max_results: u32,

    /// Plain output
    #[arg(long = "plain", short = 'P')]
    pub plain: bool,
}

#[derive(Args)]
pub struct ProjectGetArgs {
    /// Project key (e.g. PROJ)
    pub key: String,
}
