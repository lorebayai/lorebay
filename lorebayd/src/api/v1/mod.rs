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
            "/spaces/{space}/files/{key}/url",
            get(agent_files::download_url),
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
            "/spaces/{space}/tasks/{task}/checkpoints",
            post(tasks::append_checkpoint),
        )
        .route(
            "/spaces/{space}/tasks/{task}/checkpoints/{time}",
            delete(tasks::remove_checkpoint),
        )
        .route(
            "/spaces/{space}/tasks/{task}/scratchpad",
            get(tasks::scratchpad),
        )
        .route(
            "/spaces/{space}/tasks/{task}/files",
            post(agent_files::register),
        )
        .route(
            "/spaces/{space}/tasks/{task}/files/{key}",
            delete(agent_files::delete),
        )
        .route(
            "/spaces/{space}/tasks/{task}/files/{key}/complete",
            post(agent_files::complete),
        )
        .route("/users", get(users::list))
        .route("/spaces/{space}/members", get(space_members::list))
        .route(
            "/spaces/{space}/members/{username}",
            post(space_members::add)
                .patch(space_members::change_role)
                .delete(space_members::remove),
        )
        .route("/device-sign-in-request", post(device_sign_in::read))
        .route("/device-sign-in-decision", post(device_sign_in::decide))
        .route("/authorization-request", post(authorization::read))
        .route("/authorization-decision", post(authorization::decide))
        .route(
            "/sign-in",
            post(person_sign_in::log_in).delete(person_sign_in::log_out),
        )
        .route("/sign-in/refresh", post(person_sign_in::refresh))
}

mod agent_files;
mod agents;
mod authorization;
mod device_sign_in;
mod knowledge;
mod person_sign_in;
mod proposals;
mod space_members;
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
