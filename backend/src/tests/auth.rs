use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::{Connection, Executor, PgConnection, PgPool, postgres::PgPoolOptions};
use std::sync::{Arc, OnceLock};
use tower::util::ServiceExt;
use uuid::Uuid;

use crate::{
    build_router,
    common::{AppConfig, AppState},
};

static TEST_DATABASE_URL: OnceLock<String> = OnceLock::new();

fn test_database_url() -> &'static str {
    TEST_DATABASE_URL.get_or_init(|| {
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests")
    })
}

fn admin_database_url() -> String {
    format!("{}/postgres", test_database_url().trim_end_matches('/'))
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

fn test_config() -> Arc<AppConfig> {
    Arc::new(AppConfig {
        jwt_access_secret: "integration-test-access-secret".to_string(),
        jwt_refresh_secret: "integration-test-refresh-secret".to_string(),
        access_ttl: time::Duration::minutes(5),
        refresh_ttl: time::Duration::days(30),
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

struct TestApp {
    app: axum::Router,
    pool: PgPool,
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

async fn test_app() -> TestApp {
    let (pool, database_name) = create_test_pool().await;

    let state = AppState {
        pool: pool.clone(),
        config: test_config(),
    };

    TestApp {
        app: build_router(state),
        pool,
        database_name,
    }
}

async fn send_json(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
) -> axum::response::Response {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request should build");

    app.oneshot(request).await.expect("request should succeed")
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");

    serde_json::from_slice(&bytes).expect("response body should be valid json")
}

#[tokio::test]
async fn signup_returns_token_pair_and_persists_user_and_refresh_token() {
    let test_app = test_app().await;
    let username = format!("user-{}", Uuid::new_v4());

    let response = send_json(
        test_app.app.clone(),
        "POST",
        "/api/auth/signup",
        json!({
            "username": username,
            "password": "very-secure-password",
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = response_json(response).await;

    assert_eq!(body["token_type"], "Bearer");
    assert!(body["expires_in_seconds"].as_i64().unwrap() > 0);
    assert!(body["access_token"].as_str().unwrap().len() > 20);
    assert!(body["refresh_token"].as_str().unwrap().len() > 20);

    let stored_user: Option<(String,)> =
        sqlx::query_as("SELECT username FROM users WHERE username = $1")
            .bind(username.as_str())
            .fetch_optional(&test_app.pool)
            .await
            .expect("user lookup should succeed");

    assert_eq!(stored_user.map(|row| row.0), Some(username));

    let refresh_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM refresh_tokens WHERE token = $1")
            .bind(body["refresh_token"].as_str().unwrap())
            .fetch_one(&test_app.pool)
            .await
            .expect("refresh token should be persisted");

    assert_eq!(refresh_count.0, 1);
}

#[tokio::test]
async fn signup_rejects_duplicate_username() {
    let test_app = test_app().await;
    let username = format!("duplicate-{}", Uuid::new_v4());

    let first_response = send_json(
        test_app.app.clone(),
        "POST",
        "/api/auth/signup",
        json!({
            "username": username,
            "password": "very-secure-password",
        }),
    )
    .await;

    assert_eq!(first_response.status(), StatusCode::OK);

    let second_response = send_json(
        test_app.app.clone(),
        "POST",
        "/api/auth/signup",
        json!({
            "username": username,
            "password": "very-secure-password",
        }),
    )
    .await;

    assert_eq!(second_response.status(), StatusCode::CONFLICT);

    let body = response_json(second_response).await;
    assert_eq!(body["error"], "conflict");
    assert_eq!(body["message"], "username already exists");
}

#[tokio::test]
async fn login_returns_token_pair_for_existing_user() {
    let test_app = test_app().await;
    let username = format!("login-{}", Uuid::new_v4());

    let signup_response = send_json(
        test_app.app.clone(),
        "POST",
        "/api/auth/signup",
        json!({
            "username": username,
            "password": "very-secure-password",
        }),
    )
    .await;

    assert_eq!(signup_response.status(), StatusCode::OK);

    let login_response = send_json(
        test_app.app.clone(),
        "POST",
        "/api/auth/login",
        json!({
            "username": username,
            "password": "very-secure-password",
        }),
    )
    .await;

    assert_eq!(login_response.status(), StatusCode::OK);

    let body = response_json(login_response).await;

    assert_eq!(body["token_type"], "Bearer");
    assert!(body["expires_in_seconds"].as_i64().unwrap() > 0);
    assert!(body["access_token"].as_str().unwrap().len() > 20);
    assert!(body["refresh_token"].as_str().unwrap().len() > 20);
}

#[tokio::test]
async fn login_rejects_invalid_password() {
    let test_app = test_app().await;
    let username = format!("wrong-pass-{}", Uuid::new_v4());

    let signup_response = send_json(
        test_app.app.clone(),
        "POST",
        "/api/auth/signup",
        json!({
            "username": username,
            "password": "very-secure-password",
        }),
    )
    .await;

    assert_eq!(signup_response.status(), StatusCode::OK);

    let login_response = send_json(
        test_app.app.clone(),
        "POST",
        "/api/auth/login",
        json!({
            "username": username,
            "password": "wrong-password",
        }),
    )
    .await;

    assert_eq!(login_response.status(), StatusCode::UNAUTHORIZED);

    let body = response_json(login_response).await;
    assert_eq!(body["error"], "unauthorized");
    assert_eq!(body["message"], "unauthorized");
}

#[tokio::test]
async fn login_rejects_unknown_user() {
    let test_app = test_app().await;

    let response = send_json(
        test_app.app.clone(),
        "POST",
        "/api/auth/login",
        json!({
            "username": format!("missing-{}", Uuid::new_v4()),
            "password": "very-secure-password",
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = response_json(response).await;
    assert_eq!(body["error"], "unauthorized");
    assert_eq!(body["message"], "unauthorized");
}
