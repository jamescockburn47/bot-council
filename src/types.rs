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
    Round0,
    Round1,
    Round2,
    Round3,
    Round4,
    Analysing,
    Synthesising,
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
            Self::Round0 => "round_0",
            Self::Round1 => "round_1",
            Self::Round2 => "round_2",
            Self::Round3 => "round_3",
            Self::Round4 => "round_4",
            Self::Analysing => "analysing",
            Self::Synthesising => "synthesising",
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
            "round_0" => Some(Self::Round0),
            "round_1" => Some(Self::Round1),
            "round_2" => Some(Self::Round2),
            "round_3" => Some(Self::Round3),
            "round_4" => Some(Self::Round4),
            "analysing" => Some(Self::Analysing),
            "synthesising" => Some(Self::Synthesising),
            "complete" => Some(Self::Complete),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Constitutional debate roles assigned to bots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Proponent,
    Skeptic,
    DevilsAdvocate,
    Empiricist,
    Steelman,
}

impl Role {
    /// All five constitutional roles.
    pub const ALL: [Role; 5] = [
        Role::Proponent,
        Role::Skeptic,
        Role::DevilsAdvocate,
        Role::Empiricist,
        Role::Steelman,
    ];

    /// Canonical string for database storage.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Proponent => "proponent",
            Self::Skeptic => "skeptic",
            Self::DevilsAdvocate => "devils_advocate",
            Self::Empiricist => "empiricist",
            Self::Steelman => "steelman",
        }
    }

    /// Parse from database string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "proponent" => Some(Self::Proponent),
            "skeptic" => Some(Self::Skeptic),
            "devils_advocate" => Some(Self::DevilsAdvocate),
            "empiricist" => Some(Self::Empiricist),
            "steelman" => Some(Self::Steelman),
            _ => None,
        }
    }

    /// Human-readable description of the role for prompt injection.
    pub fn description(&self) -> &str {
        match self {
            Self::Proponent => "Constructs the strongest case for the proposition",
            Self::Skeptic => "Challenges assumptions and demands evidence",
            Self::DevilsAdvocate => "Argues positions it may not hold to stress-test reasoning",
            Self::Empiricist => "Demands factual grounding, flags unsupported assertions",
            Self::Steelman => "Strengthens opposing arguments before engaging them",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Status of a single round within a debate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundStatus {
    Pending,
    InProgress,
    Complete,
    Failed,
}

impl RoundStatus {
    /// Canonical string for database storage.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }

    /// Parse from database string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "in_progress" => Some(Self::InProgress),
            "complete" => Some(Self::Complete),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}
