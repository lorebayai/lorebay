use std::fmt::{self, Debug, Formatter};

use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

/// Redacts Debug output; serialization preserves the value and must stay out of logs.
#[derive(Clone, Deserialize, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(transparent)]
pub struct Secret<T> {
    value: T,
}

impl<T> Secret<T> {
    pub fn expose(&self) -> &T {
        &self.value
    }
}

impl<T> From<T> for Secret<T> {
    fn from(value: T) -> Self {
        Self { value }
    }
}

impl<T> Debug for Secret<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}
