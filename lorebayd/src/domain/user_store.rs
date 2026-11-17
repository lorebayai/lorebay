use std::fmt::Debug;

use anyhow::Result;

pub(crate) trait UserStore: Debug + Send + Sync {
    /// Lists the snapshot's usernames; failures stop account synchronization before its transaction.
    fn usernames(&self) -> Result<Vec<String>>;

    /// Returns false for unknown usernames and wrong passwords, or an error for a backend failure.
    fn verify(&self, username: &str, password: &str) -> Result<bool>;
}
