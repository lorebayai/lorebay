use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs;
use std::path::Path;
use std::process::Stdio;

use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use super::STARTUP_TIMEOUT;

pub async fn write_users(path: &Path, users: &[(&str, &str)]) {
    let mut hashes = HashMap::new();
    let mut entries = Vec::new();
    for &(username, password) in users {
        if let Entry::Vacant(entry) = hashes.entry(password) {
            entry.insert(password_hash(password).await);
        }
        entries.push(json!({"username": username, "password_hash": hashes[password]}));
    }
    fs::write(path, serde_json::to_vec(&entries).unwrap()).unwrap();
}

async fn password_hash(password: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lorebayd"))
        .arg("hash-password")
        .env_clear()
        .env("TOKIO_WORKER_THREADS", HASH_WORKER_THREADS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(password.as_bytes())
        .await
        .unwrap();
    let output = timeout(STARTUP_TIMEOUT, child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "hash-password failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

const HASH_WORKER_THREADS: &str = "2";
