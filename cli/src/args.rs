use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};
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
pub(crate) enum AgentFileCommand {
    /// Delete a file from a task named by its task ID.
    Delete {
        space: String,
        task_id: String,
        file: String,
        #[arg(long)]
        yes: bool,
    },
    /// Print a file's short-lived download URL.
    Url { space: String, file: String },
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum Command {
    /// Manage your agents.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Read and delete task files.
    #[command(name = "file")]
    AgentFile {
        #[command(subcommand)]
        command: AgentFileCommand,
    },
    /// Delete a task's checkpoints.
    Checkpoint {
        #[command(subcommand)]
        command: CheckpointCommand,
    },
    /// Read and change a space's Knowledge.
    Knowledge {
        #[command(subcommand)]
        command: KnowledgeCommand,
    },
    /// Sign in through a browser on this or another computer.
    Login {
        server_url: String,
        #[arg(long)]
        no_browser: bool,
        #[arg(long, conflicts_with = "no_browser")]
        device: bool,
    },
    /// End your sign-in and remove local credentials.
    Logout,
    /// Manage a space's members.
    Member {
        #[command(subcommand)]
        command: MemberCommand,
    },
    /// Submit, read and decide Knowledge requests.
    #[command(name = "request")]
    PersonRequest {
        #[command(subcommand)]
        command: PersonRequestCommand,
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
pub(crate) enum KnowledgeCommand {
    /// Change an entry's name, summary or content.
    #[command(group(ArgGroup::new("changes").required(true).multiple(true)
        .args(["name", "summary", "content", "content_file"])))]
    Change {
        space: String,
        entry: String,
        #[command(flatten)]
        changes: KnowledgeChanges,
    },
    /// Create a knowledge entry directly.
    #[command(group(ArgGroup::new("source").required(true)
        .args(["content", "content_file"])))]
    Create {
        space: String,
        name: String,
        #[arg(long)]
        summary: String,
        #[command(flatten)]
        content: ContentOptions,
    },
    /// Delete an entry directly.
    Delete {
        space: String,
        entry: String,
        #[arg(long)]
        yes: bool,
    },
    /// List one page of deleted entries.
    Deleted {
        space: String,
        #[command(flatten)]
        page: Page,
    },
    /// List one page of an entry's versions, newest first.
    History {
        space: String,
        entry: String,
        #[command(flatten)]
        page: Page,
    },
    /// List one page of entries and prefixes, or search by keyword.
    List {
        space: String,
        #[arg(long)]
        prefix: Option<String>,
        #[arg(long)]
        keyword: Option<String>,
        #[command(flatten)]
        page: Page,
    },
    /// Restore the content and summary of an old version.
    Rollback {
        space: String,
        entry: String,
        #[arg(allow_negative_numbers = true)]
        version: i64,
    },
    /// Show an entry's latest or chosen version by name or key.
    Show {
        space: String,
        entry: String,
        #[arg(long, allow_negative_numbers = true)]
        version: Option<i64>,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum CheckpointCommand {
    /// Delete a checkpoint named by the time printed in its Scratchpad.
    Delete {
        space: String,
        task_id: String,
        #[arg(allow_negative_numbers = true)]
        checkpoint: String,
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
pub(crate) enum MemberCommand {
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

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum PersonRequestCommand {
    /// Approve a request and apply its Knowledge change.
    Approve {
        space: String,
        #[arg(value_name = "request")]
        person_request: String,
    },
    /// Propose changes to an entry's name, summary or content.
    #[command(group(ArgGroup::new("changes").required(true).multiple(true)
        .args(["name", "summary", "content", "content_file"])))]
    Change {
        space: String,
        entry: String,
        #[command(flatten)]
        changes: KnowledgeChanges,
    },
    /// Propose creating a knowledge entry.
    #[command(group(ArgGroup::new("source").required(true)
        .args(["content", "content_file"])))]
    Create {
        space: String,
        name: String,
        #[arg(long)]
        summary: String,
        #[command(flatten)]
        content: ContentOptions,
    },
    /// Propose deletion without confirmation; approval deletes the entry.
    Delete { space: String, entry: String },
    /// List one page of requests, optionally by status and keyword.
    List {
        space: String,
        #[arg(long, value_enum)]
        status: Option<PersonRequestStatus>,
        #[arg(long)]
        keyword: Option<String>,
        #[command(flatten)]
        page: Page,
    },
    /// Reject a request, or withdraw your own party's request.
    Reject {
        space: String,
        #[arg(value_name = "request")]
        person_request: String,
        #[arg(long)]
        reason: String,
    },
    /// Show a request's fields, content and decision.
    Show {
        space: String,
        #[arg(value_name = "request")]
        person_request: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum PersonRequestStatus {
    Approved,
    Pending,
    Rejected,
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
    /// Delete a task, its checkpoints, files and their content.
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
    /// Read one page of a task's checkpoints in time order.
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
pub(crate) struct ContentOptions {
    #[arg(long, conflicts_with = "content_file")]
    pub content: Option<String>,
    /// Read UTF-8 content from a file, or '-' for standard input.
    #[arg(long, value_name = "PATH|-")]
    pub content_file: Option<PathBuf>,
}

#[derive(Args, Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[group(multiple = true)]
pub(crate) struct KnowledgeChanges {
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub summary: Option<String>,
    #[command(flatten)]
    pub content: ContentOptions,
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

impl PersonRequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Pending => "pending",
            Self::Rejected => "rejected",
        }
    }
}
