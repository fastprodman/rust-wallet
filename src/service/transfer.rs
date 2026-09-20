use std::sync::Arc;

use crate::{
    domain::Transfer,
    port::{RepositoryError, TransferRepository},
};

pub struct TransferMoney {
    pub from_wallet_id: i64,
    pub to_wallet_id: i64,
    pub amount: i64,
    pub idempotency_key: String,
}

pub enum TransferOutcome {
    Created(Transfer),
    Existing(Transfer),
}

#[derive(Debug)]
pub enum TransferError {
    InvalidAmount,
    SameWallet,
    EmptyIdempotencyKey,
    WalletNotFound,
    TransferNotFound,
    InsufficientFunds,
    IdempotencyConflict,
    BalanceOverflow,
    Repository(RepositoryError),
}

impl From<RepositoryError> for TransferError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

pub struct TransferService {
    repository: Arc<dyn TransferRepository>,
}

impl TransferService {
    pub fn new(repository: Arc<dyn TransferRepository>) -> Self {
        Self { repository }
    }

    pub async fn get(&self, id: i64) -> Result<Transfer, TransferError> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or(TransferError::TransferNotFound)
    }

    pub async fn transfer(&self, command: TransferMoney) -> Result<TransferOutcome, TransferError> {
        if command.amount <= 0 {
            return Err(TransferError::InvalidAmount);
        }

        if command.from_wallet_id == command.to_wallet_id {
            return Err(TransferError::SameWallet);
        }

        if command.idempotency_key.trim().is_empty() {
            return Err(TransferError::EmptyIdempotencyKey);
        }

        let mut transaction = self.repository.begin().await?;

        let locked_wallets = transaction
            .lock_wallets(command.from_wallet_id, command.to_wallet_id)
            .await?;

        let from_balance = locked_wallets
            .iter()
            .find(|wallet| wallet.id == command.from_wallet_id)
            .map(|wallet| wallet.balance)
            .ok_or(TransferError::WalletNotFound)?;

        let to_balance = locked_wallets
            .iter()
            .find(|wallet| wallet.id == command.to_wallet_id)
            .map(|wallet| wallet.balance)
            .ok_or(TransferError::WalletNotFound)?;

        let transfer = transaction
            .insert_transfer(
                command.from_wallet_id,
                command.to_wallet_id,
                command.amount,
                command.idempotency_key.as_str(),
            )
            .await?;

        let transfer = match transfer {
            Some(transfer) => transfer,
            None => {
                let existing = transaction
                    .find_by_idempotency_key(command.idempotency_key.as_str())
                    .await?;

                if existing.from_wallet_id != command.from_wallet_id
                    || existing.to_wallet_id != command.to_wallet_id
                    || existing.amount != command.amount
                {
                    return Err(TransferError::IdempotencyConflict);
                }

                transaction.commit().await?;
                return Ok(TransferOutcome::Existing(existing));
            }
        };

        if from_balance < command.amount {
            return Err(TransferError::InsufficientFunds);
        }

        let new_from_balance = from_balance - command.amount;
        let new_to_balance = to_balance
            .checked_add(command.amount)
            .ok_or(TransferError::BalanceOverflow)?;

        transaction
            .update_wallet_balance(command.from_wallet_id, new_from_balance)
            .await?;

        transaction
            .update_wallet_balance(command.to_wallet_id, new_to_balance)
            .await?;

        transaction.commit().await?;

        Ok(TransferOutcome::Created(transfer))
    }
}
