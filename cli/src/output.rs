use std::fmt::Debug;
use std::io::{self, IsTerminal, Write};
use std::sync::Arc;

use serde_json::{Value, json};
use typed_builder::TypedBuilder;

use crate::constants::{CLI_CANCELLED, DATA, META};

use crate::args::OutputMode;
use crate::error::{Failure, Result};

pub(crate) trait Render: Debug {
    fn human(&self) -> String;
}

#[derive(Clone, Debug)]
pub(crate) enum Field {
    Multiline(&'static str, String),
    SingleLine(&'static str, String),
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Fields {
    pub values: Vec<Field>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Outcome {
    pub body: Value,
    pub human: Arc<dyn Render>,
    #[builder(default = None)]
    pub next: Option<String>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Table {
    pub columns: Vec<&'static str>,
    pub rows: Vec<Vec<String>>,
}

impl Render for Fields {
    fn human(&self) -> String {
        self.values
            .iter()
            .map(|field| match field {
                Field::Multiline(label, value) => format!("{label}: {}", multiline(value)),
                Field::SingleLine(label, value) => format!("{label}: {}", safe(value)),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Outcome {
    pub fn new(body: Value, human: impl Render + 'static) -> Self {
        Self {
            body,
            human: Arc::new(human),
            next: None,
        }
    }

    pub fn completion(message: &'static str) -> Self {
        Self::new(
            json!({(DATA): null, (META): {}}),
            Completion::builder().message(message).build(),
        )
    }
}

impl Render for Table {
    fn human(&self) -> String {
        let mut lines = vec![self.columns.join(COLUMN_SEPARATOR)];
        lines.extend(self.rows.iter().map(|row| {
            row.iter()
                .map(|cell| safe(cell))
                .collect::<Vec<_>>()
                .join(COLUMN_SEPARATOR)
        }));
        lines.join("\n")
    }
}

pub(crate) fn success(mode: OutputMode, outcome: Outcome) -> Result<()> {
    let text = match mode {
        OutputMode::Human => outcome.human.human(),
        OutputMode::Json => outcome.body.to_string(),
    };
    if let Some(next) = outcome.next {
        hint(&next)?;
    }
    writeln!(io::stdout().lock(), "{text}")?;
    Ok(())
}

pub(crate) fn failure(mode: OutputMode, error: &Failure) {
    let text = match mode {
        OutputMode::Human => safe(&error.human()),
        OutputMode::Json => error.body().to_string(),
    };
    let _ = writeln!(io::stderr().lock(), "{text}");
}

pub(crate) fn help(text: &str) -> Result<()> {
    write!(io::stdout().lock(), "{text}")?;
    Ok(())
}

pub(crate) fn hint(text: &str) -> Result<()> {
    writeln!(io::stderr().lock(), "{}", safe(text))?;
    Ok(())
}

pub(crate) fn confirm(target: &str) -> Result<()> {
    hint(&format!("Revoke agent {}? [y/N]", safe(target)))?;
    let mut answer = String::new();
    if io::stdin().is_terminal() {
        io::stdin().read_line(&mut answer)?;
    }
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        Err(Failure::local(CLI_CANCELLED, "Revocation cancelled."))
    }
}

pub(crate) fn safe(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

pub(crate) fn shell_word(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

const COLUMN_SEPARATOR: &str = "\t";

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct Completion {
    message: &'static str,
}

impl Render for Completion {
    fn human(&self) -> String {
        self.message.to_owned()
    }
}

fn multiline(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() && !matches!(c, '\n' | '\t') {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
