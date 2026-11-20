use std::collections::HashMap;
use std::fmt;
use std::result::Result;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result as InterfaceResult;
use futures_util::future::BoxFuture;
use http::HeaderMap;
use sea_orm::DatabaseTransaction;
use typed_builder::TypedBuilder;

use crate::{CallerError, FileStorage, SpaceRole};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct Caller {
    pub person_id: String,
    pub agent_name: Option<String>,
    pub owner_id: String,
}

impl Caller {
    pub fn person_id(&self) -> &str {
        &self.person_id
    }

    pub fn person(&self) -> Result<(), CallerError> {
        if self.agent_name.is_some() {
            Err(CallerError::PersonRequired)
        } else {
            Ok(())
        }
    }

    pub fn agent(&self) -> Result<&str, CallerError> {
        self.agent_name.as_deref().ok_or(CallerError::AgentRequired)
    }
}

pub trait CallerResolver: Send + Sync {
    fn resolve<'a>(
        &'a self,
        headers: &'a HeaderMap,
    ) -> BoxFuture<'a, InterfaceResult<Option<Caller>>>;
}

pub trait DatabaseTransactions: Send + Sync {
    fn begin_read(&self) -> BoxFuture<'_, InterfaceResult<DatabaseTransaction>>;

    fn end_read(&self, transaction: DatabaseTransaction) -> BoxFuture<'_, InterfaceResult<()>>;

    fn begin_write(&self) -> BoxFuture<'_, InterfaceResult<DatabaseTransaction>>;

    fn commit_write<'a>(
        &'a self,
        transaction: DatabaseTransaction,
        record: Option<WriteRecord<'a>>,
    ) -> BoxFuture<'a, InterfaceResult<()>>;

    fn abandon_write(&self, transaction: DatabaseTransaction)
    -> BoxFuture<'_, InterfaceResult<()>>;
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[non_exhaustive]
pub struct LibrarySettings {
    pub public_url: String,
    pub mcp_path: String,
    pub server_name: String,
    pub server_version: String,
    pub authorization_server: String,
    pub upload_size_limit: u64,
    pub upload_url_lifetime: Duration,
    pub download_url_lifetime: Duration,
    pub inline_content_types: Vec<String>,
    pub abandoned_artifact_lifetime: Duration,
    pub orphaned_file_lifetime: Duration,
}

#[derive(Clone)]
pub struct LibraryState {
    settings: Arc<LibrarySettings>,
    caller: Arc<dyn CallerResolver>,
    database: Arc<dyn DatabaseTransactions>,
    names: Arc<dyn PersonNames>,
    roles: Arc<dyn SpaceRoles>,
    pub(crate) storage: Arc<dyn FileStorage>,
}

impl LibraryState {
    pub fn new(settings: LibrarySettings, interfaces: LibraryInterfaces) -> InterfaceResult<Self> {
        anyhow::ensure!(
            settings.abandoned_artifact_lifetime > settings.upload_url_lifetime
                && settings.orphaned_file_lifetime > settings.upload_url_lifetime,
            "Artifact cleanup ages must exceed the upload URL lifetime"
        );
        Ok(Self {
            settings: Arc::new(settings),
            caller: interfaces.caller,
            database: interfaces.database,
            names: interfaces.names,
            roles: interfaces.roles,
            storage: interfaces.storage,
        })
    }

    pub fn settings(&self) -> &LibrarySettings {
        &self.settings
    }

    pub async fn resolve_caller(&self, headers: &HeaderMap) -> InterfaceResult<Option<Caller>> {
        self.caller.resolve(headers).await
    }

    pub async fn run_write<T, E: From<anyhow::Error>>(
        &self,
        write: impl AsyncFnOnce(&mut Write<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        let transaction = self.database.begin_write().await?;
        let mut context = Write {
            state: self,
            transaction: &transaction,
            progress: WriteProgress::Empty,
            caller: None,
            delete: Vec::new(),
        };
        let result = write(&mut context).await;
        let Write {
            progress,
            caller,
            delete,
            ..
        } = context;
        let result = result.and_then(|value| match progress {
            WriteProgress::Recorded {
                operation,
                space_id,
            } => Ok((
                value,
                Some(WriteRecord {
                    operation,
                    caller: caller.as_ref(),
                    space_id,
                }),
            )),
            WriteProgress::Unchanged => Ok((value, None)),
            _ => Err(anyhow::anyhow!("A write must complete one library operation").into()),
        });
        let (value, record) = match result {
            Ok(result) => result,
            Err(error) => {
                if let Err(cause) = self.database.abandon_write(transaction).await {
                    tracing::error!(%cause, "could not abandon write");
                }
                return Err(error);
            }
        };
        self.database.commit_write(transaction, record).await?;
        for location in delete {
            if let Err(cause) = self.storage.delete(&location).await {
                tracing::warn!(%cause, %location, "could not delete stored file");
            }
        }
        Ok(value)
    }

    pub(crate) async fn run_read<T, E: From<anyhow::Error>>(
        &self,
        read: impl AsyncFnOnce(OperationHandle<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        let transaction = self.database.begin_read().await?;
        let result = read(OperationHandle::Read(&transaction)).await;
        match self.database.end_read(transaction).await {
            Err(cause) if result.is_ok() => Err(cause.into()),
            Err(cause) => {
                tracing::error!(%cause, "could not end read");
                result
            }
            Ok(()) => result,
        }
    }

    pub(crate) fn names(&self) -> &dyn PersonNames {
        self.names.as_ref()
    }

    pub(crate) fn roles(&self) -> &dyn SpaceRoles {
        self.roles.as_ref()
    }
}

impl fmt::Debug for LibraryState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LibraryState")
            .finish_non_exhaustive()
    }
}

#[derive(TypedBuilder)]
#[non_exhaustive]
pub struct LibraryInterfaces {
    pub caller: Arc<dyn CallerResolver>,
    pub database: Arc<dyn DatabaseTransactions>,
    pub names: Arc<dyn PersonNames>,
    pub roles: Arc<dyn SpaceRoles>,
    pub storage: Arc<dyn FileStorage>,
}

#[derive(Clone, Copy, Debug)]
pub enum OperationHandle<'a> {
    Read(&'a DatabaseTransaction),
    Write(&'a DatabaseTransaction),
}

impl<'a> OperationHandle<'a> {
    pub fn transaction(self) -> &'a DatabaseTransaction {
        match self {
            Self::Read(transaction) | Self::Write(transaction) => transaction,
        }
    }
}

pub trait PersonNames: Send + Sync {
    fn names<'a>(
        &'a self,
        handle: OperationHandle<'a>,
        people: &'a [String],
    ) -> BoxFuture<'a, InterfaceResult<HashMap<String, String>>>;

    fn person_id<'a>(
        &'a self,
        handle: OperationHandle<'a>,
        name: &'a str,
    ) -> BoxFuture<'a, InterfaceResult<Option<String>>>;
}

pub trait SpaceRoles: Send + Sync {
    fn role<'a>(
        &'a self,
        handle: OperationHandle<'a>,
        person_id: &'a str,
        space_id: &'a str,
    ) -> BoxFuture<'a, InterfaceResult<Option<SpaceRole>>>;

    fn memberships<'a>(
        &'a self,
        handle: OperationHandle<'a>,
        request: MembershipPage<'a>,
    ) -> BoxFuture<'a, InterfaceResult<Vec<Membership>>>;
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct Membership {
    pub space_id: String,
    pub space_number: i64,
    pub role: SpaceRole,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct MembershipPage<'a> {
    pub person_id: &'a str,
    pub owner_id: &'a str,
    pub after_number: i64,
    pub limit: u64,
}

pub struct Write<'a> {
    state: &'a LibraryState,
    transaction: &'a DatabaseTransaction,
    progress: WriteProgress,
    caller: Option<Caller>,
    pub(crate) delete: Vec<String>,
}

impl Write<'_> {
    pub fn transaction(&self) -> &DatabaseTransaction {
        self.transaction
    }

    pub(crate) fn begin_operation(&mut self, caller: Option<&Caller>) -> InterfaceResult<()> {
        anyhow::ensure!(
            matches!(self.progress, WriteProgress::Empty),
            "A write can run only one library operation"
        );
        self.progress = WriteProgress::Pending;
        self.caller = caller.cloned();
        Ok(())
    }

    pub(crate) fn record(&mut self, operation: WriteOperation, space_id: Option<String>) {
        self.progress = WriteProgress::Recorded {
            operation,
            space_id,
        };
    }

    pub(crate) fn handle(&self) -> OperationHandle<'_> {
        OperationHandle::Write(self.transaction)
    }

    pub(crate) fn finish_without_record(&mut self) {
        self.progress = WriteProgress::Unchanged;
    }

    pub(crate) fn state(&self) -> &LibraryState {
        self.state
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum WriteOperation {
    AddNote,
    ApproveProposal,
    ChangeKnowledgeEntry,
    ChangeSpace,
    ChangeTask,
    CompleteArtifact,
    CreateKnowledgeEntry,
    CreateSpace,
    CreateTask,
    DeleteArtifact,
    DeleteKnowledgeEntry,
    DeleteNote,
    DeleteTask,
    RefuseArtifact,
    RegisterArtifact,
    RejectProposal,
    RemoveAbandonedArtifacts,
    RollBackKnowledgeEntry,
    SubmitProposal,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct WriteRecord<'a> {
    pub operation: WriteOperation,
    pub caller: Option<&'a Caller>,
    pub space_id: Option<String>,
}

enum WriteProgress {
    Empty,
    Pending,
    Recorded {
        operation: WriteOperation,
        space_id: Option<String>,
    },
    Unchanged,
}
