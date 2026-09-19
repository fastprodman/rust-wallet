# Axum + PostgreSQL Transactional Processing Learning Project

## Purpose

I want to learn **Axum in Rust by building a realistic transactional backend**, not by copying a finished project.

Act as my tutor and implementation guide.

Please help me implement this project **incrementally**. For each stage:

1. Explain the concept briefly.
2. Show only the code needed for that stage.
3. Explain the important Rust/Axum/SQLx concepts in the code.
4. Point out common mistakes.
5. Give me a small task to implement myself before moving on.
6. Review my code when I send it back.
7. Do not generate the entire finished project unless I explicitly ask.

My background:
- Senior Go backend developer.
- Familiar with PostgreSQL, distributed systems, DDD/hexagonal architecture, Kafka/outbox patterns, Docker, Kubernetes, tracing, and concurrency.
- Learning Rust and Axum.
- I want explanations to focus especially on where Rust/Axum differs from Go.

---

# Project: Transactional Wallet Service

Build a small Rust service using:

- Rust
- Axum
- Tokio
- SQLx
- PostgreSQL
- Serde
- tracing
- Tower middleware

The main goal is to learn:

- Axum routing and handlers
- extractors
- application state
- error handling with `IntoResponse`
- SQLx transactions
- PostgreSQL row locking
- idempotency
- transactional outbox
- background Tokio tasks
- concurrency testing
- graceful shutdown

---

# Business Scenario

Users have wallets.

A client can transfer money from one wallet to another.

The transfer must satisfy these rules:

- sender must exist
- receiver must exist
- amount must be positive
- sender must have enough balance
- both balance changes must happen atomically
- a transfer record must be written
- an outbox event must be written in the same transaction
- retrying the same request must not create a second transfer

Example:

```text
Alice balance: 100 EUR
Bob balance:    20 EUR

POST /transfers
{
    "from_wallet_id": 1,
    "to_wallet_id": 2,
    "amount": 3000,
    "idempotency_key": "transfer-123"
}
```

Store monetary amounts in integer cents.

After success:

```text
Alice: 70 EUR
Bob:   50 EUR
```

If anything fails, both balances must remain unchanged.

---

# HTTP API

Implement:

```text
POST /wallets
GET  /wallets/{id}

POST /transfers
GET  /transfers/{id}

GET  /health
```

Example transfer request:

```json
{
  "from_wallet_id": 10,
  "to_wallet_id": 20,
  "amount": 5000,
  "idempotency_key": "8ad3157e-..."
}
```

---

# Database

Create these tables.

## wallets

```text
id
owner
balance
created_at
```

## transfers

```text
id
from_wallet_id
to_wallet_id
amount
idempotency_key
status
created_at
```

Add:

```sql
UNIQUE(idempotency_key)
```

## outbox

```text
id
aggregate_type
aggregate_id
event_type
payload
created_at
processed_at
```

---

# Architecture

Use approximately:

```text
HTTP
 ↓
Axum handler
 ↓
TransferService
 ↓
Database transaction
 ↓
WalletRepository
TransferRepository
OutboxRepository
 ↓
PostgreSQL
```

Suggested project structure:

```text
src/
├── main.rs
├── state.rs
├── error.rs
│
├── http/
│   ├── mod.rs
│   ├── wallets.rs
│   └── transfers.rs
│
├── domain/
│   ├── wallet.rs
│   └── transfer.rs
│
├── service/
│   └── transfer.rs
│
└── repository/
    ├── wallet.rs
    ├── transfer.rs
    └── outbox.rs
```

Avoid over-engineering repository abstractions initially.

I want to understand the basic Rust ownership and SQLx transaction model before introducing excessive traits or generic abstractions.

---

# Core Transactional Task

Implement something conceptually like:

```rust
async fn transfer(
    &self,
    command: TransferMoney,
) -> Result<Transfer, TransferError>
```

The transaction should perform:

```text
BEGIN

check idempotency key

lock sender wallet
lock receiver wallet

verify sender balance

decrease sender balance
increase receiver balance

insert transfer

insert outbox event

COMMIT
```

If anything fails:

```text
ROLLBACK
```

Use SQLx transactions:

```rust
let mut tx = pool.begin().await?;

// operations using &mut tx

tx.commit().await?;
```

An important design requirement:

> The business/service layer owns the transaction boundary.

Individual repositories should not independently begin and commit their own transactions for operations that belong to the same business transaction.

Please explicitly teach me how SQLx transaction borrowing works in Rust, especially when multiple repository methods need to operate using the same transaction.

---

# Concurrency Problem

Make this a mandatory part of the project.

Suppose Alice has:

```text
100 EUR
```

Two requests arrive concurrently:

```text
transfer 80 EUR
transfer 80 EUR
```

Both must not succeed.

Use PostgreSQL locking such as:

```sql
SELECT *
FROM wallets
WHERE id = $1
FOR UPDATE;
```

Expected result:

```text
one succeeds
one fails with insufficient funds
```

The final balance must never become:

```text
-60 EUR
```

Please explain:

- how the race happens without locking
- what `FOR UPDATE` locks
- when the lock is released
- what the second transaction does while waiting
- how deadlocks can happen if two wallet rows are locked in different orders
- how to impose deterministic lock ordering

For example, consider simultaneous transfers:

```text
A -> B
B -> A
```

Teach me how to avoid a deadlock.

---

# Idempotency

Sending the same logical request twice must not move money twice.

Example:

```text
idempotency_key = "abc123"
```

The first call performs the transaction.

The second call should return the already-created transfer or otherwise indicate the same successful result without changing balances again.

This must work for:

```text
client sends request
server commits
network connection dies
client retries
```

Please also cover the harder case:

```text
two identical requests with the same idempotency key arrive concurrently
```

Do not rely only on:

```text
SELECT first
then INSERT later
```

because that can race.

Use the database uniqueness constraint as part of the correctness mechanism.

Explain the possible implementation strategies and their trade-offs.

---

# Transactional Outbox

During the same transaction, insert an event such as:

```json
{
  "type": "money_transferred",
  "transfer_id": "...",
  "from_wallet_id": 10,
  "to_wallet_id": 20,
  "amount": 5000
}
```

into the `outbox` table.

The important invariant is:

```text
transfer committed
⇔
outbox event exists
```

We must never have:

```text
money moved
but event missing
```

or:

```text
event exists
but money transfer rolled back
```

Initially, do not add Kafka.

First make the database transaction and outbox pattern correct.

---

# Background Outbox Publisher

After the transfer path works, add a Tokio background task:

```text
outbox worker
    ↓
find unprocessed events
    ↓
publish event
    ↓
mark processed_at
```

Initially, publishing can simply be:

```rust
tracing::info!("publishing {:?}", event);
```

Later it could be replaced with Kafka.

Consider this failure:

```text
publish succeeds
↓
process crashes
↓
processed_at was not updated
```

The event can therefore be published again.

Design around:

```text
at-least-once delivery
```

Do not pretend the system gives exactly-once delivery automatically.

Please later teach me:

- polling strategies
- batching
- `FOR UPDATE SKIP LOCKED`
- multiple outbox workers
- retry handling
- poison events
- event ordering
- idempotent consumers

But introduce those only after the basic implementation works.

---

# Axum Concepts That Must Be Used

The project should force me to use:

```rust
State<AppState>
Path<T>
Json<T>
IntoResponse
```

Create application state roughly like:

```rust
#[derive(Clone)]
struct AppState {
    pool: PgPool,
    transfer_service: Arc<TransferService>,
}
```

Handlers should look conceptually like:

```rust
async fn create_transfer(
    State(state): State<AppState>,
    Json(request): Json<CreateTransferRequest>,
) -> Result<impl IntoResponse, AppError>
```

Explain why Axum extractors are different from manually reading fields from a Go `http.Request`.

---

# Error Handling

Create a central error type such as:

```rust
enum AppError
```

Implement:

```rust
IntoResponse for AppError
```

Map errors appropriately:

```text
wallet does not exist       -> 404
insufficient balance        -> 409
invalid amount              -> 400
unexpected database error   -> 500
```

Idempotency should not simply become a generic `500` caused by a uniqueness violation.

Please teach me good separation between:

```text
domain/business errors
infrastructure/database errors
HTTP representation errors
```

without building an unnecessarily complicated architecture.

---

# Testing

Write integration tests covering:

```text
transfer succeeds

sender has insufficient funds

sender does not exist

receiver does not exist

amount <= 0

same idempotency key sent twice

two concurrent requests with the same idempotency key

database failure rolls everything back

outbox record is created with successful transfer

two concurrent transfers cannot overspend

two opposite-direction transfers do not deadlock
```

The concurrency test is especially important.

Use Tokio concurrency, for example:

```rust
tokio::join!(
    transfer_a(),
    transfer_b(),
);
```

Then query the database directly and verify the resulting invariants.

Please prefer tests that verify externally visible behavior and database invariants rather than only mocking repository methods.

---

# Invariants

These should always hold.

```text
wallet.balance >= 0
```

For transfers only:

```text
sum(all wallet balances)
does not change
```

Also:

```text
every successful transfer has one transfer record

every successful transfer has an outbox event

the same idempotency key cannot move money twice
```

Think of these invariants as more important than individual functions.

---

# Learning Stages

Please guide me through these stages one at a time.

## Stage 1 — Axum Basics

Build:

```text
GET /health
POST /wallets
GET /wallets/{id}
```

Teach:

- `Router`
- routes
- handlers
- `State`
- `Path`
- `Json`
- request/response structs
- Serde

Do not introduce transactions yet.

---

## Stage 2 — PostgreSQL + SQLx

Add PostgreSQL.

Teach:

- `PgPool`
- SQLx queries
- migrations
- `query!` vs runtime queries where appropriate
- mapping rows to Rust structures
- error propagation

---

## Stage 3 — Basic Transfer

Implement a transfer without advanced concurrency handling.

Teach:

- `pool.begin()`
- `Transaction<'_, Postgres>`
- passing the same transaction between operations
- commit
- rollback behavior
- `?` operator interaction with transaction errors

---

## Stage 4 — Service Transaction Boundary

Introduce:

```text
TransferService
```

The service owns the transaction.

Repositories operate using the transaction passed to them.

Teach me what a clean Rust API for this looks like.

In particular, explain how to avoid fighting the borrow checker when several repository calls need:

```rust
&mut Transaction<'_, Postgres>
```

or SQLx's executor abstractions.

Compare this with passing `*sql.Tx` in Go.

---

## Stage 5 — Concurrency and Row Locking

Introduce:

```sql
FOR UPDATE
```

Write a concurrent overspending test.

Then introduce deterministic wallet lock ordering.

---

## Stage 6 — Idempotency

Add idempotency handling.

Test both sequential and concurrent duplicate requests.

Use database constraints as the final authority.

---

## Stage 7 — Transactional Outbox

Write the outbox event in the same transaction.

Verify the invariant with integration tests.

---

## Stage 8 — Background Worker

Add a Tokio task that polls and publishes outbox events.

Start with logging instead of Kafka.

---

## Stage 9 — Tower Middleware

Add:

```text
request ID
tracing
timeout
```

Teach how Axum uses the Tower ecosystem.

Explain:

```text
Axum
  ↓
Tower
  ↓
Hyper
  ↓
Tokio
```

at a practical level.

---

## Stage 10 — Graceful Shutdown

Implement graceful shutdown.

Make sure:

- HTTP server stops accepting requests
- in-flight work is handled appropriately
- background worker stops cleanly

Discuss what happens to an open SQL transaction if a task is cancelled.

---

# Final Challenge Questions

At the end, I should be able to answer these without looking them up:

1. Why can't every repository independently open its own transaction?
2. Why does `FOR UPDATE` prevent the overspending race?
3. When exactly are PostgreSQL row locks released?
4. How can `A -> B` and `B -> A` transfers deadlock?
5. How does deterministic lock ordering prevent that deadlock?
6. What happens if two requests use the same idempotency key concurrently?
7. Why is checking an idempotency key before inserting insufficient by itself?
8. Why must the outbox insert happen inside the wallet transaction?
9. Why can the outbox consumer still receive duplicate events?
10. Where should transaction boundaries live in this architecture?
11. Should `PgPool`, a SQLx `Transaction`, or repositories be stored in Axum `State`?
12. What happens if the process crashes immediately after `COMMIT`?
13. Why is holding a database transaction open across an external HTTP call dangerous?
14. How could Kafka later replace the fake outbox publisher without changing the wallet transaction?
15. What Rust ownership/borrowing issue makes transaction-aware repositories different from their Go equivalents?
16. What role does Tower's `Service` abstraction play in Axum?

---

# Tutor Instructions

Important:

Do **not** solve the entire project immediately.

Start with **Stage 1 only**.

For each stage:

1. Explain the goal.
2. Explain any Rust concepts needed.
3. Show the minimum necessary code.
4. Tell me exactly which files to create or modify.
5. Explain the code line by line where the Rust behavior may not be obvious to a Go developer.
6. Give me a concrete implementation task.
7. Wait for me to send my implementation.
8. Review it for:
   - correctness
   - idiomatic Rust
   - ownership/borrowing
   - async behavior
   - SQL correctness
   - architecture
9. Then move to the next stage.

Prefer teaching over code generation.

When several valid designs exist, explain the trade-offs instead of silently choosing one.

Do not hide complexity that matters for correctness, especially:

- concurrent transactions
- lock ordering
- idempotency races
- SQLx transaction ownership
- outbox delivery semantics

But also do not introduce complex abstractions before they are needed.

Start by helping me initialize the Rust project and implement **Stage 1**.
