use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use lorebayd::{Config, run};
use reqwest::{Client, Response, StatusCode};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

pub const TEST_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub struct Server {
    pub client: Client,
    pub address: SocketAddr,
    pub directory: TempDir,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<()>>>,
}

impl Server {
    pub async fn start() -> Self {
        Self::restart(TempDir::new().unwrap()).await
    }

    pub async fn restart(directory: TempDir) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let config = Config::builder()
            .listen_address(address)
            .data_directory(directory.path().join("data"))
            .build();
        let (shutdown, stopped) = oneshot::channel();
        let task = tokio::spawn(run(config, listener, async {
            let _ = stopped.await;
        }));
        let server = Self {
            client: Client::builder()
                .timeout(TEST_TIMEOUT)
                .no_proxy()
                .build()
                .unwrap(),
            address,
            directory,
            shutdown: Some(shutdown),
            task: Some(task),
        };
        server.wait_for_status("/readyz", StatusCode::OK).await;
        server
    }

    pub async fn get(&self, path: &str) -> Response {
        self.client
            .get(format!("http://{}{path}", self.address))
            .send()
            .await
            .unwrap()
    }

    pub fn begin_shutdown(&mut self) {
        self.shutdown.take().unwrap().send(()).unwrap();
    }

    pub async fn wait_for_status(&self, path: &str, expected: StatusCode) {
        timeout(TEST_TIMEOUT, async {
            loop {
                if let Ok(response) = self
                    .client
                    .get(format!("http://{}{path}", self.address))
                    .send()
                    .await
                    && response.status() == expected
                {
                    break;
                }
                assert!(
                    !self.task.as_ref().unwrap().is_finished(),
                    "server exited before {expected}"
                );
                sleep(POLL_INTERVAL).await;
            }
        })
        .await
        .unwrap();
    }

    pub async fn finish_shutdown(&mut self) {
        timeout(TEST_TIMEOUT, self.task.take().unwrap())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    pub async fn wait_for_listener_to_close(&self) {
        timeout(TEST_TIMEOUT, async {
            while TcpStream::connect(self.address).await.is_ok() {
                sleep(POLL_INTERVAL).await;
            }
        })
        .await
        .unwrap();
        assert!(!self.task.as_ref().unwrap().is_finished());
    }

    pub async fn stop(mut self) -> TempDir {
        self.begin_shutdown();
        self.finish_shutdown().await;
        self.directory
    }

    pub async fn begin_upload(&self) -> TcpStream {
        let mut stream = TcpStream::connect(self.address).await.unwrap();
        stream.write_all(b"POST /api/v1/unknown HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n").await.unwrap();
        let mut interim = [0; 25];
        timeout(TEST_TIMEOUT, stream.read_exact(&mut interim))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&interim, b"HTTP/1.1 100 Continue\r\n\r\n");
        stream
    }
}
