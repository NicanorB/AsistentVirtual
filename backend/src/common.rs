use std::sync::Arc;

use axum::{
    Json,
    extract::FromRef,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use sqlx::PgPool;
use time::Duration;

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    message: String,
}

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("invalid request: {0}")]
    BadRequest(&'static str),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("conflict: {0}")]
    Conflict(&'static str),
    #[error("internal server error")]
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", "forbidden"),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg),
            ApiError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "internal server error",
            ),
        };

        let body = Json(ErrorBody {
            error: code.to_string(),
            message: message.to_string(),
        });
        (status, body).into_response()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<AppConfig>,
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> PgPool {
        state.pool.clone()
    }
}

#[derive(Clone)]
pub struct AppConfig {
    pub jwt_access_secret: String,
    pub jwt_refresh_secret: String,
    /// Access token validity: 5 minutes.
    pub access_ttl: Duration,
    /// Refresh token validity: longer-lived; used to mint new access tokens.
    pub refresh_ttl: Duration,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let jwt_access_secret = std::env::var("JWT_ACCESS_SECRET").unwrap();
        let jwt_refresh_secret = std::env::var("JWT_REFRESH_SECRET").unwrap();

        Ok(Self {
            jwt_access_secret,
            jwt_refresh_secret,
            access_ttl: Duration::minutes(5),
            refresh_ttl: Duration::days(30),
        })
    }
}

impl FromRef<AppState> for Arc<AppConfig> {
    fn from_ref(state: &AppState) -> Arc<AppConfig> {
        state.config.clone()
    }
}
