use super::{Server, assert_usage, knowledge_json};
use serde_json::{Value, from_slice};

pub async fn require_seen_versions(server: &Server, space: &str) {
    for args in [
        vec!["request", "approve", space, "RQ-1"],
        vec!["request", "reject", space, "RQ-1", "--reason", "Read first"],
        vec!["knowledge", "change", space, "KN-1", "--content", "Changed"],
        vec!["knowledge", "delete", space, "KN-1", "--yes"],
        vec!["knowledge", "rollback", space, "KN-1", "1"],
    ] {
        let mut args = args;
        args.extend(["--output", "json"]);
        let output = server.run(&args).await;
        assert_usage(&output);
        let flag = if args[0] == "request" {
            "--revision"
        } else {
            "--version"
        };
        assert!(String::from_utf8(output.stderr).unwrap().contains(flag));
    }
}

pub async fn refuse_stale_knowledge(server: &Server, space: &str) {
    let changed = knowledge_json(
        server,
        &[
            "change",
            space,
            "KN-1",
            "--version",
            "1",
            "--content",
            "Current",
        ],
    )
    .await;
    assert_eq!(changed["version"], 2);
    for args in [
        vec![
            "knowledge",
            "change",
            space,
            "KN-1",
            "--version",
            "1",
            "--content",
            "Lost",
        ],
        vec![
            "knowledge",
            "delete",
            space,
            "KN-1",
            "--version",
            "1",
            "--yes",
        ],
        vec![
            "knowledge",
            "rollback",
            space,
            "KN-1",
            "1",
            "--version",
            "1",
        ],
    ] {
        let mut args = args;
        args.extend(["--output", "json"]);
        let output = server.run(&args).await;
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            from_slice::<Value>(&output.stderr).unwrap()["code"],
            "knowledge.version_changed"
        );
    }
}
