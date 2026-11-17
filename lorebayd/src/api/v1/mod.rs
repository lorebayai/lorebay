mod authorization;
mod login_session;

use axum::Router;
use axum::routing::post;

use crate::server::ServerState;

pub(crate) fn router() -> Router<ServerState> {
    Router::new()
        .route("/authorization-request", post(authorization::read))
        .route("/authorization-decision", post(authorization::decide))
        .route(
            "/login-session",
            post(login_session::log_in).delete(login_session::log_out),
        )
        .route("/login-session/refresh", post(login_session::refresh))
}
