use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, patch, post};

use crate::server::ServerState;

pub(crate) fn router() -> Router<ServerState> {
    Router::new()
        .route("/agents", get(agents::list))
        .route("/agents/{name}", delete(agents::revoke))
        .route("/spaces", get(spaces::list).post(spaces::create))
        .route("/spaces/{space}", get(spaces::read).patch(spaces::change))
        .merge(knowledge_router())
        .merge(proposals_router())
        .route(
            "/spaces/{space}/artifacts/{key}/url",
            get(artifacts::download_url),
        )
        .route(
            "/spaces/{space}/tasks",
            get(tasks::list).post(tasks::create),
        )
        .route(
            "/spaces/{space}/tasks/{task}",
            get(tasks::read).patch(tasks::change).delete(tasks::delete),
        )
        .route(
            "/spaces/{space}/tasks/{task}/notes",
            post(tasks::append_note),
        )
        .route(
            "/spaces/{space}/tasks/{task}/notes/{time}",
            delete(tasks::remove_note),
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
            "/spaces/{space}/tasks/{task}/artifacts/{key}",
            delete(artifacts::delete),
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
mod knowledge;
mod login_session;
mod participants;
mod proposals;
mod spaces;
mod tasks;
mod users;

fn proposals_router() -> Router<ServerState> {
    Router::new()
        .route(
            "/spaces/{space}/proposals",
            get(proposals::list).merge(
                post(proposals::submit_person)
                    .layer(DefaultBodyLimit::max(knowledge::KNOWLEDGE_BODY_MAX_BYTES)),
            ),
        )
        .route(
            "/spaces/{space}/tasks/{task}/proposals",
            post(proposals::submit_agent)
                .layer(DefaultBodyLimit::max(knowledge::KNOWLEDGE_BODY_MAX_BYTES)),
        )
        .route("/spaces/{space}/proposals/{key}", get(proposals::read))
        .route(
            "/spaces/{space}/proposals/{key}/approve",
            post(proposals::approve),
        )
        .route(
            "/spaces/{space}/proposals/{key}/reject",
            post(proposals::reject),
        )
}

fn knowledge_router() -> Router<ServerState> {
    let by_entry = get(knowledge::read).delete(knowledge::delete).merge(
        patch(knowledge::change).layer(DefaultBodyLimit::max(knowledge::KNOWLEDGE_BODY_MAX_BYTES)),
    );
    Router::new()
        .route(
            "/spaces/{space}/knowledge",
            get(knowledge::list).merge(
                post(knowledge::create)
                    .layer(DefaultBodyLimit::max(knowledge::KNOWLEDGE_BODY_MAX_BYTES)),
            ),
        )
        .route("/spaces/{space}/knowledge/entry", by_entry.clone())
        .route("/spaces/{space}/knowledge/by-key/{key}", by_entry)
        .route("/spaces/{space}/knowledge/deleted", get(knowledge::deleted))
        .route(
            "/spaces/{space}/knowledge/by-key/{key}/versions",
            get(knowledge::versions),
        )
        .route(
            "/spaces/{space}/knowledge/entry/versions",
            get(knowledge::versions),
        )
        .route(
            "/spaces/{space}/knowledge/by-key/{key}/versions/{version}",
            get(knowledge::read_version),
        )
        .route(
            "/spaces/{space}/knowledge/entry/versions/{version}",
            get(knowledge::read_version),
        )
        .route(
            "/spaces/{space}/knowledge/by-key/{key}/rollback",
            post(knowledge::roll_back),
        )
        .route(
            "/spaces/{space}/knowledge/entry/rollback",
            post(knowledge::roll_back),
        )
}
