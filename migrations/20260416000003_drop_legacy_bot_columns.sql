-- Drop the legacy token_hash and active columns from bots.
--
-- token_hash was the SHA-256 hash of the bearer token used before PR #20
-- introduced AES-GCM encrypted storage (token_ciphertext). New code has
-- been writing '' to token_hash purely to satisfy the NOT NULL constraint
-- from the original 20260415000001_init.sql schema.
--
-- active was a boolean flag superseded by the richer status column added
-- in 20260415000003_phase1_5a.sql. transition_bot_status continued writing
-- to it until now so a rollback mid-rollout could still read sensible data.
-- One release has shipped; the column is now dead weight.
--
-- SQLite 3.35+ supports ALTER TABLE ... DROP COLUMN natively.

ALTER TABLE bots DROP COLUMN token_hash;
ALTER TABLE bots DROP COLUMN active;
