use anyhow::Result;
use uuid::Uuid;

const RANDOM_ID_BYTES: usize = 16;

pub(crate) fn primary_key() -> String {
    Uuid::now_v7().to_string()
}

pub(crate) fn random_id() -> Result<String> {
    let mut bytes = [0; RANDOM_ID_BYTES];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
