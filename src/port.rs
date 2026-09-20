mod transfer_repository;
mod wallet_repository;

use std::fmt;

pub use transfer_repository::{TransferRepository, TransferTransaction, WalletBalance};
pub use wallet_repository::WalletRepository;

#[derive(Debug)]
pub struct RepositoryError {
    message: String,
}

impl RepositoryError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RepositoryError {}
