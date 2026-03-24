use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::util::ServiceExt;
use uuid::Uuid;

use super::test_app;

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

#[tokio::test]
async fn refresh_returns_new_token_pair_and_invalidates_old_refresh_token() {
    let test_app = test_app().await;
    let username = format!("refresh-{}", Uuid::new_v4());

    // 1. Signup pentru a obține token-urile inițiale
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

    let signup_body = response_json(signup_response).await;
    let old_refresh_token = signup_body["refresh_token"].as_str().unwrap().to_string();

    // 2. Folosim refresh token-ul pentru a obține un nou token pair
    let refresh_response = send_json(
        test_app.app.clone(),
        "POST",
        "/api/auth/refresh",
        json!({
            "refresh_token": old_refresh_token,
        }),
    )
    .await;

    assert_eq!(refresh_response.status(), StatusCode::OK);

    let refresh_body = response_json(refresh_response).await;

    assert_eq!(refresh_body["token_type"], "Bearer");
    assert!(refresh_body["expires_in_seconds"].as_i64().unwrap() > 0);
    assert!(refresh_body["access_token"].as_str().unwrap().len() > 20);
    assert!(refresh_body["refresh_token"].as_str().unwrap().len() > 20);

    // Noul refresh token trebuie să fie diferit de cel vechi
    let new_refresh_token = refresh_body["refresh_token"].as_str().unwrap();
    assert_ne!(new_refresh_token, old_refresh_token);

    // 3. Verificăm că vechiul refresh token a fost invalidat în DB
    let old_token_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM refresh_tokens WHERE token = $1")
            .bind(old_refresh_token.as_str())
            .fetch_one(&test_app.pool)
            .await
            .expect("query should succeed");

    assert_eq!(old_token_count.0, 0, "old refresh token should be deleted");

    // 4. Verificăm că noul refresh token există în DB
    let new_token_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM refresh_tokens WHERE token = $1")
            .bind(new_refresh_token)
            .fetch_one(&test_app.pool)
            .await
            .expect("query should succeed");

    assert_eq!(new_token_count.0, 1, "new refresh token should be persisted");
}
