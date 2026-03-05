use axum::{Json, Router, routing::get};
use serde::Serialize;
use std::net::SocketAddr;

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

#[tokio::main]
async fn main() {
    let app = Router::new().route("/api/get_settings", get(get_settings));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
