CREATE TABLE IF NOT EXISTS bots (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    endpoint_url TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    model_family TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS debates (
    id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'created',
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE TABLE IF NOT EXISTS debate_bots (
    debate_id TEXT NOT NULL REFERENCES debates(id),
    bot_id TEXT NOT NULL REFERENCES bots(id),
    pseudonym TEXT NOT NULL,
    PRIMARY KEY (debate_id, bot_id)
);

CREATE TABLE IF NOT EXISTS responses (
    id TEXT PRIMARY KEY,
    debate_id TEXT NOT NULL REFERENCES debates(id),
    round_number INTEGER NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id),
    response_json TEXT NOT NULL,
    abstained INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS peer_scores (
    id TEXT PRIMARY KEY,
    debate_id TEXT NOT NULL REFERENCES debates(id),
    scorer_bot_id TEXT NOT NULL REFERENCES bots(id),
    target_pseudonym TEXT NOT NULL,
    reasoning_quality INTEGER NOT NULL,
    factual_grounding INTEGER NOT NULL,
    overall INTEGER NOT NULL,
    reasoning TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
