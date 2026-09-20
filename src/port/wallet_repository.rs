use async_trait::async_trait;

use crate::{domain::Wallet, port::RepositoryError};

#[async_trait]
pub trait WalletRepository: Send + Sync {
    async fn create(&self, owner: String, balance: i64) -> Result<Wallet, RepositoryError>;

    async fn find_by_id(&self, id: i64) -> Result<Option<Wallet>, RepositoryError>;
}
