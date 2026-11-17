use std::env;
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

use crate::args::{Cli, OutputMode};
use crate::error::{Failure, Result};

mod args;
mod commands;
mod credentials;
mod error;
mod http;
mod login;
mod output;

#[tokio::main]
async fn main() -> ExitCode {
    let arguments: Vec<_> = env::args_os().collect();
    let fallback = arguments
        .windows(2)
        .any(|pair| pair[0] == "--output" && pair[1] == "json")
        || arguments.iter().any(|arg| arg == "--output=json");
    let fallback = if fallback {
        OutputMode::Json
    } else {
        OutputMode::Human
    };
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            return finish(fallback, output::help(&error.to_string()));
        }
        Err(error) => {
            return finish(
                fallback,
                Err(Failure::local("cli.usage", error.to_string())),
            );
        }
    };
    let result = match cli.command {
        Some(command) => commands::run(command, cli.output)
            .await
            .and_then(|outcome| output::success(cli.output, outcome)),
        None => output::help(&Cli::command().render_help().to_string()),
    };
    finish(cli.output, result)
}

fn finish(mode: OutputMode, result: Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            output::failure(mode, &error);
            ExitCode::from(error.exit_code())
        }
    }
}
