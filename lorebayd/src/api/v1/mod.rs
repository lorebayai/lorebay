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
        .merge(person_requests_router())
        .route(
            "/spaces/{space}/files/{key}/url",
            get(agent_files::download_url),
        )
        .merge(task_router())
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
mod person_requests;
mod person_sign_in;
mod space_members;
mod spaces;
mod tasks;
mod users;

fn task_router() -> Router<ServerState> {
    Router::new()
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
        .route("/spaces/{space}/tasks/{task}/results", get(tasks::results))
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
}

fn person_requests_router() -> Router<ServerState> {
    Router::new()
        .route(
            "/spaces/{space}/requests",
            get(person_requests::list).merge(
                post(person_requests::submit_person)
                    .layer(DefaultBodyLimit::max(knowledge::KNOWLEDGE_BODY_MAX_BYTES)),
            ),
        )
        .route(
            "/spaces/{space}/tasks/{task}/requests",
            post(person_requests::submit_agent)
                .layer(DefaultBodyLimit::max(knowledge::KNOWLEDGE_BODY_MAX_BYTES)),
        )
        .route("/spaces/{space}/requests/{key}", get(person_requests::read))
        .route(
            "/spaces/{space}/requests/{key}/approve",
            post(person_requests::approve),
        )
        .route(
            "/spaces/{space}/requests/{key}/reject",
            post(person_requests::reject),
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
