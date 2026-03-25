use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use axum::{
    body::Body,
    http::{Request, header},
};
use serde_json::Value;
use sqlx::{Connection, Executor, PgConnection, PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::{
    build_router,
    common::{AppConfig, AppState},
};

pub mod auth;
pub mod documents;

static TEST_DATABASE_URL: OnceLock<String> = OnceLock::new();

pub(super) struct TestApp {
    pub(super) app: axum::Router,
    pub(super) pool: PgPool,
    pub(super) config: Arc<AppConfig>,
    database_name: String,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let database_name = self.database_name.clone();
        let pool = self.pool.clone();

        tokio::spawn(async move {
            pool.close().await;
            drop_test_database(&database_name).await;
        });
    }
}

fn test_database_name() -> String {
    format!("asistent_virtual_test_{}", Uuid::new_v4().simple())
}

fn test_database_url_for(database_name: &str) -> String {
    format!(
        "{}/{}",
        test_database_url().trim_end_matches('/'),
        database_name
    )
}

fn admin_database_url() -> String {
    format!("{}/postgres", test_database_url().trim_end_matches('/'))
}

fn test_database_url() -> &'static str {
    TEST_DATABASE_URL.get_or_init(|| {
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests")
    })
}

async fn create_test_pool() -> (PgPool, String) {
    let database_name = test_database_name();
    let mut admin_connection = PgConnection::connect(&admin_database_url())
        .await
        .expect("admin database connection should succeed");

    admin_connection
        .execute(format!(r#"CREATE DATABASE "{}""#, database_name).as_str())
        .await
        .expect("test database creation should succeed");

    let database_url = test_database_url_for(&database_name);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("test database connection should succeed");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("migrations should succeed");

    (pool, database_name)
}

async fn drop_test_database(database_name: &str) {
    let mut admin_connection = PgConnection::connect(&admin_database_url())
        .await
        .expect("admin database connection should succeed");

    admin_connection
        .execute(
            format!(
                r#"DROP DATABASE IF EXISTS "{}" WITH (FORCE)"#,
                database_name
            )
            .as_str(),
        )
        .await
        .expect("test database cleanup should succeed");
}

fn test_config() -> Arc<AppConfig> {
    Arc::new(AppConfig {
        jwt_access_secret: "integration-test-access-secret".to_string(),
        jwt_refresh_secret: "integration-test-refresh-secret".to_string(),
        access_ttl: time::Duration::minutes(5),
        refresh_ttl: time::Duration::days(30),
        documents_dir: test_documents_dir().to_string_lossy().into_owned(),
    })
}

fn test_documents_dir() -> PathBuf {
    std::env::temp_dir().join(format!("asistent_virtual_test_docs_{}", Uuid::new_v4()))
}

pub(super) async fn send_json(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
) -> axum::response::Response {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request should build");

    tower::util::ServiceExt::oneshot(app, request)
        .await
        .expect("request should succeed")
}

pub(super) async fn send_multipart(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Vec<u8>,
    boundary: &str,
    authorization: Option<&str>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri).header(
        header::CONTENT_TYPE,
        format!("multipart/form-data; boundary={boundary}"),
    );

    if let Some(token) = authorization {
        builder = builder.header(header::AUTHORIZATION, token);
    }

    let request = builder
        .body(Body::from(body))
        .expect("multipart request should build");

    tower::util::ServiceExt::oneshot(app, request)
        .await
        .expect("request should succeed")
}

pub(super) async fn response_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");

    serde_json::from_slice(&bytes).expect("response body should be valid json")
}

pub(super) fn multipart_body(
    field_name: &str,
    file_name: &str,
    content_type: &str,
    contents: &[u8],
    boundary: &str,
) -> Vec<u8> {
    let mut body = Vec::new();

    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"{field_name}\"; filename=\"{file_name}\"\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(contents);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    body
}

pub(super) async fn auth_header_for(app: axum::Router, username: String, password: &str) -> String {
    let response = send_json(
        app,
        "POST",
        "/api/auth/signup",
        serde_json::json!({
            "username": username,
            "password": password,
        }),
    )
    .await;

    let body = response_json(response).await;
    format!("Bearer {}", body["access_token"].as_str().unwrap())
}

pub(super) async fn test_app() -> TestApp {
    let (pool, database_name) = create_test_pool().await;

    let config = test_config();
    let state = AppState {
        pool: pool.clone(),
        config: config.clone(),
    };

    TestApp {
        app: build_router(state),
        pool,
        config,
        database_name,
    }
}
