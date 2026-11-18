use std::process::Output;

use serde_json::json;
use url::Url;

use super::body;

pub fn printed_artifact_url(output: &Output, mode: &str) -> Url {
    let text = if mode == "json" {
        let response = body(output);
        assert_eq!(response.as_object().unwrap().len(), 2);
        assert_eq!(response["meta"], json!({}));
        assert_eq!(response["data"].as_object().unwrap().len(), 2);
        response["data"]["url"].as_str().unwrap().to_owned()
    } else {
        let text = String::from_utf8(output.stdout.clone()).unwrap();
        assert_eq!(text.lines().count(), 1);
        text.strip_suffix('\n').unwrap().to_owned()
    };
    let url = Url::parse(&text).unwrap();
    let expires = url
        .query_pairs()
        .find(|(key, _)| key == "expires")
        .unwrap()
        .1
        .parse::<i64>()
        .unwrap();
    if mode == "json" {
        assert_eq!(body(output)["data"]["expires_at"], expires);
    }
    url
}
