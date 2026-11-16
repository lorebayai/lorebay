use std::collections::HashMap;
use std::path::Path;

use mise_dotenv::{Error as DotenvError, parse};

use crate::ConfigError;
use crate::config::read_config_file;

pub(crate) fn dotenv_environment(
    path: Option<&Path>,
    environment: &HashMap<String, String>,
) -> Result<HashMap<String, String>, ConfigError> {
    let Some(path) = path else {
        return Ok(HashMap::new());
    };
    let content = read_config_file(path)?;
    parse(&content, true, environment.clone())
        .map(|assignments| assignments.into_iter().collect())
        .map_err(|error| ConfigError::EnvParse {
            path: path.to_owned(),
            line: match error {
                DotenvError::Parse(error, _) => error.line(),
                _ => 1,
            },
        })
}
