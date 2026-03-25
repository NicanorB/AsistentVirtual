use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

use super::{response_json, send_json, test_app};

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
