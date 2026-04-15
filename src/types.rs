use serde::{Deserialize, Serialize};
use std::fmt;

/// Newtype wrapper for debate IDs. Prevents mixing with other string IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DebateId(pub String);

impl DebateId {
    /// Generate a new random debate ID.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DebateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Newtype wrapper for bot IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BotId(pub String);

impl BotId {
    /// Generate a new random bot ID.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Debate status enum. Used in DB and API responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebateStatus {
    Created,
    Dispatching,
    Scoring,
    Complete,
    Cancelled,
    Failed,
}

impl DebateStatus {
    /// Return the canonical string representation stored in the database.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Created => "created",
            Self::Dispatching => "dispatching",
            Self::Scoring => "scoring",
            Self::Complete => "complete",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    /// Parse a status string from the database. Returns None for unknown values.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "created" => Some(Self::Created),
            "dispatching" => Some(Self::Dispatching),
            "scoring" => Some(Self::Scoring),
            "complete" => Some(Self::Complete),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}
