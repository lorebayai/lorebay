use std::fs;
use std::io::{self, Read};
use std::path::Path;

use crate::args::ContentOptions;
use crate::constants::CLI_USAGE;
use crate::error::{Failure, Result};

pub(super) fn read(options: ContentOptions) -> Result<Option<String>> {
    let Some(path) = options.content_file else {
        return Ok(options.content);
    };
    let result = if path == Path::new("-") {
        let mut content = String::new();
        io::stdin().read_to_string(&mut content).map(|_| content)
    } else {
        fs::read_to_string(path)
    };
    result.map(Some).map_err(|error| {
        Failure::local(CLI_USAGE, format!("Could not read UTF-8 content: {error}"))
    })
}
