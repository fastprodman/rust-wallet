pub struct Transfer {
    pub id: i64,
    pub from_wallet_id: i64,
    pub to_wallet_id: i64,
    pub amount: i64,
    pub idempotency_key: String,
    pub status: String,
}
