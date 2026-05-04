use clap::Args;

#[derive(Args)]
pub struct MeArgs {
    /// Output the account ID instead of display name
    #[arg(long = "account-id")]
    pub account_id: bool,
}
