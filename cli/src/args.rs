use clap::{Args, Parser, Subcommand, ValueEnum};
use typed_builder::TypedBuilder;

use crate::constants::HUMAN_OUTPUT;

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
    /// Sign in through a browser on this or another computer.
    Login {
        server_url: String,
        #[arg(long)]
        no_browser: bool,
        #[arg(long, conflicts_with = "no_browser")]
        device: bool,
    },
    /// End your login session and remove local credentials.
    Logout,
    /// Delete a task's notes.
    Note {
        #[command(subcommand)]
        command: NoteCommand,
    },
    /// Manage a space's participants.
    Participant {
        #[command(subcommand)]
        command: ParticipantCommand,
    },
    /// Manage your spaces.
    Space {
        #[command(subcommand)]
        command: SpaceCommand,
    },
    /// Read and manage a space's tasks.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Find people to add to a space.
    User {
        #[command(subcommand)]
        command: UserCommand,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum NoteCommand {
    /// Delete a note named by the time printed in its Scratchpad.
    Delete {
        space: String,
        task_id: String,
        #[arg(allow_negative_numbers = true)]
        note: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub(crate) enum OutputMode {
    #[default]
    Human,
    Json,
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum ParticipantCommand {
    /// Add a person as a contributor or viewer.
    Add {
        space: String,
        username: String,
        #[arg(long, value_enum)]
        role: AddedRole,
    },
    /// List one page of people and their agents.
    List {
        space: String,
        #[command(flatten)]
        page: Page,
    },
    /// Remove a person and their agents without confirmation.
    Remove { space: String, username: String },
    /// Change a person's role.
    SetRole {
        space: String,
        username: String,
        #[arg(long, value_enum)]
        role: Role,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum AddedRole {
    Contributor,
    Viewer,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum Role {
    Admin,
    Contributor,
    Viewer,
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum SpaceCommand {
    /// Change a space's name, summary or both.
    Change {
        space: String,
        #[arg(long, required_unless_present = "summary")]
        name: Option<String>,
        #[arg(long)]
        summary: Option<String>,
    },
    /// Create a space you own.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        summary: String,
    },
    /// List one page of your spaces.
    List(Page),
    /// Show a space's fields.
    Show { space: String },
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum UserCommand {
    /// List one page of usernames.
    List(Page),
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum TaskCommand {
    /// End a task named by its task ID.
    Close { space: String, task_id: String },
    /// Delete a task and its notes.
    Delete {
        space: String,
        task_id: String,
        #[arg(long)]
        yes: bool,
    },
    /// List one page of tasks, newest activity first.
    List {
        space: String,
        #[command(flatten)]
        page: Page,
        #[command(flatten)]
        filters: TaskFilters,
    },
    /// Read one page of a task's notes in time order.
    Scratchpad {
        space: String,
        task: String,
        #[command(flatten)]
        page: Page,
    },
    /// Show a task's fields and task ID.
    Show { space: String, task: String },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum TaskStatus {
    Ended,
    #[value(name = "in progress")]
    InProgress,
}

#[derive(Clone, Debug, Parser, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[command(name = "lorebay", version, about = "The Lorebay command-line client")]
pub(crate) struct Cli {
    #[arg(long, global = true, value_enum, default_value = HUMAN_OUTPUT)]
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

#[derive(Args, Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskFilters {
    #[arg(long)]
    pub person: Option<String>,
    #[arg(long)]
    pub agent: Option<String>,
    #[arg(long, value_enum)]
    pub status: Option<TaskStatus>,
    #[arg(long)]
    pub own_open: bool,
    #[arg(long)]
    pub keyword: Option<String>,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ended => "ended",
            Self::InProgress => "in progress",
        }
    }
}

impl AddedRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contributor => "contributor",
            Self::Viewer => "viewer",
        }
    }
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Contributor => "contributor",
            Self::Viewer => "viewer",
        }
    }
}
