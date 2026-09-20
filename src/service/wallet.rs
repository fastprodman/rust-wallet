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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;

    use super::*;

    struct FakeWalletRepository {
        create_calls: AtomicUsize,
        find_calls: AtomicUsize,
        wallet_exists: bool,
        fail: bool,
    }

    impl FakeWalletRepository {
        fn new(wallet_exists: bool) -> Self {
            Self {
                create_calls: AtomicUsize::new(0),
                find_calls: AtomicUsize::new(0),
                wallet_exists,
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                create_calls: AtomicUsize::new(0),
                find_calls: AtomicUsize::new(0),
                wallet_exists: false,
                fail: true,
            }
        }
    }

    #[async_trait]
    impl WalletRepository for FakeWalletRepository {
        async fn create(&self, owner: String, balance: i64) -> Result<Wallet, RepositoryError> {
            self.create_calls.fetch_add(1, Ordering::Relaxed);

            if self.fail {
                return Err(RepositoryError::new("wallet repository failed"));
            }

            Ok(Wallet {
                id: 7,
                owner,
                balance,
            })
        }

        async fn find_by_id(&self, id: i64) -> Result<Option<Wallet>, RepositoryError> {
            self.find_calls.fetch_add(1, Ordering::Relaxed);

            if self.fail {
                return Err(RepositoryError::new("wallet repository failed"));
            }

            Ok(self.wallet_exists.then(|| Wallet {
                id,
                owner: "Alice".to_owned(),
                balance: 1_000,
            }))
        }
    }

    #[tokio::test]
    async fn creates_a_valid_wallet() {
        let repository = Arc::new(FakeWalletRepository::new(false));
        let service = WalletService::new(repository.clone());

        let wallet = service
            .create(CreateWallet {
                owner: "Alice".to_owned(),
                balance: 1_000,
            })
            .await
            .expect("wallet should be created");

        assert_eq!(wallet.id, 7);
        assert_eq!(wallet.owner, "Alice");
        assert_eq!(wallet.balance, 1_000);
        assert_eq!(repository.create_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn rejects_invalid_wallets_before_calling_the_repository() {
        let repository = Arc::new(FakeWalletRepository::new(false));
        let service = WalletService::new(repository.clone());

        let empty_owner = service
            .create(CreateWallet {
                owner: String::new(),
                balance: 100,
            })
            .await;
        let negative_balance = service
            .create(CreateWallet {
                owner: "Alice".to_owned(),
                balance: -1,
            })
            .await;

        assert!(matches!(empty_owner, Err(WalletError::InvalidOwner)));
        assert!(matches!(negative_balance, Err(WalletError::InvalidBalance)));
        assert_eq!(repository.create_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn reports_a_missing_wallet() {
        let repository = Arc::new(FakeWalletRepository::new(false));
        let service = WalletService::new(repository.clone());

        let result = service.get(99).await;

        assert!(matches!(result, Err(WalletError::NotFound)));
        assert_eq!(repository.find_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn maps_repository_failures() {
        let repository = Arc::new(FakeWalletRepository::failing());
        let service = WalletService::new(repository);

        let result = service.get(1).await;

        assert!(matches!(result, Err(WalletError::Repository(_))));
    }
}
