-- The ship's log (operator-legibility spec Part 1): every operationally
-- significant event as a plain-English journal entry the owner can read,
-- with the technical handles an AI agent needs folded into
-- technical_detail. Narratives are authored templates
-- (src/observability/system_guidance.rs) — never model-generated.
CREATE TABLE system_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    severity TEXT NOT NULL,          -- 'info' | 'attention' | 'problem'
    event_kind TEXT NOT NULL,        -- stable, greppable (see catalogue)
    narrative TEXT NOT NULL,         -- plain English, authored template
    suggested_action TEXT NULL,      -- plain English, when one exists
    technical_detail TEXT NULL,      -- JSON: IDs, routes, raw error strings
    debate_id TEXT NULL,             -- when event is debate-scoped
    bot_id TEXT NULL                 -- when event is bot-scoped
);
CREATE INDEX idx_system_events_created ON system_events(created_at);
