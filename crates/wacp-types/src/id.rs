use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! define_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

define_id!(WorkspaceId, "Unique identifier for a workspace.");
define_id!(EnvelopeId, "Unique identifier for an envelope.");
define_id!(CheckpointId, "Unique identifier for a checkpoint.");
define_id!(TaskId, "Unique identifier for a task.");
define_id!(TrailEntryId, "Unique identifier for a trail entry.");
define_id!(UserId, "Authenticated user identifier.");
define_id!(GateId, "Unique identifier for a gate event.");
define_id!(EscalationId, "Unique identifier for an escalation.");
