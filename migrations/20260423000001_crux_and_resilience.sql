-- 20260423000001_crux_and_resilience.sql
--
-- Adds per-response metadata to support abstention retry + R0 carry-forward.
--
-- retry_count        — 0 on first-attempt success; 1 when the orchestrator
--                       re-dispatched with a simplified prompt after an initial
--                       failure. Never exceeds 1.
-- fallback_from_round — NULL for normal responses. 0 when the response text is
--                       a carry-forward from the bot's round-0 response after
--                       two failed dispatch attempts in a later round.

ALTER TABLE responses ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE responses ADD COLUMN fallback_from_round INTEGER NULL;
