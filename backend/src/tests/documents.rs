use std::{path::Path, time::Duration};

use axum::http::StatusCode;
use serde_json::json;
use sqlx::Row;
use tokio::time::sleep;
use uuid::Uuid;

use super::{auth_header_for, multipart_body, response_json, send_json, send_multipart, test_app};

#[tokio::test]
async fn list_documents_requires_authentication() {
    let test_app = test_app().await;

    let response = send_json(test_app.app.clone(), "GET", "/api/documents", json!(null)).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = response_json(response).await;
    assert_eq!(body["error"], "unauthorized");
    assert_eq!(body["message"], "unauthorized");
}

#[tokio::test]
async fn upload_document_requires_authentication() {
    let test_app = test_app().await;
    let boundary = format!("boundary-{}", Uuid::new_v4());
    let body = multipart_body(
        "file",
        "notes.txt",
        "text/plain",
        b"hello from test",
        &boundary,
    );

    let response = send_multipart(
        test_app.app.clone(),
        "POST",
        "/api/documents",
        body,
        &boundary,
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = response_json(response).await;
    assert_eq!(body["error"], "unauthorized");
    assert_eq!(body["message"], "unauthorized");
}

#[tokio::test]
async fn upload_document_persists_row_file_processed_flag_and_embeddings() {
    let test_app = test_app().await;
    let username = format!("docs-upload-{}", Uuid::new_v4());
    let auth_header = auth_header_for(test_app.app.clone(), username, "very-secure-password").await;

    let boundary = format!("boundary-{}", Uuid::new_v4());
    let file_name = "report.txt";
    let contents = b"document contents for upload integration test";
    let body = multipart_body("file", file_name, "text/plain", contents, &boundary);

    let response = send_multipart(
        test_app.app.clone(),
        "POST",
        "/api/documents",
        body,
        &boundary,
        Some(&auth_header),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let json = response_json(response).await;
    let document_id = Uuid::parse_str(json["id"].as_str().expect("document id should be returned"))
        .expect("document id should be valid uuid");
    let stored_title = json["title"]
        .as_str()
        .expect("document title should be returned")
        .to_string();
    let stored_file = json["file"]
        .as_str()
        .expect("stored file name should be returned")
        .to_string();

    assert_eq!(stored_title, file_name);
    assert_ne!(stored_file, file_name);
    assert!(stored_file.ends_with(".txt"));

    let db_row = sqlx::query("SELECT title, file, processed FROM documents WHERE id = $1")
        .bind(document_id)
        .fetch_one(&test_app.pool)
        .await
        .expect("document row should exist");

    let stored_title_in_db: String = db_row.get("title");
    let stored_file_in_db: String = db_row.get("file");
    let processed: bool = db_row.get("processed");

    assert_eq!(stored_title_in_db, file_name);
    assert_eq!(stored_file_in_db, stored_file);
    assert!(
        processed,
        "document should be marked as processed after upload"
    );

    let stored_path = Path::new(&test_app.config.documents_dir).join(&stored_file);
    let stored_bytes = tokio::fs::read(&stored_path)
        .await
        .expect("uploaded file should be stored on disk");

    assert_eq!(stored_bytes, contents);

    let chunk_rows = sqlx::query(
        "SELECT text_content, vector_dims(embedding) AS embedding_dims FROM document_chunks WHERE document_id = $1 ORDER BY created_at ASC, id ASC",
    )
    .bind(document_id)
    .fetch_all(&test_app.pool)
    .await
    .expect("document chunks should be queryable");

    assert_eq!(
        chunk_rows.len(),
        1,
        "short text file should produce one chunk"
    );

    let first_chunk_text: String = chunk_rows[0].get("text_content");
    let embedding_dims: i32 = chunk_rows[0].get("embedding_dims");

    assert_eq!(
        first_chunk_text,
        "document contents for upload integration test"
    );
    assert_eq!(
        embedding_dims, 1024,
        "embedding should have 1024 dimensions"
    );
}

#[tokio::test]
async fn upload_document_splits_long_text_into_overlapping_embedded_chunks() {
    let test_app = test_app().await;
    let username = format!("docs-chunks-{}", Uuid::new_v4());
    let auth_header = auth_header_for(test_app.app.clone(), username, "very-secure-password").await;

    let boundary = format!("boundary-{}", Uuid::new_v4());
    let file_name = "long-report.txt";
    let long_text = "A".repeat(950);
    let body = multipart_body(
        "file",
        file_name,
        "text/plain",
        long_text.as_bytes(),
        &boundary,
    );

    let response = send_multipart(
        test_app.app.clone(),
        "POST",
        "/api/documents",
        body,
        &boundary,
        Some(&auth_header),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let json = response_json(response).await;
    let document_id = Uuid::parse_str(json["id"].as_str().expect("document id should be returned"))
        .expect("document id should be valid uuid");

    let chunk_rows = sqlx::query(
        "SELECT text_content, vector_dims(embedding) AS embedding_dims FROM document_chunks WHERE document_id = $1 ORDER BY created_at ASC, id ASC",
    )
    .bind(document_id)
    .fetch_all(&test_app.pool)
    .await
    .expect("document chunks should be queryable");

    assert_eq!(
        chunk_rows.len(),
        2,
        "950 chars should split into two chunks with 500/50 settings"
    );

    let first_chunk: String = chunk_rows[0].get("text_content");
    let second_chunk: String = chunk_rows[1].get("text_content");
    let first_dims: i32 = chunk_rows[0].get("embedding_dims");
    let second_dims: i32 = chunk_rows[1].get("embedding_dims");

    assert_eq!(first_chunk.chars().count(), 500);
    assert_eq!(second_chunk.chars().count(), 500);
    assert_eq!(first_dims, 1024);
    assert_eq!(second_dims, 1024);

    let first_suffix: String = first_chunk
        .chars()
        .skip(first_chunk.chars().count() - 50)
        .collect();
    let second_prefix: String = second_chunk.chars().take(50).collect();

    assert_eq!(
        first_suffix, second_prefix,
        "adjacent chunks should overlap by 50 chars"
    );

    let processed: bool = sqlx::query_scalar("SELECT processed FROM documents WHERE id = $1")
        .bind(document_id)
        .fetch_one(&test_app.pool)
        .await
        .expect("document processed flag should be queryable");

    assert!(
        processed,
        "document should be marked as processed after chunk embeddings are stored"
    );
}

#[tokio::test]
async fn list_documents_returns_only_current_users_documents() {
    let test_app = test_app().await;

    let user_one_auth = auth_header_for(
        test_app.app.clone(),
        format!("docs-user-one-{}", Uuid::new_v4()),
        "very-secure-password",
    )
    .await;
    let user_two_auth = auth_header_for(
        test_app.app.clone(),
        format!("docs-user-two-{}", Uuid::new_v4()),
        "very-secure-password",
    )
    .await;

    let first_boundary = format!("boundary-{}", Uuid::new_v4());
    let first_upload = multipart_body(
        "file",
        "mine.txt",
        "text/plain",
        b"user one document",
        &first_boundary,
    );

    let first_response = send_multipart(
        test_app.app.clone(),
        "POST",
        "/api/documents",
        first_upload,
        &first_boundary,
        Some(&user_one_auth),
    )
    .await;
    assert_eq!(first_response.status(), StatusCode::CREATED);

    let second_boundary = format!("boundary-{}", Uuid::new_v4());
    let second_upload = multipart_body(
        "file",
        "other.txt",
        "text/plain",
        b"user two document",
        &second_boundary,
    );

    let second_response = send_multipart(
        test_app.app.clone(),
        "POST",
        "/api/documents",
        second_upload,
        &second_boundary,
        Some(&user_two_auth),
    )
    .await;
    assert_eq!(second_response.status(), StatusCode::CREATED);

    let list_response = send_json(test_app.app.clone(), "GET", "/api/documents", json!(null)).await;

    assert_eq!(list_response.status(), StatusCode::UNAUTHORIZED);

    let authorized_list_response = {
        let request_body = json!(null);
        let response = send_json(test_app.app.clone(), "GET", "/api/documents", request_body).await;
        response
    };

    assert_eq!(authorized_list_response.status(), StatusCode::UNAUTHORIZED);

    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/api/documents")
        .header(axum::http::header::AUTHORIZATION, user_one_auth)
        .body(axum::body::Body::empty())
        .expect("request should build");

    let response = tower::util::ServiceExt::oneshot(test_app.app.clone(), request)
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);

    let body = response_json(response).await;
    let documents = body
        .as_array()
        .expect("documents response should be an array");

    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0]["title"], "mine.txt");
    assert_eq!(
        documents[0]["file"].as_str().unwrap().ends_with(".txt"),
        true
    );
}

#[tokio::test]
async fn upload_document_rejects_unsupported_extension() {
    let test_app = test_app().await;
    let auth_header = auth_header_for(
        test_app.app.clone(),
        format!("docs-invalid-ext-{}", Uuid::new_v4()),
        "very-secure-password",
    )
    .await;

    let boundary = format!("boundary-{}", Uuid::new_v4());
    let body = multipart_body(
        "file",
        "archive.zip",
        "application/zip",
        b"zip-content",
        &boundary,
    );

    let response = send_multipart(
        test_app.app.clone(),
        "POST",
        "/api/documents",
        body,
        &boundary,
        Some(&auth_header),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response_json(response).await;
    assert_eq!(body["error"], "bad_request");
    assert_eq!(
        body["message"],
        "unsupported file type, allowed: pdf, docx, txt"
    );
}

#[tokio::test]
async fn upload_document_rejects_empty_file() {
    let test_app = test_app().await;
    let auth_header = auth_header_for(
        test_app.app.clone(),
        format!("docs-empty-{}", Uuid::new_v4()),
        "very-secure-password",
    )
    .await;

    let boundary = format!("boundary-{}", Uuid::new_v4());
    let body = multipart_body("file", "empty.txt", "text/plain", b"", &boundary);

    let response = send_multipart(
        test_app.app.clone(),
        "POST",
        "/api/documents",
        body,
        &boundary,
        Some(&auth_header),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response_json(response).await;
    assert_eq!(body["error"], "bad_request");
    assert_eq!(body["message"], "uploaded file is empty");
}

#[tokio::test]
async fn delete_document_removes_database_row_and_file() {
    let test_app = test_app().await;
    let auth_header = auth_header_for(
        test_app.app.clone(),
        format!("docs-delete-{}", Uuid::new_v4()),
        "very-secure-password",
    )
    .await;

    let boundary = format!("boundary-{}", Uuid::new_v4());
    let contents = b"file to be deleted";
    let upload_body = multipart_body("file", "delete-me.txt", "text/plain", contents, &boundary);

    let upload_response = send_multipart(
        test_app.app.clone(),
        "POST",
        "/api/documents",
        upload_body,
        &boundary,
        Some(&auth_header),
    )
    .await;

    assert_eq!(upload_response.status(), StatusCode::CREATED);

    let upload_json = response_json(upload_response).await;
    let document_id = upload_json["id"].as_str().unwrap().to_string();
    let stored_file = upload_json["file"].as_str().unwrap().to_string();
    let stored_path = Path::new(&test_app.config.documents_dir).join(&stored_file);

    assert!(tokio::fs::try_exists(&stored_path).await.unwrap());

    let delete_request = axum::http::Request::builder()
        .method("DELETE")
        .uri(format!("/api/documents/{document_id}"))
        .header(axum::http::header::AUTHORIZATION, &auth_header)
        .body(axum::body::Body::empty())
        .expect("delete request should build");

    let delete_response = tower::util::ServiceExt::oneshot(test_app.app.clone(), delete_request)
        .await
        .expect("delete request should succeed");

    assert_eq!(delete_response.status(), StatusCode::OK);

    let delete_json = response_json(delete_response).await;
    assert_eq!(delete_json["ok"], true);

    let remaining: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE id = $1")
        .bind(Uuid::parse_str(&document_id).unwrap())
        .fetch_one(&test_app.pool)
        .await
        .expect("document count query should succeed");

    assert_eq!(remaining.0, 0);

    for _ in 0..10 {
        if !tokio::fs::try_exists(&stored_path)
            .await
            .expect("file existence check should succeed")
        {
            return;
        }

        sleep(Duration::from_millis(20)).await;
    }

    panic!("document file should be removed from disk");
}

#[tokio::test]
async fn delete_document_rejects_deleting_other_users_document() {
    let test_app = test_app().await;

    let owner_auth = auth_header_for(
        test_app.app.clone(),
        format!("docs-owner-{}", Uuid::new_v4()),
        "very-secure-password",
    )
    .await;
    let other_auth = auth_header_for(
        test_app.app.clone(),
        format!("docs-other-{}", Uuid::new_v4()),
        "very-secure-password",
    )
    .await;

    let boundary = format!("boundary-{}", Uuid::new_v4());
    let upload_body = multipart_body(
        "file",
        "private.txt",
        "text/plain",
        b"owner document",
        &boundary,
    );

    let upload_response = send_multipart(
        test_app.app.clone(),
        "POST",
        "/api/documents",
        upload_body,
        &boundary,
        Some(&owner_auth),
    )
    .await;

    assert_eq!(upload_response.status(), StatusCode::CREATED);

    let upload_json = response_json(upload_response).await;
    let document_id = upload_json["id"].as_str().unwrap().to_string();

    let delete_request = axum::http::Request::builder()
        .method("DELETE")
        .uri(format!("/api/documents/{document_id}"))
        .header(axum::http::header::AUTHORIZATION, &other_auth)
        .body(axum::body::Body::empty())
        .expect("delete request should build");

    let delete_response = tower::util::ServiceExt::oneshot(test_app.app.clone(), delete_request)
        .await
        .expect("delete request should succeed");

    assert_eq!(delete_response.status(), StatusCode::BAD_REQUEST);

    let delete_json = response_json(delete_response).await;
    assert_eq!(delete_json["error"], "bad_request");
    assert_eq!(delete_json["message"], "document not found");

    let remaining: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE id = $1")
        .bind(Uuid::parse_str(&document_id).unwrap())
        .fetch_one(&test_app.pool)
        .await
        .expect("document should still exist");

    assert_eq!(remaining.0, 1);
}
