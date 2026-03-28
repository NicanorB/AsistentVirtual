use std::convert::Infallible;

use async_openai::{
    Client as OpenAiClient,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
        CreateChatCompletionStreamResponse,
    },
};
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
use tracing::info;

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

pub fn router() -> Router<AppState> {
    Router::new().route("/chat/query", post(query_chat))
}

pub async fn query_chat(
    State(state): State<AppState>,
    user: AuthUser,
    JsonExtractor(payload): JsonExtractor<ChatQueryRequest>,
) -> Result<Response, ApiError> {
    info!("Received query: {}", payload.query);
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
    let llama_stream = request_llama_stream(
        state.config.completions_host.as_deref(),
        state.config.openai_api_key.as_deref(),
        &query,
        prompt,
    )
    .await?;

    tokio::pin!(llama_stream);

    while let Some(chunk_result) = llama_stream.next().await {
        let chunk = match chunk_result {
            Ok(chunk) => chunk,
            Err(_) => return Err(ApiError::Internal),
        };

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

fn build_prompt(_query: &str, sources: &[SourceItem]) -> String {
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
If the context is insufficient, say so clearly and still try to be helpful. Do not directly quote the given context.\n\n\
Context:\n{}\n",
        context
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
    host: Option<&str>,
    api_key: Option<&str>,
    query: &str,
    prompt: String,
) -> Result<impl Stream<Item = Result<String, ApiError>>, ApiError> {
    let mut config = OpenAIConfig::new();

    if let Some(host) = host {
        config = config.with_api_base(host);
    }
    if let Some(api_key) = api_key {
        config = config.with_api_key(api_key);
    }

    let client = OpenAiClient::with_config(config);

    let messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestSystemMessageArgs::default()
            .content(prompt)
            .build()
            .map_err(|_| ApiError::Internal)?
            .into(),
        ChatCompletionRequestUserMessageArgs::default()
            .content(query)
            .build()
            .map_err(|_| ApiError::Internal)?
            .into(),
    ];

    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-5.4-mini")
        .max_tokens(512u32)
        .stream(true)
        .messages(messages)
        .build()
        .map_err(|_| ApiError::Internal)?;

    let stream = client
        .chat()
        .create_stream(request)
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(stream.map(|item| match item {
        Ok(chunk) => extract_chat_stream_content(&chunk),
        Err(_) => Err(ApiError::Internal),
    }))
}

fn extract_chat_stream_content(
    chunk: &CreateChatCompletionStreamResponse,
) -> Result<String, ApiError> {
    Ok(chunk
        .choices
        .iter()
        .filter_map(|choice| choice.delta.content.clone())
        .collect::<String>())
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
