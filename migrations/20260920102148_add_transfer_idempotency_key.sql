ALTER TABLE transfers
ADD COLUMN idempotency_key TEXT;

UPDATE transfers
SET idempotency_key = 'legacy-' || id::text;

ALTER TABLE transfers
ALTER COLUMN idempotency_key SET NOT NULL;

ALTER TABLE transfers
ADD CONSTRAINT transfers_idempotency_key_unique
UNIQUE (idempotency_key);