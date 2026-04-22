-- 20260423000001_crux_and_resilience.sql
--
-- Adds per-response metadata for R0 carry-forward resilience.
--
-- fallback_from_round — NULL for normal responses. 0 when the response text is
--                       a carry-forward from the bot's round-0 response after
--                       failed dispatch attempts in a later round.
--
-- The retry_count column already exists (added in 20260415000002_phase1.sql for
-- round 2's rejection-reprompt counter); it is reused here by the unified
-- dispatch-with-retry helper introduced in the five-round redesign.

ALTER TABLE responses ADD COLUMN fallback_from_round INTEGER NULL;
