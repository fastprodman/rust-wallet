use async_trait::async_trait;
use sqlx::PgPool;

use crate::{
    domain::Wallet,
    port::{RepositoryError, WalletRepository},
};

pub struct PostgresWalletRepository {
    pool: PgPool,
}

impl PostgresWalletRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WalletRepository for PostgresWalletRepository {
    async fn create(&self, owner: String, balance: i64) -> Result<Wallet, RepositoryError> {
        sqlx::query_as!(
            Wallet,
            r#"
            INSERT INTO wallets (owner, balance)
            VALUES ($1, $2)
            RETURNING id, owner, balance
            "#,
            owner,
            balance,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| RepositoryError::new(error.to_string()))
    }

    async fn find_by_id(&self, id: i64) -> Result<Option<Wallet>, RepositoryError> {
        sqlx::query_as!(
            Wallet,
            r#"
            SELECT id, owner, balance
            FROM wallets
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| RepositoryError::new(error.to_string()))
    }
}
