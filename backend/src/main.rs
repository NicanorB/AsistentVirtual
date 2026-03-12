use axum::{
    Json, Router,
    extract::{FromRef, FromRequestParts, State},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{net::SocketAddr, sync::Arc};
use time::{Duration, OffsetDateTime};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

//
// Configuration
//

#[derive(Clone)]
struct AppConfig {
    jwt_access_secret: String,
    jwt_refresh_secret: String,
    /// Access token validity: 5 minutes.
    access_ttl: Duration,
    /// Refresh token validity: longer-lived; used to mint new access tokens.
    refresh_ttl: Duration,
}

impl AppConfig {
    fn from_env() -> anyhow::Result<Self> {
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

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    config: Arc<AppConfig>,
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> PgPool {
        state.pool.clone()
    }
}

impl FromRef<AppState> for Arc<AppConfig> {
    fn from_ref(state: &AppState) -> Arc<AppConfig> {
        state.config.clone()
    }
}

//
// Errors
//

#[derive(thiserror::Error, Debug)]
enum ApiError {
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

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    message: String,
}

//
// JWT claims
//

#[derive(Debug, Serialize, Deserialize)]
struct AccessClaims {
    // Standard-ish fields
    sub: String, // user id
    exp: i64,    // unix timestamp
    iat: i64,

    // App fields
    typ: String, // "access"
}

#[derive(Debug, Serialize, Deserialize)]
struct RefreshClaims {
    sub: String, // user id
    exp: i64,
    iat: i64,

    typ: String, // "refresh"
    jti: String, // unique token id (can be used for revocation later)
}

fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
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

//
// Auth extractor (JWT middleware-like via extractor)
//

#[derive(Clone, Debug)]
struct AuthUser {
    user_id: Uuid,
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

//
// Request/Response DTOs
//

#[derive(Debug, Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct TokenPair {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in_seconds: i64,
}

#[derive(Debug, Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Debug, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    password_hash: String,
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

//
// Handlers
//

async fn signup(
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
    let user = db_insert_user(&pool, &payload.username, &password_hash).await?;

    let access_token = mint_access_token(&cfg, user.id)?;
    let refresh_token = mint_refresh_token(&cfg, user.id)?;

    Ok(Json(TokenPair {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in_seconds: cfg.access_ttl.whole_seconds(),
    }))
}

async fn login(
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

    Ok(Json(TokenPair {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in_seconds: cfg.access_ttl.whole_seconds(),
    }))
}

async fn refresh_token(
    State(cfg): State<Arc<AppConfig>>,
    Json(payload): Json<RefreshRequest>,
) -> Result<Json<TokenPair>, ApiError> {
    let claims = decode_refresh_token(&cfg, &payload.refresh_token)?;
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| ApiError::Unauthorized)?;

    // In a production system, you'd typically validate the refresh token against a DB table
    // (rotation, revocation, reuse detection). Here we only validate signature + expiry.
    let access_token = mint_access_token(&cfg, user_id)?;
    let refresh_token = mint_refresh_token(&cfg, user_id)?;

    Ok(Json(TokenPair {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in_seconds: cfg.access_ttl.whole_seconds(),
    }))
}

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
        .route("/signup", post(signup))
        .route("/login", post(login))
        .route("/refresh_token", post(refresh_token));

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
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Default to info if RUST_LOG isn't set
                tracing_subscriber::EnvFilter::new("info")
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL").unwrap();

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
