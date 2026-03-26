use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Multipart, Path as AxumPath, State},
    handler::Handler,
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get},
};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    common::{ApiError, AppConfig, AppState},
};

const MAX_FILE_SIZE_BYTES: usize = 50 * 1024 * 1024;
const ALLOWED_EXTENSIONS: &[&str] = &["pdf", "docx", "txt"];

#[derive(Debug, Serialize, FromRow)]
pub struct DocumentRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub file: String,
}

#[derive(Debug, Serialize)]
pub struct DocumentResponse {
    pub id: Uuid,
    pub title: String,
    pub file: String,
}

#[derive(Debug, Serialize)]
pub struct UploadDocumentResponse {
    pub id: Uuid,
    pub title: String,
    pub file: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteDocumentResponse {
    pub ok: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/documents",
            get(list_my_documents)
                .post(upload_document.layer(DefaultBodyLimit::max(MAX_FILE_SIZE_BYTES))),
        )
        .route("/documents/{id}", delete(delete_document))
}

pub async fn list_my_documents(
    State(pool): State<PgPool>,
    user: AuthUser,
) -> Result<Json<Vec<DocumentResponse>>, ApiError> {
    let docs = sqlx::query_as::<_, DocumentRow>(
        r#"
        SELECT id, user_id, title, file
        FROM documents
        WHERE user_id = $1
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .bind(user.user_id)
    .fetch_all(&pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let response = docs
        .into_iter()
        .map(|doc| DocumentResponse {
            id: doc.id,
            title: doc.title,
            file: doc.file,
        })
        .collect();

    Ok(Json(response))
}

pub async fn upload_document(
    State(state): State<AppState>,
    user: AuthUser,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadDocumentResponse>), ApiError> {
    let documents_dir = documents_dir_from_config(&state.config)?;
    ensure_documents_dir_exists(&documents_dir).await?;

    let saved_document = if let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::BadRequest("invalid multipart form data"))?
    {
        let file_name = field
            .file_name()
            .map(str::to_string)
            .ok_or(ApiError::BadRequest("missing uploaded file name"))?;

        let sanitized_original_name = sanitize_file_name(&file_name);
        if sanitized_original_name.is_empty() {
            return Err(ApiError::BadRequest("invalid uploaded file name"));
        }

        let extension = extract_allowed_extension(&sanitized_original_name)?;
        let randomized_name = format!("{}.{}", Uuid::new_v4(), extension);
        let destination_path = documents_dir.join(&randomized_name);

        let bytes = field
            .bytes()
            .await
            .map_err(|_| ApiError::BadRequest("failed to read uploaded file"))?;

        validate_file_size(&bytes)?;
        write_uploaded_file(&destination_path, &bytes).await?;

        let document_id = Uuid::new_v4();

        let insert_result = sqlx::query(
            r#"
            INSERT INTO documents (id, user_id, title, file)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(document_id)
        .bind(user.user_id)
        .bind(&sanitized_original_name)
        .bind(&randomized_name)
        .execute(&state.pool)
        .await;

        match insert_result {
            Ok(_) => Some(UploadDocumentResponse {
                id: document_id,
                title: sanitized_original_name,
                file: randomized_name,
            }),
            Err(_) => {
                let _ = tokio::fs::remove_file(&destination_path).await;
                return Err(ApiError::Internal);
            }
        }
    } else {
        None
    };

    let saved_document = saved_document.ok_or(ApiError::BadRequest("no file provided"))?;
    Ok((StatusCode::CREATED, Json(saved_document)))
}

pub async fn delete_document(
    State(state): State<AppState>,
    user: AuthUser,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<DeleteDocumentResponse>, ApiError> {
    let row = sqlx::query_as::<_, DocumentRow>(
        r#"
        SELECT id, user_id, title, file
        FROM documents
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(id)
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let document = row.ok_or(ApiError::BadRequest("document not found"))?;

    sqlx::query(
        r#"
        DELETE FROM documents
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(document.id)
    .bind(user.user_id)
    .execute(&state.pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    if let Ok(documents_dir) = documents_dir_from_config(&state.config) {
        let file_path = documents_dir.join(document.file);
        let _ = tokio::fs::remove_file(file_path).await;
    }

    Ok(Json(DeleteDocumentResponse { ok: true }))
}

fn documents_dir_from_config(config: &Arc<AppConfig>) -> Result<PathBuf, ApiError> {
    Ok(PathBuf::from(config.documents_dir.clone()))
}

async fn ensure_documents_dir_exists(path: &Path) -> Result<(), ApiError> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|_| ApiError::Internal)
}

fn sanitize_file_name(file_name: &str) -> String {
    Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn extract_allowed_extension(file_name: &str) -> Result<String, ApiError> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .ok_or(ApiError::BadRequest("file type is required"))?;

    if !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
        return Err(ApiError::BadRequest(
            "unsupported file type, allowed: pdf, docx, txt",
        ));
    }

    Ok(extension)
}

fn validate_file_size(bytes: &Bytes) -> Result<(), ApiError> {
    if bytes.len() > MAX_FILE_SIZE_BYTES {
        return Err(ApiError::BadRequest("file exceeds 50 MB limit"));
    }

    if bytes.is_empty() {
        return Err(ApiError::BadRequest("uploaded file is empty"));
    }

    Ok(())
}

async fn write_uploaded_file(path: &Path, bytes: &Bytes) -> Result<(), ApiError> {
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|_| ApiError::Internal)?;

    file.write_all(bytes)
        .await
        .map_err(|_| ApiError::Internal)?;
    file.flush().await.map_err(|_| ApiError::Internal)?;

    Ok(())
}

impl IntoResponse for DeleteDocumentResponse {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}
