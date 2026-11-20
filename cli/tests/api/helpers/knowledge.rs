use std::fs;
use std::path::Path;
use std::process::{Output, Stdio};

use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::time::timeout;

use super::{SECOND_USERNAME, Server, WAIT, assert_success, body, create};

pub async fn knowledge_context() -> (Server, String) {
    let server = Server::start().await;
    server.login().await;
    let space = create(&server, "Team", "Shared Knowledge").await;
    (server, space)
}

pub async fn create_entry(
    server: &Server,
    space: &str,
    name: &str,
    summary: &str,
    content: &str,
) -> Value {
    let output = server
        .run(&[
            "knowledge",
            "create",
            space,
            name,
            "--summary",
            summary,
            "--content",
            content,
            "--output",
            "json",
        ])
        .await;
    assert_success(&output);
    assert!(output.stderr.is_empty());
    assert_eq!(body(&output)["meta"], json!({}));
    body(&output)["data"].clone()
}

pub async fn show_entry(server: &Server, space: &str, entry: &str) -> Value {
    let output = server
        .run(&["knowledge", "show", space, entry, "--output", "json"])
        .await;
    assert_success(&output);
    assert!(output.stderr.is_empty());
    assert_eq!(body(&output)["meta"], json!({}));
    body(&output)["data"].clone()
}

pub async fn content_input(server: &Server, arguments: &[&str], bytes: &[u8]) -> Output {
    let mut child = server.cli(arguments).stdin(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(bytes).await.unwrap();
    timeout(WAIT, child.wait_with_output())
        .await
        .unwrap()
        .unwrap()
}

pub fn assert_knowledge_error(output: &Output, mode: &str, code: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
    if mode == "json" {
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], code);
        assert_eq!(
            error["status"],
            match code {
                "space.permission_required" => 403,
                "knowledge.name_taken" => 409,
                "request.invalid_body" | "request.invalid_query" => 400,
                _ => 404,
            }
        );
    }
}

pub async fn assert_knowledge_prefix_pages(server: &Server, space: &str) {
    for prefix in ["Deploy/", "deploy"] {
        for mode in ["human", "json"] {
            let first = server
                .run(&[
                    "knowledge",
                    "list",
                    space,
                    "--prefix",
                    prefix,
                    "--page-size",
                    "1",
                    "--output",
                    mode,
                ])
                .await;
            assert_success(&first);
            let hint = String::from_utf8(first.stderr).unwrap();
            assert!(hint.contains(&format!(" --prefix '{prefix}' --page-size 1 --cursor '")));
            let cursor = hint
                .split(" --cursor '")
                .nth(1)
                .unwrap()
                .split('\'')
                .next()
                .unwrap();
            let last = server
                .run(&[
                    "knowledge",
                    "list",
                    space,
                    "--prefix",
                    prefix,
                    "--page-size",
                    "1",
                    "--cursor",
                    cursor,
                    "--output",
                    mode,
                ])
                .await;
            assert_success(&last);
            assert!(last.stderr.is_empty());
            if mode == "json" {
                assert_eq!(
                    body(&last),
                    json!({"data": [{"name": "deploy/nested/"}], "meta": {}})
                );
            } else {
                assert!(
                    String::from_utf8(first.stdout)
                        .unwrap()
                        .contains("Summary\\n\\t\\u{1b}[31m")
                );
                assert_eq!(last.stdout, b"Name\tKey\tSummary\ndeploy/nested/\t\t\n");
            }
        }
    }
}

pub async fn assert_empty_knowledge_search(server: &Server, space: &str) {
    let empty = server
        .run(&[
            "knowledge",
            "list",
            space,
            "--keyword",
            "absent",
            "--output",
            "json",
        ])
        .await;
    assert_success(&empty);
    assert_eq!(body(&empty), json!({"data": [], "meta": {}}));
    assert!(empty.stderr.is_empty());
}

pub async fn assert_viewer_reads_knowledge(server: &Server, space: &str, entry: &Value) {
    assert_success(
        &server
            .run(&[
                "participant",
                "set-role",
                space,
                SECOND_USERNAME,
                "--role",
                "viewer",
            ])
            .await,
    );
    let shown = server
        .run_as(
            SECOND_USERNAME,
            &["knowledge", "show", space, "KN-1", "--output", "json"],
        )
        .await;
    assert_success(&shown);
    assert_eq!(&body(&shown)["data"], entry);
}

pub async fn assert_content_file_limits(server: &Server, space: &str, path: &Path) {
    for bytes in [
        vec![0; 1024 * 1024],
        vec![b'x'; 1024 * 1024 + 1],
        Vec::new(),
    ] {
        fs::write(path, &bytes).unwrap();
        let output = server
            .run(&[
                "knowledge",
                "create",
                space,
                "limit.md",
                "--summary",
                "Limit",
                "--content-file",
                path.to_str().unwrap(),
                "--output",
                "json",
            ])
            .await;
        if bytes.len() == 1024 * 1024 {
            assert_success(&output);
            assert_eq!(
                show_entry(server, space, "KN-2").await["content"]
                    .as_str()
                    .unwrap()
                    .as_bytes(),
                bytes
            );
            let shown = server.run(&["knowledge", "show", space, "KN-2"]).await;
            assert_success(&shown);
            assert!(shown.stderr.is_empty());
            let text = String::from_utf8(shown.stdout).unwrap();
            assert!(text.contains(&format!("Content: {}\n", "\\u{0}".repeat(bytes.len()))));
        } else {
            assert_knowledge_error(&output, "json", "request.invalid_body");
        }
    }
}

pub async fn assert_other_content_sources(server: &Server, space: &str) {
    let created = content_input(
        server,
        &[
            "knowledge",
            "create",
            space,
            "stdin.md",
            "--summary",
            "Input",
            "--content-file",
            "-",
            "--output",
            "json",
        ],
        b"stdin\r\n\tcontent\n",
    )
    .await;
    assert_success(&created);
    assert_eq!(
        show_entry(server, space, "KN-2").await["content"],
        "stdin\r\n\tcontent\n"
    );
    let path = server.directory.path().join("changed.md");
    fs::write(&path, b"File change\r\n").unwrap();
    assert_success(
        &server
            .run(&[
                "knowledge",
                "change",
                space,
                "guide.md",
                "--content-file",
                path.to_str().unwrap(),
            ])
            .await,
    );
    assert_eq!(
        show_entry(server, space, "KN-1").await["content"],
        "File change\r\n"
    );
}
