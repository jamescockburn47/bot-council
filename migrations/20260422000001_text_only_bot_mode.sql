-- Adds text-only bot mode. Spec: docs/superpowers/specs/2026-04-22-text-only-bot-mode-design.md
-- `bot_kind` gates dispatch + smoke-test behaviour. Default 'external' preserves
-- the legacy contract for existing bots without any data fix-up.
-- `introduction` is populated during approval smoke test for text_only bots.
ALTER TABLE bots ADD COLUMN bot_kind TEXT NOT NULL DEFAULT 'external';
ALTER TABLE bots ADD COLUMN introduction TEXT;

-- Per-field extraction provenance, shown in the transcript UI.
-- JSON shape: { "challenge": {"source": "extracted", "quote": "..."}, "position_change": {...} }
-- NULL for rows belonging to external-mode bots (no extraction ever runs).
ALTER TABLE responses ADD COLUMN extraction_metadata TEXT;
