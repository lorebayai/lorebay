use std::collections::HashMap;
use std::io::{Read, Result as IoResult};
use std::path::Path;

use dotenvy::Error as DotenvError;
use typed_builder::TypedBuilder;

use crate::ConfigError;
use crate::config::read_config_file;

pub(crate) fn dotenv_environment(
    path: Option<&Path>,
) -> Result<HashMap<String, String>, ConfigError> {
    let Some(path) = path else {
        return Ok(HashMap::new());
    };
    let content = read_config_file(path)?;
    let mut reader = LineReader {
        remaining: content.as_bytes(),
        line: 0,
    };
    dotenvy::Iter::new(&mut reader)
        .collect::<Result<_, _>>()
        .map_err(|error| ConfigError::EnvParse {
            path: path.to_owned(),
            line: error_line(error, reader.line),
        })
}

fn error_line(error: DotenvError, last_line: usize) -> usize {
    let DotenvError::LineParse(input, index) = error else {
        return last_line.max(1);
    };
    let newlines = input.bytes().filter(|byte| *byte == b'\n').count();
    let preceding_newlines = input.chars().take(index).filter(|ch| *ch == '\n').count();
    (last_line.saturating_sub(newlines) + preceding_newlines).max(1)
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct LineReader<'a> {
    remaining: &'a [u8],
    line: usize,
}

impl Read for LineReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> IoResult<usize> {
        // Limiting read-ahead preserves the parser's physical line without displaying its input.
        let length = self
            .remaining
            .iter()
            .take(buffer.len())
            .position(|byte| *byte == b'\n')
            .map_or(self.remaining.len(), |index| index + 1)
            .min(buffer.len());
        let (chunk, remaining) = self.remaining.split_at(length);
        if chunk.ends_with(b"\n") || (!chunk.is_empty() && remaining.is_empty()) {
            self.line += 1;
        }
        buffer[..length].copy_from_slice(chunk);
        self.remaining = remaining;
        Ok(length)
    }
}
