use clap::{Args, Parser, Subcommand, ValueEnum};
use typed_builder::TypedBuilder;

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum AgentCommand {
    /// List one page of your agents.
    List(Page),
    /// Revoke one of your agents.
    Revoke {
        name: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum Command {
    /// Manage your agents.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Sign in through your browser.
    Login {
        server_url: String,
        #[arg(long)]
        no_browser: bool,
    },
    /// End your login session and remove local credentials.
    Logout,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub(crate) enum OutputMode {
    #[default]
    Human,
    Json,
}

#[derive(Clone, Debug, Parser, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[command(name = "lorebay", version, about = "The Lorebay command-line client")]
pub(crate) struct Cli {
    #[arg(long, global = true, value_enum, default_value = "human")]
    pub output: OutputMode,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Args, Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Page {
    #[arg(long)]
    pub page_size: Option<u32>,
    #[arg(long)]
    pub cursor: Option<String>,
}
