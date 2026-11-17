use crate::args::{OutputMode, UserCommand};
use crate::credentials::Store;
use crate::error::Result;
use crate::output::Outcome;

use super::{Listing, api_path, list_page};

pub(super) async fn run(command: UserCommand, store: &Store, mode: OutputMode) -> Result<Outcome> {
    match command {
        UserCommand::List(page) => {
            list_page(
                store,
                page,
                mode,
                Listing::builder()
                    .path(api_path(&[USERS])?)
                    .command("user list")
                    .columns(vec!["Username"])
                    .fields(vec!["username"])
                    .build(),
            )
            .await
        }
    }
}

const USERS: &str = "users";
