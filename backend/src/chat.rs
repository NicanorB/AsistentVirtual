use std::convert::Infallible;

use axum::{
    Json as JsonExtractor, Router,
    extract::State,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::post,
};
use futures_util::{Stream, StreamExt};
use pgvector::Vector;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    auth::AuthUser,
    common::{ApiError, AppState},
};

const TOP_K_MATCHES: i64 = 3;
const EXPECTED_EMBEDDING_SIZE: usize = 1024;
const MAX_CONTEXT_SNIPPET_LEN: usize = 1_200;

#[derive(Debug, Deserialize)]
pub struct ChatQueryRequest {
    pub query: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct EmbeddingRequest {
    content: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    index: u32,
    embedding: Vec<Vec<f32>>,
}

#[derive(Debug, FromRow)]
struct RetrievedChunkRow {
    document: String,
    text_snippet: String,
}

#[derive(Debug, Serialize, Clone)]
struct SourceItem {
    document: String,
    text_snippet: String,
}

#[derive(Debug, Serialize)]
struct StreamChunk {
    content: String,
    stop: bool,
}

#[derive(Debug, Serialize)]
struct StreamDone {
    content: String,
    stop: bool,
    sources: Vec<SourceItem>,
}

#[derive(Debug, Deserialize)]
struct LlamaCompletionChunk {
    content: Option<String>,
    stop: Option<bool>,
}

#[derive(Debug, Serialize)]
struct LlamaCompletionRequest {
    prompt: String,
    stream: bool,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/chat/query", post(query_chat))
}

pub async fn query_chat(
    State(state): State<AppState>,
    user: AuthUser,
    JsonExtractor(payload): JsonExtractor<ChatQueryRequest>,
) -> Result<Response, ApiError> {
    let query = payload.query.trim().to_string();
    if query.is_empty() {
        return Err(ApiError::BadRequest("query must not be empty"));
    }

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);
    let state_for_task = state.clone();

    tokio::spawn(async move {
        let client = Client::new();

        let task_result = handle_chat_query(state_for_task, user, query, client, tx.clone()).await;
        if task_result.is_err() {
            let fallback = StreamDone {
                content: String::new(),
                stop: true,
                sources: Vec::new(),
            };

            let _ = tx.send(Ok(json_event(&fallback))).await;
        }
    });

    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}

async fn handle_chat_query(
    state: AppState,
    user: AuthUser,
    query: String,
    client: Client,
    tx: mpsc::Sender<Result<Event, Infallible>>,
) -> Result<(), ApiError> {
    let query_embedding = fetch_embedding(&client, &state.config.embeddings_host, &query).await?;
    if query_embedding.len() != EXPECTED_EMBEDDING_SIZE {
        return Err(ApiError::Internal);
    }

    let retrieved_chunks = fetch_similar_chunks(&state.pool, user.user_id, query_embedding).await?;
    let sources: Vec<SourceItem> = retrieved_chunks
        .iter()
        .map(|row| SourceItem {
            document: row.document.clone(),
            text_snippet: row.text_snippet.clone(),
        })
        .collect();

    let prompt = build_prompt(&query, &sources);
    let llama_stream =
        request_llama_stream(&client, &state.config.completions_host, prompt).await?;

    tokio::pin!(llama_stream);

    while let Some(chunk_result) = llama_stream.next().await {
        let chunk = match chunk_result {
            Ok(chunk) => chunk,
            Err(_) => return Err(ApiError::Internal),
        };

        if chunk.trim().is_empty() {
            continue;
        }

        let event = StreamChunk {
            content: chunk,
            stop: false,
        };

        if tx.send(Ok(json_event(&event))).await.is_err() {
            return Ok(());
        }
    }

    let done = StreamDone {
        content: String::new(),
        stop: true,
        sources,
    };

    let _ = tx.send(Ok(json_event(&done))).await;
    Ok(())
}

async fn fetch_similar_chunks(
    pool: &PgPool,
    user_id: uuid::Uuid,
    embedding: Vec<f32>,
) -> Result<Vec<RetrievedChunkRow>, ApiError> {
    sqlx::query_as::<_, RetrievedChunkRow>(
        r#"
        SELECT
            d.title AS document,
            dc.text_content AS text_snippet
        FROM document_chunks dc
        INNER JOIN documents d ON d.id = dc.document_id
        WHERE d.user_id = $1
          AND d.processed = true
        ORDER BY dc.embedding <-> $2
        LIMIT $3
        "#,
    )
    .bind(user_id)
    .bind(Vector::from(embedding))
    .bind(TOP_K_MATCHES)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

fn build_prompt(query: &str, sources: &[SourceItem]) -> String {
    let context = if sources.is_empty() {
        "No relevant document context was found for this user.".to_string()
    } else {
        sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                format!(
                    "Source {} - Document: {}\n{}\n",
                    index + 1,
                    source.document,
                    truncate_text(&source.text_snippet, MAX_CONTEXT_SNIPPET_LEN)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "You are a helpful assistant. Answer the user's question using the provided context when relevant. \
If the context is insufficient, say so clearly and still try to be helpful. Do not invent citations.\n\n\
Context:\n{}\n\n\
User question:\n{}\n\n\
Answer:",
        context, query
    )
}

async fn fetch_embedding(client: &Client, host: &str, content: &str) -> Result<Vec<f32>, ApiError> {
    let url = format!("{}/embedding", host.trim_end_matches('/'));

    let response = client
        .post(url)
        .json(&EmbeddingRequest {
            content: content.to_string(),
        })
        .send()
        .await
        .map_err(|_| ApiError::Internal)?;

    if !response.status().is_success() {
        return Err(ApiError::Internal);
    }

    let mut embeddings = response
        .json::<Vec<EmbeddingResponse>>()
        .await
        .map_err(|_| ApiError::Internal)?;

    let embedding = embeddings
        .drain(..)
        .find(|item| item.index == 0)
        .ok_or(ApiError::Internal)?;

    embedding
        .embedding
        .into_iter()
        .next()
        .ok_or(ApiError::Internal)
}

async fn request_llama_stream(
    client: &Client,
    host: &str,
    prompt: String,
) -> Result<impl Stream<Item = Result<String, ApiError>>, ApiError> {
    let url = format!("{}/completion", host.trim_end_matches('/'));

    let response = client
        .post(url)
        .json(&LlamaCompletionRequest {
            prompt,
            stream: true,
        })
        .send()
        .await
        .map_err(|_| ApiError::Internal)?;

    if !response.status().is_success() {
        return Err(ApiError::Internal);
    }

    let parsed = response.bytes_stream().map(|item| match item {
        Ok(bytes) => parse_llama_chunk(&bytes),
        Err(_) => vec![Err(ApiError::Internal)],
    });

    Ok(parsed.flat_map(futures_util::stream::iter))
}

fn parse_llama_chunk(bytes: &[u8]) -> Vec<Result<String, ApiError>> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| parse_llama_line(line))
        .collect()
}

fn parse_llama_line(line: &[u8]) -> Result<String, ApiError> {
    let line = std::str::from_utf8(line).map_err(|_| ApiError::Internal)?;
    let trimmed = line.trim();

    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let payload = trimmed.strip_prefix("data: ").unwrap_or(trimmed);
    if payload == "[DONE]" {
        return Ok(String::new());
    }

    let parsed: LlamaCompletionChunk =
        serde_json::from_str(payload).map_err(|_| ApiError::Internal)?;

    if parsed.stop.unwrap_or(false) {
        return Ok(String::new());
    }

    Ok(parsed.content.unwrap_or_default())
}

fn truncate_text(text: &str, max_len: usize) -> String {
    let mut chars = text.chars();

    let truncated: String = chars.by_ref().take(max_len).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn json_event<T: Serialize>(value: &T) -> Event {
    let data = serde_json::to_string(value)
        .unwrap_or_else(|_| "{\"content\":\"\",\"stop\":true,\"sources\":[]}".to_string());

    Event::default().data(data)
}
