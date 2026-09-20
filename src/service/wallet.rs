use std::sync::Arc;

use crate::{
    domain::Wallet,
    port::{RepositoryError, WalletRepository},
};

pub struct CreateWallet {
    pub owner: String,
    pub balance: i64,
}

#[derive(Debug)]
pub enum WalletError {
    InvalidOwner,
    InvalidBalance,
    NotFound,
    Repository(RepositoryError),
}

impl From<RepositoryError> for WalletError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

pub struct WalletService {
    repository: Arc<dyn WalletRepository>,
}

impl WalletService {
    pub fn new(repository: Arc<dyn WalletRepository>) -> Self {
        Self { repository }
    }

    pub async fn create(&self, command: CreateWallet) -> Result<Wallet, WalletError> {
        if command.owner.is_empty() {
            return Err(WalletError::InvalidOwner);
        }

        if command.balance < 0 {
            return Err(WalletError::InvalidBalance);
        }

        self.repository
            .create(command.owner, command.balance)
            .await
            .map_err(WalletError::from)
    }

    pub async fn get(&self, id: i64) -> Result<Wallet, WalletError> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or(WalletError::NotFound)
    }
}
