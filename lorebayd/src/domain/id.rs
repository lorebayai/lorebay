use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use uuid::Uuid;

const RANDOM_ID_BYTES: usize = 16;

pub(crate) fn primary_key() -> String {
    Uuid::now_v7().to_string()
}

pub(crate) fn random_id() -> Result<String, getrandom::Error> {
    let mut bytes = [0; RANDOM_ID_BYTES];
    getrandom::fill(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
