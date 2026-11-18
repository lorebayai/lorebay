use std::net::TcpListener as BoundListener;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Result;
use lorebayd::{Config, run};
use tokio::net::TcpListener;
use tokio::runtime::Builder;
use tokio::sync::oneshot;
use tokio::time::{Instant, sleep};

use super::POLL_INTERVAL;

pub struct ServerTask {
    abort: Option<oneshot::Sender<()>>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<Result<()>>>,
}

impl ServerTask {
    pub fn start(config: Config, listener: BoundListener) -> Self {
        listener.set_nonblocking(true).unwrap();
        let (shutdown, stopped) = oneshot::channel();
        let (abort, aborted) = oneshot::channel();
        let thread = thread::spawn(move || serve(config, listener, stopped, aborted));
        Self {
            abort: Some(abort),
            shutdown: Some(shutdown),
            thread: Some(thread),
        }
    }

    pub fn assert_running(&mut self) {
        if self.thread.as_ref().unwrap().is_finished() {
            let result = self.join();
            panic!("server exited before the expected response: {result:?}");
        }
    }

    pub fn begin_shutdown(&mut self) {
        self.assert_running();
        self.shutdown.take().unwrap().send(()).unwrap();
    }

    pub async fn finish_shutdown(&mut self, budget: Duration) {
        let deadline = Instant::now() + budget;
        while !self.thread.as_ref().unwrap().is_finished() {
            if Instant::now() >= deadline {
                self.fail("server did not shut down before the deadline");
            }
            sleep(POLL_INTERVAL).await;
        }
        self.join().unwrap();
    }

    pub fn fail(&mut self, message: &str) -> ! {
        self.assert_running();
        panic!("{message}");
    }

    fn join(&mut self) -> Result<()> {
        self.thread.take().unwrap().join().unwrap()
    }
}

impl Drop for ServerTask {
    fn drop(&mut self) {
        if let Some(abort) = self.abort.take() {
            let _ = abort.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    config: Config,
    listener: BoundListener,
    stopped: oneshot::Receiver<()>,
    aborted: oneshot::Receiver<()>,
) -> Result<()> {
    let runtime = Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async {
        let listener = TcpListener::from_std(listener)?;
        tokio::select! {
            biased;
            _ = aborted => Ok(()),
            result = run(config, listener, async { let _ = stopped.await; }) => result,
        }
    })
}
