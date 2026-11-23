use lorebay_lib_core::{TaskRead, read_task};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, resume_unwind};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use futures_util::future::BoxFuture;
use futures_util::stream::{self, BoxStream};
use futures_util::{FutureExt, StreamExt};
use http::HeaderMap;
use lorebay_lib_core::{
    AgentFileCleanupError, AgentFileCompleted, AgentFileError, AgentFileRegistered, Caller,
    CallerResolver, CreatedTask, DatabaseTransactions, DownloadRequest, DownloadSpec, FileStorage,
    LibraryInterfaces, LibrarySettings, LibraryState, Membership, MembershipPage, NewAgentFile,
    NewSpace, NoteAdded, OperationHandle, PageOptions, PersonNames, SpaceRole, SpaceRoles,
    StoredFile, TaskFilters, TaskTarget, UploadRequest, UploadSpec, WriteOperation, WriteRecord,
    add_note, complete_agent_file, create_space, create_task, delete_agent_file,
    prepare_agent_file_completion, register_agent_file, remove_abandoned_agent_files,
};
use lorebay_lib_migration::Migrator;
use lorebay_lib_types::Secret;
use sea_orm::sea_query::{Alias, Expr, ExprTrait, Func, Query};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DatabaseTransaction, DbBackend,
    IsolationLevel, Statement, TransactionOptions, TransactionTrait,
};
use sea_orm_migration::MigratorTrait;
use tokio::sync::{Notify, mpsc};
use tokio::task::{JoinHandle, yield_now};
use tokio::time::timeout;
use typed_builder::TypedBuilder;
use uuid::Uuid;

pub const TIMEOUT: Duration = Duration::from_secs(20);
pub const SPACE: &str = "SPACE-1";

const CONNECTION: &str = "postgres://lorebay_test:lorebay_test@127.0.0.1:55432";
const START_POSTGRES: &str = "docker run --rm -d --name lorebay-test-postgres -e POSTGRES_USER=lorebay_test -e POSTGRES_PASSWORD=lorebay_test -e POSTGRES_DB=postgres -p 127.0.0.1:55432:5432 postgres:18.7@sha256:2ea8630df176ad090ec92136dee850397c5af8781e4bcf2e4162593691c61bbc";

pub struct Fixture {
    pub database: DatabaseConnection,
    pub state: LibraryState,
    pub storage: Arc<Files>,
    pub roles: Arc<Identity>,
    pub agent: Caller,
    pub person: Caller,
}

impl Fixture {
    pub async fn space(&self) {
        self.state
            .run_write(async |write| {
                create_space(
                    write,
                    &self.person,
                    1,
                    SpaceRole::Owner,
                    NewSpace::new("Team".to_owned(), "Shared work".to_owned()).unwrap(),
                )
                .await
            })
            .await
            .unwrap();
    }

    pub async fn task(&self, title: &str) -> CreatedTask {
        new_task(&self.state, &self.agent, title).await
    }

    pub async fn read(&self, task: &CreatedTask) -> TaskRead {
        read_task(&self.state, &self.person, SPACE, &task.key, Some(page()))
            .await
            .unwrap()
    }

    pub fn state_with(&self, gates: Gates, short_abandonment: bool) -> LibraryState {
        state(
            self.database.clone(),
            self.storage.clone(),
            self.roles.clone(),
            gates,
            short_abandonment,
        )
    }

    pub async fn wait_for_lock(&self, pid: i32) {
        timeout(TIMEOUT, async {
            loop {
                let row = self
                    .database
                    .query_one(
                        Query::select()
                            .expr(Expr::value(1))
                            .from(Alias::new("pg_locks"))
                            .and_where(Expr::col(Alias::new("pid")).eq(pid))
                            .and_where(Expr::col(Alias::new("granted")).eq(false)),
                    )
                    .await
                    .unwrap();
                if row.is_some() {
                    break;
                }
                yield_now().await;
            }
        })
        .await
        .expect("expected the second transaction to wait for a PostgreSQL lock");
    }
}

pub async fn with_postgres(run: impl AsyncFnOnce(&Fixture)) {
    let admin = connect("postgres", 1).await.unwrap_or_else(|error| {
        panic!(
            "Postgres is required at 127.0.0.1:55432. Start it with: {START_POSTGRES}\n{error:#}"
        )
    });
    let name = format!("lorebay_test_{}", Uuid::now_v7().simple());
    database_command(&admin, format!("CREATE DATABASE {name}"))
        .await
        .unwrap();
    let test = AssertUnwindSafe(async {
        let database = connect(&name, 6).await.unwrap();
        let result = AssertUnwindSafe(async {
            Migrator::up(&database, None).await.unwrap();
            run(&fixture(database.clone())).await;
        })
        .catch_unwind()
        .await;
        let closed = database.close().await;
        if let Err(cause) = result {
            resume_unwind(cause);
        }
        closed.unwrap();
    })
    .catch_unwind()
    .await;

    let dropped = database_command(&admin, format!("DROP DATABASE {name} WITH (FORCE)")).await;
    let closed = admin.close().await;
    if let Err(cause) = test {
        if let Err(error) = dropped {
            eprintln!("could not drop test database {name}: {error}");
        }
        resume_unwind(cause);
    }
    dropped.unwrap();
    closed.unwrap();
}

#[derive(Default)]
pub struct Gates {
    pub before_begin: Option<Arc<Gate>>,
    pub before_commit: Option<Arc<Gate>>,
    pub report_pid: Option<mpsc::UnboundedSender<i32>>,
    pub records: Option<Records>,
}

#[derive(Default)]
pub struct Pause(Arc<Gate>);

impl Pause {
    pub fn gate(&self) -> Arc<Gate> {
        Arc::clone(&self.0)
    }

    pub async fn reached(&self) {
        timeout(TIMEOUT, self.0.reached.notified())
            .await
            .expect("transaction did not reach the gate");
    }

    pub fn release(&self) {
        self.0.release.notify_one();
    }
}

impl Drop for Pause {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Default)]
pub struct Gate {
    reached: Notify,
    release: Notify,
}

impl Gate {
    async fn hold(&self) -> Result<()> {
        self.reached.notify_one();
        timeout(TIMEOUT, self.release.notified())
            .await
            .context("transaction gate timed out")
    }
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct RecordedWrite {
    pub operation: WriteOperation,
    pub caller: Option<Caller>,
    pub space_id: Option<String>,
}

pub type Records = Arc<Mutex<Vec<Option<RecordedWrite>>>>;

#[derive(Default)]
pub struct Files {
    files: Mutex<HashMap<String, Vec<u8>>>,
    delete_gate: Mutex<Option<Arc<Gate>>>,
    fail_delete: AtomicBool,
    old_files: AtomicBool,
    deletes: AtomicUsize,
}

impl Files {
    pub fn pause_delete(&self, gate: Arc<Gate>) {
        *self.delete_gate.lock().unwrap() = Some(gate);
    }

    pub fn fail_next_delete(&self) {
        self.fail_delete.store(true, Ordering::Relaxed);
    }

    pub fn age(&self) {
        self.old_files.store(true, Ordering::Relaxed);
    }

    pub fn deletes(&self) -> usize {
        self.deletes.load(Ordering::Relaxed)
    }

    pub fn upload(&self, registration: &AgentFileRegistered, bytes: &[u8]) -> String {
        let location = registration.upload.url.expose().clone();
        let old = self
            .files
            .lock()
            .unwrap()
            .insert(location.clone(), bytes.to_vec());
        assert!(old.is_none());
        location
    }

    pub fn contains(&self, location: &str, bytes: &[u8]) -> bool {
        self.files
            .lock()
            .unwrap()
            .get(location)
            .is_some_and(|stored| stored == bytes)
    }
}

impl FileStorage for Files {
    fn upload_url<'a>(&'a self, request: UploadSpec<'a>) -> BoxFuture<'a, Result<UploadRequest>> {
        Box::pin(async move {
            Ok(UploadRequest {
                method: "PUT".to_owned(),
                url: Secret::from(request.location.to_owned()),
                headers: [("If-None-Match".to_owned(), "*".to_owned())].into(),
                expires_at: 0,
            })
        })
    }

    fn download_url<'a>(
        &'a self,
        request: DownloadSpec<'a>,
    ) -> BoxFuture<'a, Result<DownloadRequest>> {
        Box::pin(async move {
            Ok(DownloadRequest {
                url: Secret::from(request.location.to_owned()),
                expires_at: 0,
            })
        })
    }

    fn size<'a>(&'a self, location: &'a str) -> BoxFuture<'a, Result<Option<u64>>> {
        Box::pin(async move {
            Ok(self
                .files
                .lock()
                .unwrap()
                .get(location)
                .map(|bytes| bytes.len() as u64))
        })
    }

    fn delete<'a>(&'a self, location: &'a str) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.deletes.fetch_add(1, Ordering::Relaxed);
            let gate = self.delete_gate.lock().unwrap().take();
            if let Some(gate) = gate {
                gate.hold().await?;
            }
            anyhow::ensure!(
                !self.fail_delete.swap(false, Ordering::Relaxed),
                "storage deletion failed"
            );
            self.files.lock().unwrap().remove(location);
            Ok(())
        })
    }

    fn list(&self) -> BoxStream<'_, Result<StoredFile>> {
        let files = self
            .files
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .map(|location| {
                Ok(StoredFile {
                    location,
                    modified: if self.old_files.load(Ordering::Relaxed) {
                        SystemTime::now() - Duration::from_secs(3600)
                    } else {
                        SystemTime::now()
                    },
                })
            })
            .collect::<Vec<_>>();
        stream::iter(files).boxed()
    }
}

#[derive(Default)]
pub struct Identity(pub AtomicUsize);

impl CallerResolver for Identity {
    fn resolve<'a>(&'a self, _headers: &'a HeaderMap) -> BoxFuture<'a, Result<Option<Caller>>> {
        Box::pin(async { Ok(None) })
    }
}

impl PersonNames for Identity {
    fn names<'a>(
        &'a self,
        _handle: OperationHandle<'a>,
        ids: &'a [String],
    ) -> BoxFuture<'a, Result<HashMap<String, String>>> {
        Box::pin(async { Ok(ids.iter().map(|id| (id.clone(), id.clone())).collect()) })
    }

    fn person_id<'a>(
        &'a self,
        _handle: OperationHandle<'a>,
        name: &'a str,
    ) -> BoxFuture<'a, Result<Option<String>>> {
        Box::pin(async { Ok(Some(name.to_owned())) })
    }
}

impl SpaceRoles for Identity {
    fn role<'a>(
        &'a self,
        _handle: OperationHandle<'a>,
        _person: &'a str,
        _space: &'a str,
    ) -> BoxFuture<'a, Result<Option<SpaceRole>>> {
        Box::pin(async {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(Some(SpaceRole::Owner))
        })
    }

    fn memberships<'a>(
        &'a self,
        _handle: OperationHandle<'a>,
        _request: MembershipPage<'a>,
    ) -> BoxFuture<'a, Result<Vec<Membership>>> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

struct Transactions {
    database: DatabaseConnection,
    gates: Gates,
}

impl DatabaseTransactions for Transactions {
    fn begin_read(&self) -> BoxFuture<'_, Result<DatabaseTransaction>> {
        Box::pin(async {
            Ok(self
                .database
                .begin_with_options(TransactionOptions {
                    isolation_level: Some(IsolationLevel::RepeatableRead),
                    ..Default::default()
                })
                .await?)
        })
    }

    fn end_read(&self, transaction: DatabaseTransaction) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move { Ok(transaction.commit().await?) })
    }

    fn begin_write(&self) -> BoxFuture<'_, Result<DatabaseTransaction>> {
        Box::pin(async {
            if let Some(gate) = &self.gates.before_begin {
                gate.hold().await?;
            }
            let transaction = self.database.begin().await?;
            if let Some(report) = &self.gates.report_pid {
                let row = transaction
                    .query_one(Query::select().expr(Func::cust(Alias::new("pg_backend_pid"))))
                    .await?
                    .context("no PostgreSQL backend PID")?;
                report.send(row.try_get_by_index(0)?)?;
            }
            Ok(transaction)
        })
    }

    fn commit_write<'a>(
        &'a self,
        transaction: DatabaseTransaction,
        record: Option<WriteRecord<'a>>,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            if let Some(gate) = &self.gates.before_commit {
                gate.hold().await?;
            }
            transaction.commit().await?;
            if let Some(records) = &self.gates.records {
                records.lock().unwrap().push(record.map(|record| {
                    RecordedWrite::builder()
                        .operation(record.operation)
                        .caller(record.caller.cloned())
                        .space_id(record.space_id)
                        .build()
                }));
            }
            Ok(())
        })
    }

    fn abandon_write(&self, transaction: DatabaseTransaction) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move { Ok(transaction.rollback().await?) })
    }
}

pub async fn new_task(state: &LibraryState, caller: &Caller, title: &str) -> CreatedTask {
    let title = title.to_owned().try_into().unwrap();
    state
        .run_write(async |write| create_task(write, caller, SPACE, title).await)
        .await
        .unwrap()
}

pub async fn register(
    state: &LibraryState,
    caller: &Caller,
    task: &CreatedTask,
) -> AgentFileRegistered {
    state
        .run_write(async |write| {
            register_agent_file(
                write,
                caller,
                target(task),
                NewAgentFile {
                    file_name: "report.txt".to_owned().try_into().unwrap(),
                    summary: "Result".to_owned().try_into().unwrap(),
                },
            )
            .await
        })
        .await
        .unwrap()
        .issue_upload(state)
        .await
        .unwrap()
}

pub async fn complete(
    state: &LibraryState,
    caller: &Caller,
    target: TaskTarget<'_>,
    key: &str,
) -> Result<AgentFileCompleted, AgentFileError> {
    let prepared = prepare_agent_file_completion(state, caller, target, key).await?;
    state
        .run_write(async |write| complete_agent_file(write, caller, target, prepared).await)
        .await?
        .into_result()
}

pub fn spawn_completion(
    state: LibraryState,
    caller: &Caller,
    task: &CreatedTask,
    key: &str,
) -> JoinHandle<Result<AgentFileCompleted, AgentFileError>> {
    let caller = caller.clone();
    let task = task.clone();
    let key = key.to_owned();
    tokio::spawn(async move { complete(&state, &caller, target(&task), &key).await })
}

pub fn agent_file_input() -> NewAgentFile {
    NewAgentFile {
        file_name: "report.txt".to_owned().try_into().unwrap(),
        summary: "Result".to_owned().try_into().unwrap(),
    }
}

pub async fn delete(
    state: &LibraryState,
    caller: &Caller,
    target: TaskTarget<'_>,
    key: &str,
) -> Result<(), AgentFileError> {
    state
        .run_write(async |write| delete_agent_file(write, caller, target, key).await)
        .await
}

pub async fn cleanup(state: &LibraryState) -> Result<u64, AgentFileCleanupError> {
    state
        .run_write(async |write| remove_abandoned_agent_files(write).await)
        .await
}

pub fn target(task: &CreatedTask) -> TaskTarget<'_> {
    TaskTarget {
        path_space: None,
        task_id: task.task_id.expose(),
    }
}

pub fn page() -> PageOptions {
    PageOptions {
        page_size: 100,
        cursor: None,
    }
}

pub async fn next_pid(receiver: &mut mpsc::UnboundedReceiver<i32>) -> i32 {
    timeout(TIMEOUT, receiver.recv())
        .await
        .expect("backend PID timed out")
        .expect("backend PID unavailable")
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}

pub async fn append(
    state: &LibraryState,
    caller: &Caller,
    task: &CreatedTask,
    content: &str,
) -> NoteAdded {
    let content = content.to_owned().try_into().unwrap();
    state
        .run_write(async |write| add_note(write, caller, target(task), content).await)
        .await
        .unwrap()
}

pub fn filters(keyword: Option<&str>) -> TaskFilters {
    TaskFilters {
        person: None,
        agent: None,
        status: None,
        own_open: false,
        keyword: keyword.map(|value| value.to_owned().try_into().unwrap()),
    }
}

pub async fn wait_until_after(then: i64) {
    timeout(TIMEOUT, async {
        while now_millis() <= then {
            yield_now().await;
        }
    })
    .await
    .expect("clock did not advance before the timeout");
}

async fn connect(database: &str, connections: u32) -> Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(format!("{CONNECTION}/{database}"));
    options
        .max_connections(connections)
        .connect_timeout(Duration::from_secs(3));
    Ok(Database::connect(options).await?)
}

#[expect(
    clippy::disallowed_methods,
    reason = "sea-query has no database lifecycle statements"
)]
async fn database_command(connection: &DatabaseConnection, sql: String) -> Result<()> {
    connection
        .execute_raw(Statement::from_string(DbBackend::Postgres, sql))
        .await?;
    Ok(())
}

fn fixture(database: DatabaseConnection) -> Fixture {
    let storage = Arc::new(Files::default());
    let roles = Arc::new(Identity::default());
    let person = Caller {
        person_id: "alice".to_owned(),
        agent_name: None,
        owner_id: "team".to_owned(),
    };
    let agent = Caller {
        agent_name: Some("research".to_owned()),
        ..person.clone()
    };
    let state = state(
        database.clone(),
        storage.clone(),
        roles.clone(),
        Gates::default(),
        false,
    );
    Fixture {
        database,
        state,
        storage,
        roles,
        agent,
        person,
    }
}

fn state(
    database: DatabaseConnection,
    storage: Arc<Files>,
    roles: Arc<Identity>,
    gates: Gates,
    short_abandonment: bool,
) -> LibraryState {
    let lifetime = if short_abandonment {
        Duration::from_millis(1)
    } else {
        Duration::from_secs(30)
    };
    LibraryState::new(
        LibrarySettings::builder()
            .public_url("http://localhost".to_owned())
            .mcp_path("/mcp".to_owned())
            .server_name("test".to_owned())
            .server_version("0".to_owned())
            .authorization_server("http://localhost".to_owned())
            .upload_size_limit(100_u64)
            .upload_url_lifetime(lifetime)
            .download_url_lifetime(lifetime)
            .inline_content_types(Vec::new())
            .abandoned_agent_file_lifetime(lifetime * 2)
            .orphaned_file_lifetime(lifetime * 2)
            .build(),
        LibraryInterfaces::builder()
            .caller(roles.clone())
            .database(Arc::new(Transactions { database, gates }))
            .names(roles.clone())
            .roles(roles)
            .storage(storage)
            .build(),
    )
    .unwrap()
}
