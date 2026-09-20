use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    domain::Transfer,
    port::{RepositoryError, TransferRepository, TransferTransaction, WalletBalance},
};

pub struct PostgresTransferRepository {
    pool: PgPool,
}

impl PostgresTransferRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TransferRepository for PostgresTransferRepository {
    async fn find_by_id(&self, id: i64) -> Result<Option<Transfer>, RepositoryError> {
        sqlx::query_as!(
            Transfer,
            r#"
            SELECT
                id,
                from_wallet_id,
                to_wallet_id,
                amount,
                idempotency_key,
                status
            FROM transfers
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| RepositoryError::new(error.to_string()))
    }

    async fn begin(&self) -> Result<Box<dyn TransferTransaction>, RepositoryError> {
        let transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| RepositoryError::new(error.to_string()))?;

        Ok(Box::new(PostgresTransferTransaction { transaction }))
    }
}

struct PostgresTransferTransaction {
    transaction: Transaction<'static, Postgres>,
}

#[async_trait]
impl TransferTransaction for PostgresTransferTransaction {
    async fn lock_wallets(
        &mut self,
        from_wallet_id: i64,
        to_wallet_id: i64,
    ) -> Result<Vec<WalletBalance>, RepositoryError> {
        sqlx::query_as!(
            WalletBalance,
            r#"
            SELECT id, balance
            FROM wallets
            WHERE id IN ($1, $2)
            ORDER BY id
            FOR UPDATE
            "#,
            from_wallet_id,
            to_wallet_id,
        )
        .fetch_all(&mut *self.transaction)
        .await
        .map_err(|error| RepositoryError::new(error.to_string()))
    }

    async fn insert_transfer(
        &mut self,
        from_wallet_id: i64,
        to_wallet_id: i64,
        amount: i64,
        idempotency_key: &str,
    ) -> Result<Option<Transfer>, RepositoryError> {
        sqlx::query_as!(
            Transfer,
            r#"
            INSERT INTO transfers (
                from_wallet_id,
                to_wallet_id,
                amount,
                idempotency_key,
                status
            )
            VALUES ($1, $2, $3, $4, 'completed')
            ON CONFLICT (idempotency_key) DO NOTHING
            RETURNING
                id,
                from_wallet_id,
                to_wallet_id,
                amount,
                idempotency_key,
                status
            "#,
            from_wallet_id,
            to_wallet_id,
            amount,
            idempotency_key,
        )
        .fetch_optional(&mut *self.transaction)
        .await
        .map_err(|error| RepositoryError::new(error.to_string()))
    }

    async fn find_by_idempotency_key(
        &mut self,
        idempotency_key: &str,
    ) -> Result<Transfer, RepositoryError> {
        sqlx::query_as!(
            Transfer,
            r#"
            SELECT
                id,
                from_wallet_id,
                to_wallet_id,
                amount,
                idempotency_key,
                status
            FROM transfers
            WHERE idempotency_key = $1
            "#,
            idempotency_key,
        )
        .fetch_one(&mut *self.transaction)
        .await
        .map_err(|error| RepositoryError::new(error.to_string()))
    }

    async fn update_wallet_balance(
        &mut self,
        wallet_id: i64,
        balance: i64,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            UPDATE wallets
            SET balance = $1
            WHERE id = $2
            "#,
            balance,
            wallet_id,
        )
        .execute(&mut *self.transaction)
        .await
        .map(|_| ())
        .map_err(|error| RepositoryError::new(error.to_string()))
    }

    async fn commit(self: Box<Self>) -> Result<(), RepositoryError> {
        let Self { transaction } = *self;

        transaction
            .commit()
            .await
            .map_err(|error| RepositoryError::new(error.to_string()))
    }
}
