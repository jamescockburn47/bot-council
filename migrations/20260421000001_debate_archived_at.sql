-- Soft-delete column for admin-facing debate archival.
-- NULL = live; ISO-8601 timestamp = archived at that moment.
-- Permanent delete goes through `cascade_delete_debate` in queries_cleanup
-- and does not use this column.
ALTER TABLE debates ADD COLUMN archived_at TEXT;
