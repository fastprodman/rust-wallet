mod adapter;
mod domain;
mod port;
mod service;

use std::sync::Arc;

use adapter::{
    http,
    postgres::{PostgresTransferRepository, PostgresWalletRepository},
};
use service::{TransferService, WalletService};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let wallet_repository = Arc::new(PostgresWalletRepository::new(pool.clone()));
    let wallet_service = Arc::new(WalletService::new(wallet_repository));
    let transfer_repository = Arc::new(PostgresTransferRepository::new(pool));
    let transfer_service = Arc::new(TransferService::new(transfer_repository));

    let app = http::router(wallet_service, transfer_service);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    axum::serve(listener, app).await?;

    Ok(())
}
