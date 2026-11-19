use typed_builder::TypedBuilder;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct PageOptions {
    pub page_size: u64,
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

pub const DEFAULT_PAGE_SIZE: u64 = 20;
pub const MAX_PAGE_SIZE: u64 = 100;

#[derive(Debug, thiserror::Error)]
#[error("Invalid pagination parameter.")]
pub struct PageSizeError;

impl PageSizeError {
    pub const FIELD: &str = "page_size";
    pub const FIELD_CODE: &str = "invalid";
}

impl PageOptions {
    pub fn new(page_size: Option<u64>, cursor: Option<String>) -> Result<Self, PageSizeError> {
        let options = Self {
            page_size: page_size.unwrap_or(DEFAULT_PAGE_SIZE),
            cursor,
        };
        options.validate()?;
        Ok(options)
    }

    pub fn from_text(
        page_size: Option<&str>,
        cursor: Option<String>,
    ) -> Result<Self, PageSizeError> {
        let page_size = page_size
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|_| PageSizeError)?;
        Self::new(page_size, cursor)
    }

    pub fn validate(&self) -> Result<(), PageSizeError> {
        if (1..=MAX_PAGE_SIZE).contains(&self.page_size) {
            Ok(())
        } else {
            Err(PageSizeError)
        }
    }
}
