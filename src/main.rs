use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Clone)]
struct AppState {
    wallets: Arc<RwLock<HashMap<i64, Wallet>>>,
    next_id: Arc<AtomicI64>,
}

#[derive(Clone, Serialize)]
struct Wallet {
    id: i64,
    owner: String,
    balance: i64,
}

#[derive(Deserialize)]
struct CreateWalletRequest {
    owner: String,
    balance: i64,
}

async fn health() -> &'static str {
    "ok"
}

async fn create_wallet(
    State(state): State<AppState>,
    Json(request): Json<CreateWalletRequest>,
) -> Result<(StatusCode, Json<Wallet>), StatusCode> {
    let owner = request.owner.to_owned();

    if owner.is_empty() || request.balance < 0 {
        return Err(StatusCode::BAD_REQUEST);
    };

    let id = state.next_id.fetch_add(1, Ordering::Relaxed);

    let wallet = Wallet {
        id,
        owner,
        balance: request.balance,
    };

    let mut wallet_storage = state.wallets.write().await;
    wallet_storage.insert(id, wallet.clone());

    Ok((StatusCode::OK, Json(wallet)))
}

async fn get_wallet(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Wallet>, StatusCode> {
    let wattet_storage = state.wallets.read().await;

    let Some(wallet) = wattet_storage.get(&id).cloned() else {
        return Err(StatusCode::NOT_FOUND);
    };

    Ok(Json(wallet))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let state = AppState {
        wallets: Arc::new(RwLock::new(HashMap::new())),
        next_id: Arc::new(AtomicI64::new(1)),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/wallets", post(create_wallet))
        .route("/wallets/{id}", get(get_wallet))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    axum::serve(listener, app).await
}
