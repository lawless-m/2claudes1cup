use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::config::Config;
use crate::db::Db;
use crate::mcp::{self, JsonRpcRequest};

pub async fn run(db: Arc<Db>, config: &Config) -> anyhow::Result<()> {
    let addr: SocketAddr = format!(
        "{}:{}",
        config.server.listen_addr, config.server.listen_port
    )
    .parse()?;

    let app = Router::new()
        .route("/mcp", post(handle_post))
        .with_state(db);

    tracing::info!("Listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_post(
    State(db): State<Arc<Db>>,
    Json(request): Json<JsonRpcRequest>,
) -> Response {
    match mcp::handle_request(&db, request) {
        Some(response) => Json(response).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}
