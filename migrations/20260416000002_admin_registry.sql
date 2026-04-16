-- In-app admin registry.
-- Replaces the config.auth.admin_user_ids allowlist so admins can be promoted
-- and demoted at runtime without a redeploy.

CREATE TABLE IF NOT EXISTS admins (
    user_id TEXT PRIMARY KEY,
    granted_at TEXT NOT NULL DEFAULT (datetime('now')),
    granted_by TEXT
);

-- Log of every Clerk user that has authenticated at least once. Used by the
-- /admins UI to offer a pick-list of promotable users. Best-effort upsert on
-- every authenticated request; auth never fails because of a seen_users error.
CREATE TABLE IF NOT EXISTS seen_users (
    user_id TEXT PRIMARY KEY,
    first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
);
