use std::io;
use std::result::Result as StdResult;

use serde_json::{Value, json};
use thiserror::Error;

pub(crate) type Result<T> = StdResult<T, Failure>;

#[derive(Debug, Error)]
pub(crate) enum Failure {
    #[error(transparent)]
    Credentials(#[from] io::Error),
    #[error("{message}")]
    Local { code: &'static str, message: String },
    #[error("Server refused the operation")]
    Server { body: Value, exit: u8 },
}

impl Failure {
    pub fn server(body: Value) -> Self {
        let exit = if body["status"] == 401 { 4 } else { 1 };
        Self::Server { body, exit }
    }

    pub fn local(code: &'static str, message: impl Into<String>) -> Self {
        Self::Local {
            code,
            message: message.into(),
        }
    }

    pub fn login_required() -> Self {
        Self::local(
            "cli.login_required",
            "No active login session; run `lorebay login` again.",
        )
    }

    pub fn connection(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::local("cli.timeout", "The server request timed out.")
        } else {
            Self::local(
                "cli.connection",
                "Could not connect to the server or verify its certificate.",
            )
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Local {
                code: "cli.usage", ..
            } => 2,
            Self::Local {
                code: "cli.login_required",
                ..
            } => 4,
            Self::Server { exit, .. } => *exit,
            _ => 1,
        }
    }

    pub fn body(&self) -> Value {
        match self {
            Self::Credentials(_) => json!({"code": "cli.credentials", "message": self.to_string()}),
            Self::Local { code, message } => json!({"code": code, "message": message}),
            Self::Server { body, .. } => body.clone(),
        }
    }

    pub fn human(&self) -> String {
        let body = self.body();
        let mut parts = vec![
            body["title"]
                .as_str()
                .or(body["message"].as_str())
                .unwrap_or("The server refused the operation.")
                .to_owned(),
        ];
        if let Some(errors) = body["errors"].as_array() {
            parts.extend(
                errors
                    .iter()
                    .filter_map(|e| e["message"].as_str())
                    .map(str::to_owned),
            );
        }
        if body["title"].is_string()
            && let Some(message) = body["message"].as_str()
        {
            parts.push(message.to_owned());
        }
        parts.join(": ")
    }

    pub fn is_ended(&self) -> bool {
        matches!(self, Self::Server { body, .. } if body["code"] == "invalid_grant"
            || body["code"] == "invalid_token" || body["code"] == "auth.unauthorized")
    }

    pub fn require_login(self) -> Self {
        let mut failure = self.context("Run `lorebay login` again.");
        if let Self::Server { exit, .. } = &mut failure {
            *exit = 4;
        }
        failure
    }

    pub fn context(self, context: &str) -> Self {
        let exit = self.exit_code();
        let mut body = self.body();
        let message = body["message"]
            .as_str()
            .map(|message| format!("{message} {context}"))
            .unwrap_or_else(|| context.to_owned());
        body["message"] = json!(message);
        Self::Server { body, exit }
    }
}
