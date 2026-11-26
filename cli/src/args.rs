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
        #[arg(help = "Agent name to revoke")]
        name: String,
        #[arg(long, help = "Confirm revocation without prompting")]
        yes: bool,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum AgentFileCommand {
    /// Delete a file from a task named by its task ID.
    Delete {
        #[arg(help = "Space containing the file")]
        space: String,
        #[arg(help = "Task ID containing the file")]
        task_id: String,
        #[arg(help = "File key, such as FL-1")]
        file: String,
        #[arg(long, help = "Confirm deletion without prompting")]
        yes: bool,
    },
    /// Print a file's short-lived download URL.
    Url {
        #[arg(help = "Space containing the file")]
        space: String,
        #[arg(help = "File key, such as FL-1")]
        file: String,
    },
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
        #[arg(help = "Lorebay server URL, such as https://lorebay.example.com")]
        server_url: String,
        #[arg(long, help = "Print the sign-in URL instead of opening a browser")]
        no_browser: bool,
        #[arg(
            long,
            conflicts_with = "no_browser",
            help = "Sign in with a short code on another computer"
        )]
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
        #[arg(help = "Space containing the entry")]
        space: String,
        #[arg(help = "Entry key, such as KN-1, or entry name")]
        entry: String,
        #[arg(long, help = "Current version being replaced")]
        version: i64,
        #[command(flatten)]
        changes: KnowledgeChanges,
    },
    /// Create a knowledge entry directly.
    #[command(group(ArgGroup::new("source").required(true)
        .args(["content", "content_file"])))]
    Create {
        #[arg(help = "Space to create the entry in")]
        space: String,
        #[arg(help = "Entry name, such as deploy/staging.md")]
        name: String,
        #[arg(long, help = "Short summary of the entry")]
        summary: String,
        #[command(flatten)]
        content: ContentOptions,
    },
    /// Delete an entry directly.
    Delete {
        #[arg(help = "Space containing the entry")]
        space: String,
        #[arg(help = "Entry key, such as KN-1, or entry name")]
        entry: String,
        #[arg(long, help = "Current version being deleted")]
        version: i64,
        #[arg(long, help = "Confirm deletion without prompting")]
        yes: bool,
    },
    /// List one page of deleted entries.
    Deleted {
        #[arg(help = "Space to list deleted entries from")]
        space: String,
        #[command(flatten)]
        page: Page,
    },
    /// List one page of an entry's versions, newest first.
    History {
        #[arg(help = "Space containing the entry")]
        space: String,
        #[arg(help = "Entry key, such as KN-1, or entry name")]
        entry: String,
        #[command(flatten)]
        page: Page,
    },
    /// List one page of entries and prefixes, or search by keyword.
    List {
        #[arg(help = "Space to list entries from")]
        space: String,
        #[arg(long, help = "Filter entries by name prefix")]
        prefix: Option<String>,
        #[arg(long, help = "Search entries by keyword")]
        keyword: Option<String>,
        #[command(flatten)]
        page: Page,
    },
    /// Restore the content and summary of an old version.
    Rollback {
        #[arg(help = "Space containing the entry")]
        space: String,
        #[arg(help = "Entry key, such as KN-1, or entry name")]
        entry: String,
        #[arg(allow_negative_numbers = true, help = "Version number to restore")]
        source_version: i64,
        #[arg(long, help = "Current version being replaced")]
        version: i64,
    },
    /// Show an entry's latest or chosen version by name or key.
    Show {
        #[arg(help = "Space containing the entry")]
        space: String,
        #[arg(help = "Entry key, such as KN-1, or entry name")]
        entry: String,
        #[arg(long, allow_negative_numbers = true, help = "Version number to show")]
        version: Option<i64>,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum CheckpointCommand {
    /// Delete a checkpoint named by the time printed in its Scratchpad.
    Delete {
        #[arg(help = "Space containing the task")]
        space: String,
        #[arg(help = "Task ID containing the checkpoint")]
        task_id: String,
        #[arg(
            allow_negative_numbers = true,
            help = "Checkpoint time printed in the Scratchpad"
        )]
        checkpoint: String,
        #[arg(long, help = "Confirm deletion without prompting")]
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
        #[arg(help = "Space to add the person to")]
        space: String,
        #[arg(help = "Username of the person to add")]
        username: String,
        #[arg(long, value_enum, help = "Space role for the person")]
        role: AddedRole,
    },
    /// List one page of people and their agents.
    List {
        #[arg(help = "Space to list members from")]
        space: String,
        #[command(flatten)]
        page: Page,
    },
    /// Remove a person and their agents without confirmation.
    Remove {
        #[arg(help = "Space to remove the person from")]
        space: String,
        #[arg(help = "Username of the person to remove")]
        username: String,
    },
    /// Change a person's role.
    SetRole {
        #[arg(help = "Space containing the member")]
        space: String,
        #[arg(help = "Username of the member")]
        username: String,
        #[arg(long, value_enum, help = "New space role for the member")]
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
        #[arg(help = "Space containing the request")]
        space: String,
        #[arg(value_name = "REQUEST", help = "Request key, such as RQ-1")]
        person_request: String,
        #[arg(long, help = "Request revision you read")]
        revision: i64,
    },
    /// Propose changes to an entry's name, summary or content.
    #[command(group(ArgGroup::new("changes").required(true).multiple(true)
        .args(["name", "summary", "content", "content_file"])))]
    Change {
        #[arg(help = "Space containing the entry")]
        space: String,
        #[arg(help = "Entry key, such as KN-1, or entry name")]
        entry: String,
        #[command(flatten)]
        changes: KnowledgeChanges,
    },
    /// Propose creating a knowledge entry.
    #[command(group(ArgGroup::new("source").required(true)
        .args(["content", "content_file"])))]
    Create {
        #[arg(help = "Space to propose the entry in")]
        space: String,
        #[arg(help = "Entry name, such as deploy/staging.md")]
        name: String,
        #[arg(long, help = "Short summary of the entry")]
        summary: String,
        #[command(flatten)]
        content: ContentOptions,
    },
    /// Propose deletion without confirmation; approval deletes the entry.
    Delete {
        #[arg(help = "Space containing the entry")]
        space: String,
        #[arg(help = "Entry key, such as KN-1, or entry name")]
        entry: String,
    },
    /// List one page of requests, optionally by status and keyword.
    List {
        #[arg(help = "Space to list requests from")]
        space: String,
        #[arg(long, value_enum, help = "Filter by request status")]
        status: Option<PersonRequestStatus>,
        #[arg(long, help = "Search requests by keyword")]
        keyword: Option<String>,
        #[command(flatten)]
        page: Page,
    },
    /// Reject a request, or withdraw your own party's request.
    Reject {
        #[arg(help = "Space containing the request")]
        space: String,
        #[arg(value_name = "REQUEST", help = "Request key, such as RQ-1")]
        person_request: String,
        #[arg(
            long,
            help = "One-line reason for rejecting or withdrawing the request"
        )]
        reason: String,
        #[arg(long, help = "Request revision you read")]
        revision: i64,
    },
    /// Revise your party's pending request, keeping its key.
    Revise {
        #[arg(help = "Space containing the request")]
        space: String,
        #[arg(value_name = "REQUEST", help = "Request key, such as RQ-1")]
        person_request: String,
        #[command(flatten)]
        changes: KnowledgeChanges,
    },
    /// Show a request's fields, content and decision.
    Show {
        #[arg(help = "Space containing the request")]
        space: String,
        #[arg(value_name = "REQUEST", help = "Request key, such as RQ-1")]
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
        #[arg(help = "Space key, such as SPACE-1")]
        space: String,
        #[arg(long, required_unless_present = "summary", help = "New space name")]
        name: Option<String>,
        #[arg(long, help = "New space summary")]
        summary: Option<String>,
    },
    /// Create a space you own.
    Create {
        #[arg(long, help = "Space name")]
        name: String,
        #[arg(long, help = "Short summary of the space")]
        summary: String,
    },
    /// List one page of your spaces.
    List(Page),
    /// Show a space's fields.
    Show {
        #[arg(help = "Space key, such as SPACE-1")]
        space: String,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum UserCommand {
    /// List one page of usernames.
    List(Page),
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum TaskCommand {
    /// Cancel a task named by its task ID, with a one-line reason.
    Cancel {
        #[arg(help = "Space containing the task")]
        space: String,
        #[arg(help = "Task ID to cancel")]
        task_id: String,
        #[arg(long, help = "One-line reason for canceling the task")]
        reason: String,
    },
    /// Delete a task, its checkpoints, files and their content.
    Delete {
        #[arg(help = "Space containing the task")]
        space: String,
        #[arg(help = "Task ID to delete")]
        task_id: String,
        #[arg(long, help = "Confirm deletion without prompting")]
        yes: bool,
    },
    /// List one page of tasks, newest activity first.
    List {
        #[arg(help = "Space to list tasks from")]
        space: String,
        #[command(flatten)]
        page: Page,
        #[command(flatten)]
        filters: TaskFilters,
    },
    /// Plan a task for an agent to claim later.
    Plan {
        #[arg(help = "Space to plan the task in")]
        space: String,
        #[arg(long, help = "One sentence naming the work to plan")]
        title: String,
    },
    /// Read one page of result versions with their writers and times, newest first.
    Results {
        #[arg(help = "Space containing the task")]
        space: String,
        #[arg(help = "Task key, such as TK-1, or task ID")]
        task: String,
        #[command(flatten)]
        page: Page,
    },
    /// Read one page of checkpoints with their writing agents in time order.
    Scratchpad {
        #[arg(help = "Space containing the task")]
        space: String,
        #[arg(help = "Task key, such as TK-1, or task ID")]
        task: String,
        #[command(flatten)]
        page: Page,
    },
    /// Show task fields, result version, state attribution and files with their agents.
    Show {
        #[arg(help = "Space containing the task")]
        space: String,
        #[arg(help = "Task key, such as TK-1, or task ID")]
        task: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum TaskState {
    Canceled,
    Done,
    Failed,
    Planned,
    Waiting,
    Working,
}

#[derive(Clone, Debug, Parser, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[command(name = "lorebay", version, about = "The Lorebay command-line client")]
pub(crate) struct Cli {
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = HUMAN_OUTPUT,
        help = "Output format"
    )]
    pub output: OutputMode,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Args, Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ContentOptions {
    #[arg(long, conflicts_with = "content_file", help = "UTF-8 content text")]
    pub content: Option<String>,
    #[arg(
        long,
        value_name = "PATH|-",
        help = "Read UTF-8 content from a file, or '-' for standard input"
    )]
    pub content_file: Option<PathBuf>,
}

#[derive(Args, Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[group(multiple = true)]
pub(crate) struct KnowledgeChanges {
    #[arg(long, help = "New entry name")]
    pub name: Option<String>,
    #[arg(long, help = "New summary of the entry")]
    pub summary: Option<String>,
    #[command(flatten)]
    pub content: ContentOptions,
}

#[derive(Args, Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Page {
    #[arg(long, help = "Maximum items per page")]
    pub page_size: Option<u32>,
    #[arg(long, help = "Pagination cursor from a previous page")]
    pub cursor: Option<String>,
}

#[derive(Args, Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskFilters {
    #[arg(long, help = "Filter by assigned person username")]
    pub person: Option<String>,
    #[arg(long, help = "Filter by agent name")]
    pub agent: Option<String>,
    #[arg(long, value_enum, help = "Filter by task state")]
    pub state: Option<TaskState>,
    #[arg(long, help = "Show only your party's working and waiting tasks")]
    pub own_open: bool,
    #[arg(long, help = "Show only claimable planned tasks")]
    pub claimable: bool,
    #[arg(long, help = "Search tasks by keyword in title or result")]
    pub keyword: Option<String>,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Canceled => "canceled",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Planned => "planned",
            Self::Waiting => "waiting",
            Self::Working => "working",
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
