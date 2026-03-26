use std::{fs, sync::Arc};

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
    /// Directory where uploaded documents are stored.
    pub documents_dir: String,
}

impl AppConfig {
    fn read_env_or_file(env_key: &str, file_key: &str) -> anyhow::Result<String> {
        if let Ok(value) = std::env::var(env_key)
            && !value.trim().is_empty()
        {
            return Ok(value);
        }

        let file_path = std::env::var(file_key).map_err(|_| {
            anyhow::anyhow!(
                "{env_key} is not set and fallback secret file env {file_key} is not set"
            )
        })?;

        let value = fs::read_to_string(&file_path)
            .map_err(|err| anyhow::anyhow!("failed to read secret file {file_path}: {err}"))?;

        let value = value.trim().to_string();

        if value.is_empty() {
            anyhow::bail!("secret loaded from {file_path} is empty");
        }

        Ok(value)
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let jwt_access_secret =
            Self::read_env_or_file("JWT_ACCESS_SECRET", "JWT_ACCESS_SECRET_FILE")?;
        let jwt_refresh_secret =
            Self::read_env_or_file("JWT_REFRESH_SECRET", "JWT_REFRESH_SECRET_FILE")?;
        let documents_dir = std::env::var("DOCUMENTS_DIR")
            .map_err(|_| anyhow::anyhow!("DOCUMENTS_DIR is not set"))?;

        Ok(Self {
            jwt_access_secret,
            jwt_refresh_secret,
            access_ttl: Duration::minutes(5),
            refresh_ttl: Duration::days(30),
            documents_dir,
        })
    }
}

impl FromRef<AppState> for Arc<AppConfig> {
    fn from_ref(state: &AppState) -> Arc<AppConfig> {
        state.config.clone()
    }
}
