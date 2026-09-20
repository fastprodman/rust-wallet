use async_trait::async_trait;

use crate::{domain::Transfer, port::RepositoryError};

pub struct WalletBalance {
    pub id: i64,
    pub balance: i64,
}

#[async_trait]
pub trait TransferTransaction: Send {
    async fn lock_wallets(
        &mut self,
        from_wallet_id: i64,
        to_wallet_id: i64,
    ) -> Result<Vec<WalletBalance>, RepositoryError>;

    async fn insert_transfer(
        &mut self,
        from_wallet_id: i64,
        to_wallet_id: i64,
        amount: i64,
        idempotency_key: &str,
    ) -> Result<Option<Transfer>, RepositoryError>;

    async fn find_by_idempotency_key(
        &mut self,
        idempotency_key: &str,
    ) -> Result<Transfer, RepositoryError>;

    async fn update_wallet_balance(
        &mut self,
        wallet_id: i64,
        balance: i64,
    ) -> Result<(), RepositoryError>;

    async fn commit(self: Box<Self>) -> Result<(), RepositoryError>;
}

#[async_trait]
pub trait TransferRepository: Send + Sync {
    async fn find_by_id(&self, id: i64) -> Result<Option<Transfer>, RepositoryError>;

    async fn begin(&self) -> Result<Box<dyn TransferTransaction>, RepositoryError>;
}
