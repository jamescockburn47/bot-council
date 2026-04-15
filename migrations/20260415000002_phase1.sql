-- Round state tracking (resumable state machine)
CREATE TABLE IF NOT EXISTS rounds (
    debate_id TEXT NOT NULL REFERENCES debates(id),
    round_number INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    started_at TEXT,
    completed_at TEXT,
    PRIMARY KEY (debate_id, round_number)
);

-- Add role column to debate_bots (nullable for Phase 0 backward compat)
ALTER TABLE debate_bots ADD COLUMN role TEXT;

-- Add Phase 1 columns to responses (all nullable for backward compat)
ALTER TABLE responses ADD COLUMN confidence INTEGER;
ALTER TABLE responses ADD COLUMN challenge_json TEXT;
ALTER TABLE responses ADD COLUMN position_change_json TEXT;
ALTER TABLE responses ADD COLUMN valid INTEGER NOT NULL DEFAULT 1;
ALTER TABLE responses ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;

-- Analysis results (challenge validation, divergence, pairing)
CREATE TABLE IF NOT EXISTS analyses (
    id TEXT PRIMARY KEY,
    debate_id TEXT NOT NULL REFERENCES debates(id),
    bot_id TEXT,
    analysis_type TEXT NOT NULL,
    input_json TEXT NOT NULL,
    result_json TEXT NOT NULL,
    model_used TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Cross-examination pairings
CREATE TABLE IF NOT EXISTS pairings (
    debate_id TEXT NOT NULL REFERENCES debates(id),
    bot_a_id TEXT NOT NULL REFERENCES bots(id),
    bot_b_id TEXT NOT NULL REFERENCES bots(id),
    third_id TEXT REFERENCES bots(id),
    pairing_json TEXT NOT NULL,
    PRIMARY KEY (debate_id, bot_a_id, bot_b_id)
);

-- Final synthesis output
CREATE TABLE IF NOT EXISTS syntheses (
    debate_id TEXT PRIMARY KEY REFERENCES debates(id),
    output_json TEXT NOT NULL,
    model_used TEXT NOT NULL,
    prompt_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Role rotation history (prevents same role in consecutive debates)
CREATE TABLE IF NOT EXISTS role_history (
    bot_id TEXT NOT NULL REFERENCES bots(id),
    debate_id TEXT NOT NULL REFERENCES debates(id),
    role TEXT NOT NULL,
    PRIMARY KEY (bot_id, debate_id)
);
