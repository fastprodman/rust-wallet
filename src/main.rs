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

    let bind_address =
        std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());

    let listener = tokio::net::TcpListener::bind(bind_address).await?;

    let local_address = listener.local_addr()?;

    println!("server listening on http://{local_address}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    eprintln!("shutdown signal received");
}
