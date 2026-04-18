-- Phase 1: add error classification to responses so `/bots/{id}/history`
-- can surface per-bot failure patterns and Clint's `lqc_bot_diagnose`
-- tool has structured data to aggregate on.
--
-- Closed-set taxonomy for error_kind (stringly-typed for simplicity):
--   timeout | http_5xx | http_4xx | connection_refused | dns | tls |
--   json_parse | schema_missing_field | schema_invalid_type |
--   schema_invalid_value | late_response | internal
--
-- Columns are nullable on existing rows; populated by the orchestrator
-- on abstention or validation failure.
ALTER TABLE responses ADD COLUMN error_kind TEXT NULL;
ALTER TABLE responses ADD COLUMN error_detail TEXT NULL;
ALTER TABLE responses ADD COLUMN elapsed_ms INTEGER NULL;

CREATE INDEX IF NOT EXISTS ix_responses_bot_error
    ON responses(bot_id, error_kind);
