-- Adds encrypted-token storage and rejection-reason feedback loop.
-- Retains token_hash and active columns; a follow-up migration will drop
-- them after one release when all rows are confirmed on the new path.

ALTER TABLE bots ADD COLUMN token_ciphertext BLOB;
ALTER TABLE bots ADD COLUMN rejection_reason TEXT;

CREATE INDEX idx_bots_status_reviewable
    ON bots(status)
    WHERE status IN ('pending', 'smoke_test_failed');
