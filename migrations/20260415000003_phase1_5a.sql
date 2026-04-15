-- Phase 1.5a: Bot application workflow and frontend support

-- Add status workflow fields to bots table
-- Replace boolean 'active' with richer status field
ALTER TABLE bots ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
ALTER TABLE bots ADD COLUMN submitted_by TEXT;
ALTER TABLE bots ADD COLUMN description TEXT;
ALTER TABLE bots ADD COLUMN reviewed_at TEXT;
ALTER TABLE bots ADD COLUMN reviewed_by TEXT;

-- Backfill: existing active bots get 'active' status, inactive get 'inactive'
UPDATE bots SET status = CASE WHEN active = 1 THEN 'active' ELSE 'inactive' END;

-- Index for filtering by status and submitted_by
CREATE INDEX idx_bots_status ON bots(status);
CREATE INDEX idx_bots_submitted_by ON bots(submitted_by);
