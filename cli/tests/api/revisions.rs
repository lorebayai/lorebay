use serde_json::{Value, from_slice};

use crate::helpers::{
    SECOND_USERNAME, Server, assert_outdated_approval, assert_success, body, contributor,
    create_entry, create_person_request, knowledge_context, knowledge_json, person_request_json,
    refuse_stale_knowledge, require_seen_versions, show_entry, show_person_request,
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
    assert_eq!(body(&output)["data"]["was_outdated"], false);
    let shown = server.run(&["request", "show", &space, "RQ-1"]).await;
    assert_success(&shown);
    let text = String::from_utf8(shown.stdout).unwrap();
    assert!(text.contains("Revision: 2\n"));
    assert!(text.contains("Outdated: false\n"));
    assert!(!text.contains("Was outdated:"));
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

#[tokio::test]
async fn request_list_shows_outdated_pending_and_empty_decided_marks() {
    let (server, space) = knowledge_context().await;
    create_entry(&server, &space, "Guide", "Original", "Old").await;
    person_request_json(
        &server,
        &["change", &space, "KN-1", "--content", "Proposal"],
    )
    .await;
    knowledge_json(
        &server,
        &["change", &space, "KN-1", "--content", "Person's edit"],
    )
    .await;
    create_person_request(&server, &space, "Available", "Current", "Content").await;
    create_person_request(&server, &space, "Decided", "Withdrawn", "Content").await;
    person_request_json(
        &server,
        &["reject", &space, "RQ-3", "--reason", "Withdrawn"],
    )
    .await;
    let output = server.run(&["request", "list", &space]).await;
    assert_success(&output);
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        text,
        concat!(
            "Key\tAction\tStatus\tOutdated\tTarget\tProposed summary\n",
            "RQ-3\tcreate\trejected\t\tDecided\tWithdrawn\n",
            "RQ-2\tcreate\tpending\tfalse\tAvailable\tCurrent\n",
            "RQ-1\tchange\tpending\ttrue\tKN-1 Guide\t\n",
        )
    );
    let listed = person_request_json(&server, &["list", &space]).await;
    assert!(listed[0].get("outdated").is_none());
    assert_eq!(listed[1]["outdated"], false);
    assert_eq!(listed[2]["outdated"], true);
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row.get("was_outdated").is_none())
    );
}

#[tokio::test]
async fn request_approve_guides_reconciliation_and_revise_reports_prior_state() {
    let (server, space) = knowledge_context().await;
    create_entry(&server, &space, "Guide", "Original", "Old").await;
    person_request_json(
        &server,
        &["change", &space, "KN-1", "--content", "Proposal"],
    )
    .await;
    knowledge_json(
        &server,
        &["change", &space, "KN-1", "--content", "Person's edit"],
    )
    .await;
    let own_guidance = format!(
        "Read the current entry, reconcile your proposal, then run `lorebay request revise '{space}' 'RQ-1'` with the reconciled changes. Then run `lorebay request show '{space}' 'RQ-1'` and approve the revision it returns."
    );
    assert_outdated_approval(&server, &space, None, &own_guidance).await;
    contributor(&server, &space).await;
    let other_guidance = format!(
        "Ask the requester or their agent to revise the request against the current entry. Then run `lorebay request show '{space}' 'RQ-1'` and approve the revision it returns."
    );
    assert_outdated_approval(&server, &space, Some(SECOND_USERNAME), &other_guidance).await;
    assert_revision_output(&server, &space, "true").await;
    assert_revision_output(&server, &space, "false").await;
    let revised = person_request_json(&server, &["revise", &space, "RQ-1"]).await;
    assert_eq!(revised["was_outdated"], false);
    let shown = show_person_request(&server, &space, "RQ-1").await;
    assert!(shown.get("was_outdated").is_none());
    assert_eq!(shown["outdated"], false);
    person_request_json(&server, &["approve", &space, "RQ-1", "--revision", "4"]).await;
    assert_eq!(
        show_entry(&server, &space, "KN-1").await["content"],
        "Person's edit with proposal"
    );
}

async fn assert_revision_output(server: &Server, space: &str, expected: &str) {
    let output = server
        .run(&[
            "request",
            "revise",
            space,
            "RQ-1",
            "--content",
            "Person's edit with proposal",
        ])
        .await;
    assert_success(&output);
    assert!(output.stderr.is_empty());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains(&format!("Was outdated: {expected}\n")));
    assert!(text.contains("Outdated: false\n"));
}
