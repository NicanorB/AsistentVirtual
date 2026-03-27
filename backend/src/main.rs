mod auth;
mod chat;
mod common;
mod documents;

#[cfg(test)]
mod tests;

use axum::{Router, routing::post};
use sqlx::postgres::PgPoolOptions;
use std::{fs, net::SocketAddr, sync::Arc};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use chat::router as chat_router;
use common::{AppConfig, AppState};
use documents::router as documents_router;

fn build_router(state: AppState) -> Router {
    let auth_routes = Router::new()
        .route("/signup", post(auth::signup))
        .route("/login", post(auth::login))
        .route("/refresh_token", post(auth::refresh_token));

    let protected_routes = Router::new().merge(documents_router()).merge(chat_router());

    Router::new()
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
