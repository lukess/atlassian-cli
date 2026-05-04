use clap::{Parser, Subcommand};

pub mod confluence;
pub mod epic;
pub mod issue;
pub mod me;
pub mod project;

#[cfg(test)]
mod tests;

#[derive(Parser)]
#[command(
    name = "atlassian",
    version,
    about = "Atlassian CLI - interact with Jira from the command line",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Interact with Jira
    Jira {
        #[command(subcommand)]
        command: JiraCommands,
    },
    /// Interact with Confluence
    Confluence {
        #[command(subcommand)]
        command: confluence::ConfluenceCommands,
    },
    /// Shortcut: issue commands (alias for jira issue)
    Issue {
        #[command(subcommand)]
        command: issue::IssueCommands,
    },
    /// Generate shell completions (e.g. `atlassian completion zsh`)
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
pub enum JiraCommands {
    /// Manage issues
    Issue {
        #[command(subcommand)]
        command: issue::IssueCommands,
    },
    /// Manage epics
    Epic {
        #[command(subcommand)]
        command: epic::EpicCommands,
    },
    /// Manage projects
    Project {
        #[command(subcommand)]
        command: project::ProjectCommands,
    },
    /// Show current user info
    Me(me::MeArgs),
}
