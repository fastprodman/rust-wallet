-- Add migration script here
CREATE TABLE wallets (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    owner TEXT NOT NULL,
    balance BIGINT NOT NULL CHECK (balance >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);