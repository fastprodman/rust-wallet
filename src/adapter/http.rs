use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{Transfer, Wallet},
    service::{
        CreateWallet, TransferError, TransferMoney, TransferOutcome, TransferService, WalletError,
        WalletService,
    },
};

#[derive(Clone)]
struct AppState {
    wallet_service: Arc<WalletService>,
    transfer_service: Arc<TransferService>,
}

#[derive(Deserialize)]
struct CreateWalletRequest {
    owner: String,
    balance: i64,
}

#[derive(Deserialize)]
struct CreateTransferRequest {
    from_wallet_id: i64,
    to_wallet_id: i64,
    amount: i64,
    idempotency_key: String,
}

#[derive(Serialize)]
struct WalletResponse {
    id: i64,
    owner: String,
    balance: i64,
}

impl From<Wallet> for WalletResponse {
    fn from(wallet: Wallet) -> Self {
        Self {
            id: wallet.id,
            owner: wallet.owner,
            balance: wallet.balance,
        }
    }
}

#[derive(Serialize)]
struct TransferResponse {
    id: i64,
    from_wallet_id: i64,
    to_wallet_id: i64,
    amount: i64,
    idempotency_key: String,
    status: String,
}

impl From<Transfer> for TransferResponse {
    fn from(transfer: Transfer) -> Self {
        Self {
            id: transfer.id,
            from_wallet_id: transfer.from_wallet_id,
            to_wallet_id: transfer.to_wallet_id,
            amount: transfer.amount,
            idempotency_key: transfer.idempotency_key,
            status: transfer.status,
        }
    }
}

pub fn router(
    wallet_service: Arc<WalletService>,
    transfer_service: Arc<TransferService>,
) -> Router {
    let state = AppState {
        wallet_service,
        transfer_service,
    };

    Router::new()
        .route("/health", get(health))
        .route("/wallets", post(create_wallet))
        .route("/wallets/{id}", get(get_wallet))
        .route("/transfers", post(create_transfer))
        .route("/transfers/{id}", get(get_transfer))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn create_wallet(
    State(state): State<AppState>,
    Json(request): Json<CreateWalletRequest>,
) -> Result<(StatusCode, Json<WalletResponse>), StatusCode> {
    let command = CreateWallet {
        owner: request.owner,
        balance: request.balance,
    };

    let wallet = state
        .wallet_service
        .create(command)
        .await
        .map_err(wallet_error_status)?;

    Ok((StatusCode::CREATED, Json(wallet.into())))
}

async fn get_wallet(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<WalletResponse>, StatusCode> {
    let wallet = state
        .wallet_service
        .get(id)
        .await
        .map_err(wallet_error_status)?;

    Ok(Json(wallet.into()))
}

fn wallet_error_status(error: WalletError) -> StatusCode {
    match error {
        WalletError::InvalidOwner | WalletError::InvalidBalance => StatusCode::BAD_REQUEST,
        WalletError::NotFound => StatusCode::NOT_FOUND,
        WalletError::Repository(error) => {
            log::error!("wallet repository error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn get_transfer(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<TransferResponse>, StatusCode> {
    let transfer = state
        .transfer_service
        .get(id)
        .await
        .map_err(transfer_error_status)?;

    Ok(Json(transfer.into()))
}

async fn create_transfer(
    State(state): State<AppState>,
    Json(request): Json<CreateTransferRequest>,
) -> Result<(StatusCode, Json<TransferResponse>), StatusCode> {
    let command = TransferMoney {
        from_wallet_id: request.from_wallet_id,
        to_wallet_id: request.to_wallet_id,
        amount: request.amount,
        idempotency_key: request.idempotency_key,
    };

    let outcome = state
        .transfer_service
        .transfer(command)
        .await
        .map_err(transfer_error_status)?;

    match outcome {
        TransferOutcome::Created(transfer) => Ok((StatusCode::CREATED, Json(transfer.into()))),
        TransferOutcome::Existing(transfer) => Ok((StatusCode::OK, Json(transfer.into()))),
    }
}

fn transfer_error_status(error: TransferError) -> StatusCode {
    match error {
        TransferError::InvalidAmount
        | TransferError::SameWallet
        | TransferError::EmptyIdempotencyKey
        | TransferError::BalanceOverflow => StatusCode::BAD_REQUEST,
        TransferError::WalletNotFound | TransferError::TransferNotFound => StatusCode::NOT_FOUND,
        TransferError::InsufficientFunds | TransferError::IdempotencyConflict => {
            StatusCode::CONFLICT
        }
        TransferError::Repository(error) => {
            log::error!("transfer repository error: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
