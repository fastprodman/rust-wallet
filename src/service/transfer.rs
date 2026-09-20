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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::*;
    use crate::port::{TransferTransaction, WalletBalance};

    #[derive(Clone)]
    struct TransferFixture {
        id: i64,
        from_wallet_id: i64,
        to_wallet_id: i64,
        amount: i64,
        idempotency_key: String,
    }

    impl TransferFixture {
        fn matching(amount: i64) -> Self {
            Self {
                id: 10,
                from_wallet_id: 1,
                to_wallet_id: 2,
                amount,
                idempotency_key: "transfer-1".to_owned(),
            }
        }

        fn to_transfer(&self) -> Transfer {
            Transfer {
                id: self.id,
                from_wallet_id: self.from_wallet_id,
                to_wallet_id: self.to_wallet_id,
                amount: self.amount,
                idempotency_key: self.idempotency_key.clone(),
                status: "completed".to_owned(),
            }
        }
    }

    struct FakeTransactionState {
        wallets: Vec<(i64, i64)>,
        insert_returns_none: bool,
        existing: TransferFixture,
        updates: Vec<(i64, i64)>,
        begin_calls: usize,
        committed: bool,
        rolled_back: bool,
    }

    struct FakeTransferRepository {
        state: Arc<Mutex<FakeTransactionState>>,
        found_transfer: Option<TransferFixture>,
    }

    #[async_trait]
    impl TransferRepository for FakeTransferRepository {
        async fn find_by_id(&self, _id: i64) -> Result<Option<Transfer>, RepositoryError> {
            Ok(self
                .found_transfer
                .as_ref()
                .map(TransferFixture::to_transfer))
        }

        async fn begin(&self) -> Result<Box<dyn TransferTransaction>, RepositoryError> {
            self.state.lock().expect("state lock poisoned").begin_calls += 1;

            Ok(Box::new(FakeTransferTransaction {
                state: self.state.clone(),
                committed: false,
            }))
        }
    }

    struct FakeTransferTransaction {
        state: Arc<Mutex<FakeTransactionState>>,
        committed: bool,
    }

    impl Drop for FakeTransferTransaction {
        fn drop(&mut self) {
            if !self.committed {
                self.state.lock().expect("state lock poisoned").rolled_back = true;
            }
        }
    }

    #[async_trait]
    impl TransferTransaction for FakeTransferTransaction {
        async fn lock_wallets(
            &mut self,
            _from_wallet_id: i64,
            _to_wallet_id: i64,
        ) -> Result<Vec<WalletBalance>, RepositoryError> {
            let wallets = self
                .state
                .lock()
                .expect("state lock poisoned")
                .wallets
                .clone();

            Ok(wallets
                .into_iter()
                .map(|(id, balance)| WalletBalance { id, balance })
                .collect())
        }

        async fn insert_transfer(
            &mut self,
            from_wallet_id: i64,
            to_wallet_id: i64,
            amount: i64,
            idempotency_key: &str,
        ) -> Result<Option<Transfer>, RepositoryError> {
            if self
                .state
                .lock()
                .expect("state lock poisoned")
                .insert_returns_none
            {
                return Ok(None);
            }

            Ok(Some(Transfer {
                id: 10,
                from_wallet_id,
                to_wallet_id,
                amount,
                idempotency_key: idempotency_key.to_owned(),
                status: "completed".to_owned(),
            }))
        }

        async fn find_by_idempotency_key(
            &mut self,
            _idempotency_key: &str,
        ) -> Result<Transfer, RepositoryError> {
            let existing = self
                .state
                .lock()
                .expect("state lock poisoned")
                .existing
                .clone();

            Ok(existing.to_transfer())
        }

        async fn update_wallet_balance(
            &mut self,
            wallet_id: i64,
            balance: i64,
        ) -> Result<(), RepositoryError> {
            self.state
                .lock()
                .expect("state lock poisoned")
                .updates
                .push((wallet_id, balance));

            Ok(())
        }

        async fn commit(mut self: Box<Self>) -> Result<(), RepositoryError> {
            self.committed = true;
            self.state.lock().expect("state lock poisoned").committed = true;

            Ok(())
        }
    }

    fn service_with(
        wallets: Vec<(i64, i64)>,
        insert_returns_none: bool,
        existing: TransferFixture,
    ) -> (TransferService, Arc<Mutex<FakeTransactionState>>) {
        let state = Arc::new(Mutex::new(FakeTransactionState {
            wallets,
            insert_returns_none,
            existing,
            updates: Vec::new(),
            begin_calls: 0,
            committed: false,
            rolled_back: false,
        }));
        let repository = Arc::new(FakeTransferRepository {
            state: state.clone(),
            found_transfer: None,
        });

        (TransferService::new(repository), state)
    }

    fn command(amount: i64) -> TransferMoney {
        TransferMoney {
            from_wallet_id: 1,
            to_wallet_id: 2,
            amount,
            idempotency_key: "transfer-1".to_owned(),
        }
    }

    #[tokio::test]
    async fn rejects_invalid_commands_before_starting_a_transaction() {
        let (service, state) = service_with(
            vec![(1, 1_000), (2, 500)],
            false,
            TransferFixture::matching(100),
        );

        let invalid_amount = service.transfer(command(0)).await;
        let same_wallet = service
            .transfer(TransferMoney {
                from_wallet_id: 1,
                to_wallet_id: 1,
                amount: 100,
                idempotency_key: "transfer-2".to_owned(),
            })
            .await;
        let empty_key = service
            .transfer(TransferMoney {
                from_wallet_id: 1,
                to_wallet_id: 2,
                amount: 100,
                idempotency_key: "   ".to_owned(),
            })
            .await;

        assert!(matches!(invalid_amount, Err(TransferError::InvalidAmount)));
        assert!(matches!(same_wallet, Err(TransferError::SameWallet)));
        assert!(matches!(empty_key, Err(TransferError::EmptyIdempotencyKey)));
        assert_eq!(state.lock().expect("state lock poisoned").begin_calls, 0);
    }

    #[tokio::test]
    async fn completes_a_transfer_and_updates_both_balances() {
        let (service, state) = service_with(
            vec![(1, 1_000), (2, 500)],
            false,
            TransferFixture::matching(100),
        );

        let outcome = service
            .transfer(command(100))
            .await
            .expect("transfer should succeed");

        let TransferOutcome::Created(transfer) = outcome else {
            panic!("expected a newly created transfer");
        };
        assert_eq!(transfer.amount, 100);

        let state = state.lock().expect("state lock poisoned");
        assert_eq!(state.updates, vec![(1, 900), (2, 600)]);
        assert!(state.committed);
        assert!(!state.rolled_back);
    }

    #[tokio::test]
    async fn returns_an_existing_idempotent_transfer_without_updating_balances() {
        let (service, state) = service_with(
            vec![(1, 900), (2, 600)],
            true,
            TransferFixture::matching(100),
        );

        let outcome = service
            .transfer(command(100))
            .await
            .expect("idempotent retry should succeed");

        let TransferOutcome::Existing(transfer) = outcome else {
            panic!("expected the existing transfer");
        };
        assert_eq!(transfer.id, 10);

        let state = state.lock().expect("state lock poisoned");
        assert!(state.updates.is_empty());
        assert!(state.committed);
        assert!(!state.rolled_back);
    }

    #[tokio::test]
    async fn rejects_reusing_an_idempotency_key_for_different_parameters() {
        let (service, state) = service_with(
            vec![(1, 1_000), (2, 500)],
            true,
            TransferFixture::matching(99),
        );

        let result = service.transfer(command(100)).await;

        assert!(matches!(result, Err(TransferError::IdempotencyConflict)));
        let state = state.lock().expect("state lock poisoned");
        assert!(state.updates.is_empty());
        assert!(!state.committed);
        assert!(state.rolled_back);
    }

    #[tokio::test]
    async fn rolls_back_when_funds_are_insufficient() {
        let (service, state) = service_with(
            vec![(1, 50), (2, 500)],
            false,
            TransferFixture::matching(100),
        );

        let result = service.transfer(command(100)).await;

        assert!(matches!(result, Err(TransferError::InsufficientFunds)));
        let state = state.lock().expect("state lock poisoned");
        assert!(state.updates.is_empty());
        assert!(!state.committed);
        assert!(state.rolled_back);
    }

    #[tokio::test]
    async fn rolls_back_when_a_wallet_is_missing() {
        let (service, state) =
            service_with(vec![(1, 1_000)], false, TransferFixture::matching(100));

        let result = service.transfer(command(100)).await;

        assert!(matches!(result, Err(TransferError::WalletNotFound)));
        let state = state.lock().expect("state lock poisoned");
        assert!(!state.committed);
        assert!(state.rolled_back);
    }

    #[tokio::test]
    async fn rolls_back_when_the_destination_balance_overflows() {
        let (service, state) = service_with(
            vec![(1, 10), (2, i64::MAX)],
            false,
            TransferFixture::matching(1),
        );

        let result = service.transfer(command(1)).await;

        assert!(matches!(result, Err(TransferError::BalanceOverflow)));
        let state = state.lock().expect("state lock poisoned");
        assert!(state.updates.is_empty());
        assert!(!state.committed);
        assert!(state.rolled_back);
    }

    #[tokio::test]
    async fn reports_a_missing_transfer() {
        let (service, state) = service_with(
            vec![(1, 1_000), (2, 500)],
            false,
            TransferFixture::matching(100),
        );

        let result = service.get(99).await;

        assert!(matches!(result, Err(TransferError::TransferNotFound)));
        assert_eq!(state.lock().expect("state lock poisoned").begin_calls, 0);
    }
}
