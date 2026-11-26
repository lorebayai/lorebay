use std::process::Output;
use std::time::{Duration, UNIX_EPOCH};

use httpdate::fmt_http_date;
use serde_json::Value;

use super::{Server, assert_success};

pub fn human_time(value: &Value) -> String {
    let milliseconds = value.as_u64().unwrap();
    let date = fmt_http_date(UNIX_EPOCH + Duration::from_millis(milliseconds));
    date.replace(" GMT", &format!(".{:03} GMT", milliseconds % 1000))
}

pub fn human_files(files: &Value) -> String {
    let mut rows =
        vec!["\n  Key\tName\tSize\tSummary\tAgent name\tCreated at\tCompleted at".to_owned()];
    for file in files.as_array().unwrap() {
        rows.push(format!(
            "  {}\t{}\t{} bytes\t{}\t{}\t{}\t{}",
            file["key"].as_str().unwrap(),
            file["file_name"].as_str().unwrap(),
            file["size"],
            file["summary"].as_str().unwrap(),
            file["agent_name"].as_str().unwrap(),
            human_time(&file["created_at"]),
            human_time(&file["completed_at"]),
        ));
    }
    rows.join("\n")
}

pub fn expected_time(value: &Value, mode: &str) -> String {
    if mode == "human" {
        human_time(value)
    } else {
        value.to_string()
    }
}

pub async fn assert_usage_lines(server: &Server, arguments: &[&str], output: &Output) {
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("\n\nUsage:"), "{text}");
    assert!(!text.contains("\\n"), "{text}");
    let mut json_args = arguments.to_vec();
    json_args.push("--output=json");
    let json_output = server.run(&json_args).await;
    assert_eq!(json_output.status.code(), Some(2));
    assert!(json_output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&json_output.stderr).unwrap();
    assert_eq!(error["code"], "cli.usage");
    let mut human_args = arguments.to_vec();
    human_args.push("--output=human");
    let human_output = server.run(&human_args).await;
    assert_eq!(human_output.status.code(), Some(2));
    assert!(human_output.stdout.is_empty());
    assert_eq!(
        error["message"].as_str().unwrap().trim_end(),
        String::from_utf8_lossy(&human_output.stderr).trim_end()
    );
}

pub async fn assert_planned_task_output(server: &Server, space: &str, planned: &Value) {
    let output = server.run(&["task", "show", space, "TK-1"]).await;
    assert_success(&output);
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(&format!(
        "Created at: {}\n",
        human_time(&planned["created_at"])
    )));
    assert!(text.contains("Started at: \nEnded at: \n"));
    assert!(text.ends_with("Files: \n"));
}
