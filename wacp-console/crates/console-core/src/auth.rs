use serde::{Deserialize, Serialize};

/// Console-level roles. admin ⊃ operator ⊃ viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleRole {
    Admin,
    Operator,
    Viewer,
}

impl ConsoleRole {
    /// Returns true if this role has at least the permissions of `required`.
    pub fn has_at_least(&self, required: ConsoleRole) -> bool {
        self.level() >= required.level()
    }

    fn level(&self) -> u8 {
        match self {
            ConsoleRole::Admin => 2,
            ConsoleRole::Operator => 1,
            ConsoleRole::Viewer => 0,
        }
    }
}

impl std::fmt::Display for ConsoleRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsoleRole::Admin => write!(f, "admin"),
            ConsoleRole::Operator => write!(f, "operator"),
            ConsoleRole::Viewer => write!(f, "viewer"),
        }
    }
}

impl std::str::FromStr for ConsoleRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(ConsoleRole::Admin),
            "operator" => Ok(ConsoleRole::Operator),
            "viewer" => Ok(ConsoleRole::Viewer),
            other => Err(format!("unknown console role: {other}")),
        }
    }
}

/// Identity of an authenticated user, extracted from session cookie or API token.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub username: String,
    pub console_role: ConsoleRole,
}
