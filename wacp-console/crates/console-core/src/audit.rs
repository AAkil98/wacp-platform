//! Audit service — append-only mutation log.
//!
//! Spec: `wcon-auth` §10

use console_db::DbPool;
use console_db::queries::audit_log;

use crate::error::ConsoleError;

/// All auditable action types from `wcon-auth` §10.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditAction {
    UserCreate,
    UserDisable,
    UserEnable,
    UserChangeRole,
    UserResetPassword,
    AuthLogin,
    AuthLogout,
    AuthChangePassword,
    TokenCreate,
    TokenRevoke,
    ProfileCreate,
    ProfileUpdate,
    ProfileDelete,
    ProfileClone,
    ProfileImport,
    ProfileVisibilityChange,
    SessionCreate,
    SessionLaunch,
    SessionCancel,
    SessionGateApprove,
    SessionGateReject,
    SessionEscalationRespond,
    SessionInjectDirective,
    SettingsUpdate,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserCreate => "user.create",
            Self::UserDisable => "user.disable",
            Self::UserEnable => "user.enable",
            Self::UserChangeRole => "user.change_role",
            Self::UserResetPassword => "user.reset_password",
            Self::AuthLogin => "auth.login",
            Self::AuthLogout => "auth.logout",
            Self::AuthChangePassword => "auth.change_password",
            Self::TokenCreate => "token.create",
            Self::TokenRevoke => "token.revoke",
            Self::ProfileCreate => "profile.create",
            Self::ProfileUpdate => "profile.update",
            Self::ProfileDelete => "profile.delete",
            Self::ProfileClone => "profile.clone",
            Self::ProfileImport => "profile.import",
            Self::ProfileVisibilityChange => "profile.visibility_change",
            Self::SessionCreate => "session.create",
            Self::SessionLaunch => "session.launch",
            Self::SessionCancel => "session.cancel",
            Self::SessionGateApprove => "session.gate_approve",
            Self::SessionGateReject => "session.gate_reject",
            Self::SessionEscalationRespond => "session.escalation_respond",
            Self::SessionInjectDirective => "session.inject_directive",
            Self::SettingsUpdate => "settings.update",
        }
    }

    pub fn target_kind(&self) -> &'static str {
        match self {
            Self::UserCreate
            | Self::UserDisable
            | Self::UserEnable
            | Self::UserChangeRole
            | Self::UserResetPassword
            | Self::AuthLogin
            | Self::AuthLogout
            | Self::AuthChangePassword => "user",

            Self::TokenCreate | Self::TokenRevoke => "token",

            Self::ProfileCreate
            | Self::ProfileUpdate
            | Self::ProfileDelete
            | Self::ProfileClone
            | Self::ProfileImport
            | Self::ProfileVisibilityChange => "profile",

            Self::SessionCreate
            | Self::SessionLaunch
            | Self::SessionCancel
            | Self::SessionGateApprove
            | Self::SessionGateReject
            | Self::SessionEscalationRespond
            | Self::SessionInjectDirective => "session",

            Self::SettingsUpdate => "settings",
        }
    }
}

/// Context for an audit entry.
pub struct AuditEntry {
    pub user_id: String,
    pub action: AuditAction,
    pub target_id: String,
    pub detail: Option<serde_json::Value>,
    pub ip: String,
    pub user_agent: String,
}

/// Write an audit entry. This is append-only — no UPDATE or DELETE.
pub async fn log_audit(pool: &DbPool, entry: AuditEntry) -> Result<(), ConsoleError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let detail_str = entry.detail.map(|d| d.to_string());

    audit_log::insert_entry(
        pool,
        &id,
        &entry.user_id,
        &now,
        entry.action.as_str(),
        entry.action.target_kind(),
        &entry.target_id,
        detail_str.as_deref(),
        &entry.ip,
        &entry.user_agent,
    )
    .await
    .map_err(|e| ConsoleError::Database(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_24_actions_have_distinct_strings() {
        let actions = [
            AuditAction::UserCreate,
            AuditAction::UserDisable,
            AuditAction::UserEnable,
            AuditAction::UserChangeRole,
            AuditAction::UserResetPassword,
            AuditAction::AuthLogin,
            AuditAction::AuthLogout,
            AuditAction::AuthChangePassword,
            AuditAction::TokenCreate,
            AuditAction::TokenRevoke,
            AuditAction::ProfileCreate,
            AuditAction::ProfileUpdate,
            AuditAction::ProfileDelete,
            AuditAction::ProfileClone,
            AuditAction::ProfileImport,
            AuditAction::ProfileVisibilityChange,
            AuditAction::SessionCreate,
            AuditAction::SessionLaunch,
            AuditAction::SessionCancel,
            AuditAction::SessionGateApprove,
            AuditAction::SessionGateReject,
            AuditAction::SessionEscalationRespond,
            AuditAction::SessionInjectDirective,
            AuditAction::SettingsUpdate,
        ];

        let mut strings: Vec<&str> = actions.iter().map(|a| a.as_str()).collect();
        let total = strings.len();
        strings.sort();
        strings.dedup();
        assert_eq!(strings.len(), total, "all action strings must be unique");
    }

    #[test]
    fn target_kinds_are_valid() {
        let valid = ["user", "token", "profile", "session", "settings"];
        let actions = [
            AuditAction::UserCreate,
            AuditAction::TokenCreate,
            AuditAction::ProfileCreate,
            AuditAction::SessionCreate,
            AuditAction::SettingsUpdate,
        ];
        for action in &actions {
            assert!(valid.contains(&action.target_kind()));
        }
    }
}
