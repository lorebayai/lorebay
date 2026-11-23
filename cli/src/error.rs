use std::io;
use std::result::Result as StdResult;

use reqwest::StatusCode;
use serde_json::{Value, json};
use thiserror::Error;

use crate::constants::{
    CLI_CONNECTION, CLI_CREDENTIALS, CLI_LOGIN_REQUIRED, CLI_TIMEOUT, CLI_USAGE, CODE, LOGIN_HINT,
    MESSAGE, STATUS, TITLE,
};

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
        let exit = if body[STATUS] == StatusCode::UNAUTHORIZED.as_u16() {
            LOGIN_REQUIRED_EXIT
        } else {
            FAILURE_EXIT
        };
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
            CLI_LOGIN_REQUIRED,
            "No active sign-in; run `lorebay login` again.",
        )
    }

    pub fn connection(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::local(CLI_TIMEOUT, "The server request timed out.")
        } else {
            Self::local(
                CLI_CONNECTION,
                "Could not connect to the server or verify its certificate.",
            )
        }
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Local {
                code: CLI_USAGE, ..
            } => USAGE_EXIT,
            Self::Local {
                code: CLI_LOGIN_REQUIRED,
                ..
            } => LOGIN_REQUIRED_EXIT,
            Self::Server { exit, .. } => *exit,
            _ => FAILURE_EXIT,
        }
    }

    pub fn body(&self) -> Value {
        match self {
            Self::Credentials(_) => json!({(CODE): CLI_CREDENTIALS, (MESSAGE): self.to_string()}),
            Self::Local { code, message } => json!({(CODE): code, (MESSAGE): message}),
            Self::Server { body, .. } => body.clone(),
        }
    }

    pub fn human(&self) -> String {
        let body = self.body();
        let mut parts = vec![
            body[TITLE]
                .as_str()
                .or(body[MESSAGE].as_str())
                .unwrap_or("The server refused the operation.")
                .to_owned(),
        ];
        if let Some(errors) = body["errors"].as_array() {
            parts.extend(
                errors
                    .iter()
                    .filter_map(|e| e[MESSAGE].as_str())
                    .map(str::to_owned),
            );
        }
        if body[TITLE].is_string()
            && let Some(message) = body[MESSAGE].as_str()
        {
            parts.push(message.to_owned());
        }
        parts.join(": ")
    }

    pub fn is_ended(&self) -> bool {
        matches!(self, Self::Server { body, .. } if body[CODE] == "invalid_grant"
            || body[CODE] == "invalid_token" || body[CODE] == "auth.unauthorized")
    }

    pub fn require_login(self) -> Self {
        let mut failure = self.context(LOGIN_HINT);
        if let Self::Server { exit, .. } = &mut failure {
            *exit = LOGIN_REQUIRED_EXIT;
        }
        failure
    }

    pub fn context(self, context: &str) -> Self {
        let exit = self.exit_code();
        let mut body = self.body();
        let message = body[MESSAGE]
            .as_str()
            .map(|message| format!("{message} {context}"))
            .unwrap_or_else(|| context.to_owned());
        body[MESSAGE] = json!(message);
        Self::Server { body, exit }
    }
}

const FAILURE_EXIT: u8 = 1;
const LOGIN_REQUIRED_EXIT: u8 = 4;
const USAGE_EXIT: u8 = 2;
