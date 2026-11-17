use axum::Router;
use axum::routing::{delete, get, post};

use crate::server::ServerState;

pub(crate) fn router() -> Router<ServerState> {
    Router::new()
        .route("/agents", get(agents::list))
        .route("/agents/{name}", delete(agents::revoke))
        .route("/spaces", get(spaces::list).post(spaces::create))
        .route("/spaces/{space}", get(spaces::read).patch(spaces::change))
        .route(
            "/spaces/{space}/tasks",
            get(tasks::list).post(tasks::create),
        )
        .route(
            "/spaces/{space}/tasks/{task}",
            get(tasks::read).patch(tasks::change),
        )
        .route(
            "/spaces/{space}/tasks/{task}/notes",
            post(tasks::append_note),
        )
        .route(
            "/spaces/{space}/tasks/{task}/scratchpad",
            get(tasks::scratchpad),
        )
        .route(
            "/spaces/{space}/tasks/{task}/artifacts",
            post(artifacts::register),
        )
        .route(
            "/spaces/{space}/tasks/{task}/artifacts/{key}/complete",
            post(artifacts::complete),
        )
        .route("/users", get(users::list))
        .route("/spaces/{space}/participants", get(participants::list))
        .route(
            "/spaces/{space}/participants/{username}",
            post(participants::add)
                .patch(participants::change_role)
                .delete(participants::remove),
        )
        .route("/device-sign-in-request", post(device_sign_in::read))
        .route("/device-sign-in-decision", post(device_sign_in::decide))
        .route("/authorization-request", post(authorization::read))
        .route("/authorization-decision", post(authorization::decide))
        .route(
            "/login-session",
            post(login_session::log_in).delete(login_session::log_out),
        )
        .route("/login-session/refresh", post(login_session::refresh))
}

mod agents;
mod artifacts;
mod authorization;
mod device_sign_in;
mod login_session;
mod participants;
mod spaces;
mod tasks;
mod users;
