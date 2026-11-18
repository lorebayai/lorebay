use uuid::Uuid;

pub(crate) fn primary_key() -> String {
    Uuid::now_v7().to_string()
}

pub(crate) fn random_task_id() -> Result<String, getrandom::Error> {
    let mut bytes = [0; RANDOM_ID_BYTES];
    getrandom::fill(&mut bytes)?;
    let mut value = u128::from_be_bytes(bytes);
    let mut encoded = [b'0'; TASK_ID_CHARS];
    for digit in encoded.iter_mut().rev() {
        *digit = CROCKFORD_ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    Ok(encoded.into_iter().map(char::from).collect())
}

const CROCKFORD_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
const RANDOM_ID_BYTES: usize = 16;
const TASK_ID_CHARS: usize = 26;
