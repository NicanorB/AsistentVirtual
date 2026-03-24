mod auth;
mod common;

#[cfg(test)]
mod tests;

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde::Serialize;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{fs, net::SocketAddr, sync::Arc};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

use auth::AuthUser;
use common::{ApiError, AppConfig, AppState};

//
// Configuration
//

#[derive(Serialize)]
struct MockProtectedResponse {
    ok: bool,
    user_id: String,
}

async fn mock_protected(user: AuthUser) -> Json<MockProtectedResponse> {
    Json(MockProtectedResponse {
        ok: true,
        user_id: user.user_id.to_string(),
    })
}

#[derive(Serialize)]
struct Settings {
    name: String,
    status: String,
}

async fn get_settings() -> Json<Settings> {
    Json(Settings {
        name: "AsistentVirtual".to_string(),
        status: "OK".to_string(),
    })
}

//
// Example of an authenticated DB-backed endpoint pattern
// (kept as a reference; you can remove or expand it later).
//

#[allow(dead_code)]
#[derive(Debug, Serialize, sqlx::FromRow)]
struct DocumentRow {
    id: Uuid,
    user_id: Uuid,
    title: String,
    body: String,
}

#[allow(dead_code)]
async fn list_my_documents(
    State(pool): State<PgPool>,
    user: AuthUser,
) -> Result<Json<Vec<DocumentRow>>, ApiError> {
    let docs = sqlx::query_as::<_, DocumentRow>(
        r#"
        SELECT id, user_id, title, file
        FROM documents
        WHERE user_id = $1
        ORDER BY id DESC
        "#,
    )
    .bind(user.user_id)
    .fetch_all(&pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(Json(docs))
}

//
// App bootstrap
//

fn build_router(state: AppState) -> Router {
    let auth_routes = Router::new()
        .route("/signup", post(auth::signup))
        .route("/login", post(auth::login))
        .route("/refresh_token", post(auth::refresh_token));

    let protected_routes = Router::new().route("/mock", get(mock_protected));
    // Example:
    // .route("/documents", get(list_my_documents));

    Router::new()
        .route("/api/get_settings", get(get_settings))
        .nest("/api/auth", auth_routes)
        .nest("/api", protected_routes)
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Default to info if RUST_LOG isn't set
                tracing_subscriber::EnvFilter::new("info")
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        database_url
    } else {
        let postgres_password = if let Ok(value) = std::env::var("POSTGRES_PASSWORD") {
            if value.trim().is_empty() {
                anyhow::bail!("POSTGRES_PASSWORD is set but empty");
            }
            value
        } else {
            let file_path = std::env::var("POSTGRES_PASSWORD_FILE").map_err(|_| {
                anyhow::anyhow!(
                    "DATABASE_URL is not set and neither POSTGRES_PASSWORD nor POSTGRES_PASSWORD_FILE are set"
                )
            })?;

            let password = fs::read_to_string(&file_path)
                .map_err(|err| anyhow::anyhow!("failed to read secret file {file_path}: {err}"))?;

            let password = password.trim().to_string();

            if password.is_empty() {
                anyhow::bail!("secret loaded from {file_path} is empty");
            }

            password
        };

        format!("postgres://postgres:{postgres_password}@db:5432/postgres")
    };

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    sqlx::migrate!().run(&pool).await?;

    let config = Arc::new(AppConfig::from_env()?);

    let state = AppState { pool, config };
    let app = build_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
