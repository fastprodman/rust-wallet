# Rust Wallet

Rust Wallet is a small HTTP API for creating wallets and transferring integer balance units between them. It is an educational project focused on transaction correctness and a hexagonal architecture in Rust.

## What this project demonstrates

- Axum HTTP handlers running on Tokio.
- A domain layer that does not depend on Axum, Serde, SQLx, or PostgreSQL.
- Application services that contain validation and transfer orchestration.
- Repository and transaction ports implemented with Rust traits and dynamic dispatch.
- PostgreSQL adapters implemented with SQLx compile-time checked queries.
- Atomic balance transfers using a database transaction.
- Deterministic `SELECT ... FOR UPDATE` wallet locking to reduce deadlock risk.
- Idempotent transfers backed by a unique idempotency key.
- Checked balance arithmetic and transaction rollback on failure.
- Docker Compose startup ordering for PostgreSQL, migrations, and the API.
- Graceful shutdown for Ctrl+C and container `SIGTERM` signals.

Balances are stored as integers. The application does not use floating-point values for money.

## Architecture

```text
HTTP adapter ──▶ application services ──▶ repository ports ◀── PostgreSQL adapter
                            │
                            ▼
                          domain

main.rs constructs the adapters and services and starts the server.
```

The HTTP adapter translates JSON and status codes. Services make application decisions. Ports describe the persistence operations required by the services, and the PostgreSQL adapter provides their SQLx implementations.

## Requirements

- Docker with Docker Compose
- `curl` for the examples below

## Run with Docker Compose

Build the images and run PostgreSQL, migrations, and the API:

```bash
docker compose up --build
```

Compose waits for PostgreSQL to become healthy, runs the migrations, and starts the API only after the migrations complete successfully.

To run everything in the background:

```bash
docker compose up --build --detach
docker compose logs --follow api
```

The API listens at `http://127.0.0.1:3000`.

## Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/health` | Check whether the API is running |
| `POST` | `/wallets` | Create a wallet |
| `GET` | `/wallets/{id}` | Get a wallet |
| `POST` | `/transfers` | Transfer balance between wallets |
| `GET` | `/transfers/{id}` | Get a transfer |

## Test with curl

The `-i` option includes the HTTP status and response headers.

### 1. Check health

```bash
curl -i http://127.0.0.1:3000/health
```

Expected status: `200 OK`.

### 2. Create two wallets

```bash
curl -i \
  -X POST \
  -H 'Content-Type: application/json' \
  --data '{"owner":"Alice","balance":10000}' \
  http://127.0.0.1:3000/wallets
```

```bash
curl -i \
  -X POST \
  -H 'Content-Type: application/json' \
  --data '{"owner":"Bob","balance":500}' \
  http://127.0.0.1:3000/wallets
```

Expected status for each request: `201 Created`.

Copy the returned wallet IDs into shell variables. Do not assume they are `1` and `2` when reusing an existing database volume.

```bash
ALICE_ID=1
BOB_ID=2
```

### 3. Read a wallet

```bash
curl -i "http://127.0.0.1:3000/wallets/${ALICE_ID}"
```

Expected status: `200 OK`.

### 4. Create a transfer

```bash
curl -i \
  -X POST \
  -H 'Content-Type: application/json' \
  --data-binary @- \
  http://127.0.0.1:3000/transfers <<JSON
{
  "from_wallet_id": ${ALICE_ID},
  "to_wallet_id": ${BOB_ID},
  "amount": 1000,
  "idempotency_key": "demo-transfer-1"
}
JSON
```

Expected status for the first request: `201 Created`. Copy the returned transfer ID:

```bash
TRANSFER_ID=1
```

### 5. Verify idempotency

Repeat the same logical transfer with the same idempotency key:

```bash
curl -i \
  -X POST \
  -H 'Content-Type: application/json' \
  --data-binary @- \
  http://127.0.0.1:3000/transfers <<JSON
{
  "from_wallet_id": ${ALICE_ID},
  "to_wallet_id": ${BOB_ID},
  "amount": 1000,
  "idempotency_key": "demo-transfer-1"
}
JSON
```

Expected status: `200 OK`. The original transfer is returned and balances are not changed again.

Reusing the key with different transfer parameters is rejected:

```bash
curl -i \
  -X POST \
  -H 'Content-Type: application/json' \
  --data-binary @- \
  http://127.0.0.1:3000/transfers <<JSON
{
  "from_wallet_id": ${ALICE_ID},
  "to_wallet_id": ${BOB_ID},
  "amount": 1001,
  "idempotency_key": "demo-transfer-1"
}
JSON
```

Expected status: `409 Conflict`.

### 6. Read the transfer and resulting balances

```bash
curl -i "http://127.0.0.1:3000/transfers/${TRANSFER_ID}"
curl -i "http://127.0.0.1:3000/wallets/${ALICE_ID}"
curl -i "http://127.0.0.1:3000/wallets/${BOB_ID}"
```

For the example values, Alice should have `9000` units and Bob should have `1500` units.

### 7. Try an unknown wallet

```bash
curl -i http://127.0.0.1:3000/wallets/999999
```

Expected status: `404 Not Found`.

## Run the API locally

PostgreSQL and the migration CLI can remain in Docker while the Rust API runs on the host:

```bash
docker compose up --detach postgres
docker compose run --rm migrate

export DATABASE_URL='postgres://wallet:wallet@127.0.0.1:5432/wallet?sslmode=disable'
cargo run
```

The default bind address for local execution is `127.0.0.1:3000`. Override it when necessary:

```bash
export BIND_ADDRESS='0.0.0.0:3000'
cargo run
```

## Stop the application

Stop all services and preserve the PostgreSQL data volume:

```bash
docker compose down
```

To observe graceful shutdown of only the API:

```bash
docker compose stop api
docker compose logs api
```

The API stops accepting new connections, allows active requests to finish, and has a 30-second grace period before Docker forces termination.
