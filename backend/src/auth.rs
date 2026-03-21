use std::sync::Arc;

use axum::{
    Json,
    extract::{FromRef, FromRequestParts, State},
    http::request::Parts,
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::common::{ApiError, AppConfig};

#[derive(Debug, Serialize, Deserialize)]
pub struct AccessClaims {
    // Standard-ish fields
    sub: String, // user id
    exp: i64,    // unix timestamp
    iat: i64,

    // App fields
    typ: String, // "access"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefreshClaims {
    sub: String, // user id
    exp: i64,
    iat: i64,

    typ: String, // "refresh"
    jti: String, // unique token id (can be used for revocation later)
}

#[derive(Debug, Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
pub struct TokenPair {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in_seconds: i64,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    refresh_token: String,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct UserRow {
    id: Uuid,
    username: String,
    password_hash: String,
}

#[derive(Debug, sqlx::FromRow)]
struct RefreshTokenRow {
    token: String,
    expires_at: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub user_id: Uuid,
}

impl<S> FromRequestParts<S> for AuthUser
where
    Arc<AppConfig>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let cfg: Arc<AppConfig> = Arc::from_ref(state);

        let auth_header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;

        // Expect: "Bearer <token>"
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(ApiError::Unauthorized)?;

        let claims = decode_access_token(&cfg, token)?;
        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?;

        Ok(Self { user_id })
    }
}

fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn hash_password(password: &str) -> Result<String, ApiError> {
    use argon2::password_hash::{SaltString, rand_core::OsRng};
    use argon2::{Argon2, PasswordHasher};

    if password.len() < 8 {
        return Err(ApiError::BadRequest(
            "password must be at least 8 characters",
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|ph| ph.to_string())
        .map_err(|_| ApiError::Internal)
}

fn verify_password(password: &str, password_hash: &str) -> Result<bool, ApiError> {
    use argon2::password_hash::PasswordHash;
    use argon2::{Argon2, PasswordVerifier};

    let parsed = PasswordHash::new(password_hash).map_err(|_| ApiError::Internal)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

fn mint_access_token(cfg: &AppConfig, user_id: Uuid) -> Result<String, ApiError> {
    let iat = now_unix();
    let exp = (OffsetDateTime::now_utc() + cfg.access_ttl).unix_timestamp();

    let claims = AccessClaims {
        sub: user_id.to_string(),
        iat,
        exp,
        typ: "access".to_string(),
    };

    jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(cfg.jwt_access_secret.as_bytes()),
    )
    .map_err(|_| ApiError::Internal)
}

fn mint_refresh_token(cfg: &AppConfig, user_id: Uuid) -> Result<String, ApiError> {
    let iat = now_unix();
    let exp = (OffsetDateTime::now_utc() + cfg.refresh_ttl).unix_timestamp();

    let claims = RefreshClaims {
        sub: user_id.to_string(),
        iat,
        exp,
        typ: "refresh".to_string(),
        jti: Uuid::new_v4().to_string(),
    };

    jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(cfg.jwt_refresh_secret.as_bytes()),
    )
    .map_err(|_| ApiError::Internal)
}

fn decode_access_token(cfg: &AppConfig, token: &str) -> Result<AccessClaims, ApiError> {
    let mut validation = Validation::default();
    validation.validate_exp = true;

    let data = jsonwebtoken::decode::<AccessClaims>(
        token,
        &DecodingKey::from_secret(cfg.jwt_access_secret.as_bytes()),
        &validation,
    )
    .map_err(|_| ApiError::Unauthorized)?;

    if data.claims.typ != "access" {
        return Err(ApiError::Unauthorized);
    }

    Ok(data.claims)
}

fn decode_refresh_token(cfg: &AppConfig, token: &str) -> Result<RefreshClaims, ApiError> {
    let mut validation = Validation::default();
    validation.validate_exp = true;

    let data = jsonwebtoken::decode::<RefreshClaims>(
        token,
        &DecodingKey::from_secret(cfg.jwt_refresh_secret.as_bytes()),
        &validation,
    )
    .map_err(|_| ApiError::Unauthorized)?;

    if data.claims.typ != "refresh" {
        return Err(ApiError::Unauthorized);
    }

    Ok(data.claims)
}

async fn db_find_user_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<UserRow>, ApiError> {
    sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, username, password_hash
        FROM users
        WHERE username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

async fn db_insert_user(
    pool: &PgPool,
    username: &str,
    password_hash: &str,
) -> Result<UserRow, ApiError> {
    sqlx::query_as::<_, UserRow>(
        r#"
        INSERT INTO users (id, username, password_hash)
        VALUES ($1, $2, $3)
        RETURNING id, username, password_hash
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(username)
    .bind(password_hash)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        // Handle unique violation in a driver-agnostic way by string matching.
        // If you want a more robust approach: match on sqlx::Error::Database and check constraint codes.
        let msg = e.to_string().to_lowercase();
        if msg.contains("unique") || msg.contains("duplicate") {
            ApiError::Conflict("username already exists")
        } else {
            ApiError::Internal
        }
    })
}

async fn db_upsert_refresh_token(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    token: &str,
    expires_at: OffsetDateTime,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        INSERT INTO refresh_tokens (user_id, token, expires_at)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id)
        DO UPDATE SET
            token = EXCLUDED.token,
            expires_at = EXCLUDED.expires_at
        "#,
    )
    .bind(user_id)
    .bind(token)
    .bind(expires_at)
    .execute(tx.as_mut())
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

async fn db_insert_or_rotate_refresh_token(
    pool: &PgPool,
    user_id: Uuid,
    token: &str,
    expires_at: OffsetDateTime,
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    db_upsert_refresh_token(&mut tx, user_id, token, expires_at).await?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(())
}

async fn db_find_refresh_token(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<RefreshTokenRow>, ApiError> {
    sqlx::query_as::<_, RefreshTokenRow>(
        r#"
        SELECT token, expires_at
        FROM refresh_tokens
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn signup(
    State(pool): State<PgPool>,
    State(cfg): State<Arc<AppConfig>>,
    Json(payload): Json<Credentials>,
) -> Result<Json<TokenPair>, ApiError> {
    if payload.username.trim().is_empty() {
        return Err(ApiError::BadRequest("username must not be empty"));
    }

    let existing = db_find_user_by_username(&pool, &payload.username).await?;
    if existing.is_some() {
        return Err(ApiError::Conflict("username already exists"));
    }

    let password_hash = hash_password(&payload.password)?;
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let user = sqlx::query_as::<_, UserRow>(
        r#"
        INSERT INTO users (id, username, password_hash)
        VALUES ($1, $2, $3)
        RETURNING id, username, password_hash
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(&payload.username)
    .bind(&password_hash)
    .fetch_one(tx.as_mut())
    .await
    .map_err(|e| {
        let msg = e.to_string().to_lowercase();
        if msg.contains("unique") || msg.contains("duplicate") {
            ApiError::Conflict("username already exists")
        } else {
            ApiError::Internal
        }
    })?;

    let access_token = mint_access_token(&cfg, user.id)?;
    let refresh_token = mint_refresh_token(&cfg, user.id)?;
    let refresh_claims = decode_refresh_token(&cfg, &refresh_token)?;
    let refresh_expires_at =
        OffsetDateTime::from_unix_timestamp(refresh_claims.exp).map_err(|_| ApiError::Internal)?;

    db_upsert_refresh_token(&mut tx, user.id, &refresh_token, refresh_expires_at).await?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(Json(TokenPair {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in_seconds: cfg.access_ttl.whole_seconds(),
    }))
}

pub async fn login(
    State(pool): State<PgPool>,
    State(cfg): State<Arc<AppConfig>>,
    Json(payload): Json<Credentials>,
) -> Result<Json<TokenPair>, ApiError> {
    let user = db_find_user_by_username(&pool, &payload.username)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    let ok = verify_password(&payload.password, &user.password_hash)?;
    if !ok {
        return Err(ApiError::Unauthorized);
    }

    let access_token = mint_access_token(&cfg, user.id)?;
    let refresh_token = mint_refresh_token(&cfg, user.id)?;
    let refresh_claims = decode_refresh_token(&cfg, &refresh_token)?;
    let refresh_expires_at =
        OffsetDateTime::from_unix_timestamp(refresh_claims.exp).map_err(|_| ApiError::Internal)?;

    db_insert_or_rotate_refresh_token(&pool, user.id, &refresh_token, refresh_expires_at).await?;

    Ok(Json(TokenPair {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in_seconds: cfg.access_ttl.whole_seconds(),
    }))
}

pub async fn refresh_token(
    State(pool): State<PgPool>,
    State(cfg): State<Arc<AppConfig>>,
    Json(payload): Json<RefreshRequest>,
) -> Result<Json<TokenPair>, ApiError> {
    let claims = decode_refresh_token(&cfg, &payload.refresh_token)?;
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?;

    let stored_refresh_token = db_find_refresh_token(&pool, user_id)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    if stored_refresh_token.token != payload.refresh_token {
        return Err(ApiError::Unauthorized);
    }

    if stored_refresh_token.expires_at < OffsetDateTime::now_utc() {
        return Err(ApiError::Unauthorized);
    }

    let access_token = mint_access_token(&cfg, user_id)?;
    let refresh_token = mint_refresh_token(&cfg, user_id)?;
    let new_refresh_claims = decode_refresh_token(&cfg, &refresh_token)?;
    let new_refresh_expires_at = OffsetDateTime::from_unix_timestamp(new_refresh_claims.exp)
        .map_err(|_| ApiError::Internal)?;

    db_insert_or_rotate_refresh_token(&pool, user_id, &refresh_token, new_refresh_expires_at)
        .await?;

    Ok(Json(TokenPair {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in_seconds: cfg.access_ttl.whole_seconds(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::AppConfig;
    use axum::{body::Body, extract::FromRequestParts, http::Request};
    use std::sync::Arc;
    use time::Duration;

    fn test_config() -> AppConfig {
        AppConfig {
            jwt_access_secret: "access-secret-for-tests".to_string(),
            jwt_refresh_secret: "refresh-secret-for-tests".to_string(),
            access_ttl: Duration::minutes(5),
            refresh_ttl: Duration::days(30),
        }
    }

    #[test]
    fn hash_password_rejects_short_passwords() {
        let result = hash_password("short");

        assert!(matches!(
            result,
            Err(ApiError::BadRequest(
                "password must be at least 8 characters"
            ))
        ));
    }

    #[test]
    fn hash_and_verify_password_round_trip() {
        let password = "very-secure-password";
        let hash = hash_password(password).expect("password should hash successfully");

        assert_ne!(hash, password);
        assert!(
            verify_password(password, &hash).expect("password verification should succeed"),
            "expected original password to verify"
        );
        assert!(
            !verify_password("wrong-password", &hash)
                .expect("password verification should succeed"),
            "expected wrong password to fail verification"
        );
    }

    #[test]
    fn verify_password_returns_internal_for_invalid_hash() {
        let result = verify_password("password123", "not-a-valid-argon2-hash");

        assert!(matches!(result, Err(ApiError::Internal)));
    }

    #[test]
    fn mint_and_decode_access_token_round_trip() {
        let cfg = test_config();
        let user_id = Uuid::new_v4();

        let token = mint_access_token(&cfg, user_id).expect("access token should be minted");
        let claims = decode_access_token(&cfg, &token).expect("access token should decode");

        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.typ, "access");
        assert!(claims.exp >= claims.iat);
    }

    #[test]
    fn mint_and_decode_refresh_token_round_trip() {
        let cfg = test_config();
        let user_id = Uuid::new_v4();

        let token = mint_refresh_token(&cfg, user_id).expect("refresh token should be minted");
        let claims = decode_refresh_token(&cfg, &token).expect("refresh token should decode");

        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.typ, "refresh");
        assert!(!claims.jti.is_empty());
        assert!(claims.exp >= claims.iat);
    }

    #[test]
    fn decode_access_token_rejects_refresh_token() {
        let cfg = test_config();
        let user_id = Uuid::new_v4();

        let refresh_token =
            mint_refresh_token(&cfg, user_id).expect("refresh token should be minted");

        let result = decode_access_token(&cfg, &refresh_token);

        assert!(matches!(result, Err(ApiError::Unauthorized)));
    }

    #[test]
    fn decode_refresh_token_rejects_access_token() {
        let cfg = test_config();
        let user_id = Uuid::new_v4();

        let access_token = mint_access_token(&cfg, user_id).expect("access token should be minted");

        let result = decode_refresh_token(&cfg, &access_token);

        assert!(matches!(result, Err(ApiError::Unauthorized)));
    }

    #[tokio::test]
    async fn auth_user_extractor_reads_valid_bearer_token() {
        let cfg = Arc::new(test_config());
        let user_id = Uuid::new_v4();
        let token = mint_access_token(&cfg, user_id).expect("access token should be minted");

        let request = Request::builder()
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .expect("request should build");
        let (mut parts, _) = request.into_parts();

        let auth_user = AuthUser::from_request_parts(&mut parts, &cfg)
            .await
            .expect("extractor should succeed");

        assert_eq!(auth_user.user_id, user_id);
    }

    #[tokio::test]
    async fn auth_user_extractor_rejects_missing_header() {
        let cfg = Arc::new(test_config());
        let request = Request::builder()
            .body(Body::empty())
            .expect("request should build");
        let (mut parts, _) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &cfg).await;

        assert!(matches!(result, Err(ApiError::Unauthorized)));
    }

    #[tokio::test]
    async fn auth_user_extractor_rejects_non_bearer_header() {
        let cfg = Arc::new(test_config());
        let request = Request::builder()
            .header("authorization", "Basic abc123")
            .body(Body::empty())
            .expect("request should build");
        let (mut parts, _) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &cfg).await;

        assert!(matches!(result, Err(ApiError::Unauthorized)));
    }

    #[tokio::test]
    async fn auth_user_extractor_rejects_token_signed_with_wrong_secret() {
        let valid_cfg = Arc::new(test_config());
        let other_cfg = AppConfig {
            jwt_access_secret: "different-access-secret".to_string(),
            jwt_refresh_secret: valid_cfg.jwt_refresh_secret.clone(),
            access_ttl: valid_cfg.access_ttl,
            refresh_ttl: valid_cfg.refresh_ttl,
        };
        let user_id = Uuid::new_v4();
        let token = mint_access_token(&other_cfg, user_id).expect("access token should be minted");

        let request = Request::builder()
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .expect("request should build");
        let (mut parts, _) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &valid_cfg).await;

        assert!(matches!(result, Err(ApiError::Unauthorized)));
    }
}
