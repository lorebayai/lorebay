use serde_json::{Value, from_slice};

use crate::helpers::{
    assert_success, body, create_person_request, knowledge_context, refuse_stale_knowledge,
    require_seen_versions,
};

#[tokio::test]
async fn request_revise_keeps_key_and_cli_requires_seen_revision_and_version() {
    let (server, space) = knowledge_context().await;
    let first = create_person_request(&server, &space, "Guide", "Summary", "Original").await;
    assert_eq!(first["revision"], 1);
    let output = server
        .run(&[
            "request",
            "revise",
            &space,
            "RQ-1",
            "--content",
            "Reconciled",
            "--output",
            "json",
        ])
        .await;
    assert_success(&output);
    assert_eq!(body(&output)["data"]["revision"], 2);
    assert_eq!(body(&output)["data"]["key"], first["key"]);
    let shown = server.run(&["request", "show", &space, "RQ-1"]).await;
    assert_success(&shown);
    let text = String::from_utf8(shown.stdout).unwrap();
    assert!(text.contains("Revision: 2\n"));
    assert!(text.contains("Outdated: false\n"));
    require_seen_versions(&server, &space).await;
    let stale = server
        .run(&[
            "request",
            "approve",
            &space,
            "RQ-1",
            "--revision",
            "1",
            "--output",
            "json",
        ])
        .await;
    assert_eq!(stale.status.code(), Some(1));
    assert_eq!(
        from_slice::<Value>(&stale.stderr).unwrap()["code"],
        "person_request.revision_changed"
    );
    let approved = server
        .run(&["request", "approve", &space, "RQ-1", "--revision", "2"])
        .await;
    assert_success(&approved);
    let shown = server.run(&["knowledge", "show", &space, "KN-1"]).await;
    assert_success(&shown);
    assert!(
        String::from_utf8(shown.stdout)
            .unwrap()
            .contains("Version: 1\n")
    );
    refuse_stale_knowledge(&server, &space).await;
}
