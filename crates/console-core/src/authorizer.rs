//! Authorizer — role-based permission matrix.
//!
//! Spec: `wcon-auth` §4.2, `wcon-architecture` §8.3

use crate::auth::{AuthenticatedUser, ConsoleRole};
use crate::error::ConsoleError;

/// Actions that can be authorized. Maps to the 32-action matrix in `wcon-auth` §4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // Discovery
    BrowseTaxonomy,
    // Profiles
    ListOwnProfiles,
    ListAllProfiles,
    ViewOwnProfile,
    ViewAnyProfile,
    CreateProfile,
    EditOwnProfile,
    EditAnyProfile,
    DeleteOwnProfile,
    DeleteAnyProfile,
    ExportProfile,
    ImportProfile,
    // Sessions
    ListOwnSessions,
    ListAllSessions,
    ViewOwnOversight,
    ViewAnyOversight,
    CreateSession,
    CancelOwnSession,
    CancelAnySession,
    ApproveOwnGates,
    ApproveAnyGates,
    HandleOwnEscalations,
    HandleAnyEscalations,
    InjectOwnDirectives,
    InjectAnyDirectives,
    // Users
    ListUsers,
    CreateUser,
    DisableUser,
    ChangeUserRole,
    ResetUserPassword,
    // API Tokens
    ListOwnTokens,
    CreateOwnToken,
    RevokeOwnToken,
    ManageAnyTokens,
    // Audit
    ViewAuditLog,
    // Settings
    ViewSettings,
    ModifySettings,
}

/// Check if a user is authorized for an action.
/// Returns Ok(()) if allowed, Err(Forbidden) if not.
pub fn authorize(user: &AuthenticatedUser, action: Action) -> Result<(), ConsoleError> {
    if is_allowed(user.console_role, action) {
        Ok(())
    } else {
        Err(ConsoleError::Forbidden(format!(
            "{} cannot perform {:?}",
            user.console_role, action
        )))
    }
}

/// Check if a user can act on a resource they own.
/// Falls back to "any" permission if they don't own it.
pub fn authorize_owned(
    user: &AuthenticatedUser,
    own_action: Action,
    any_action: Action,
    resource_owner_id: &str,
) -> Result<(), ConsoleError> {
    if user.user_id == resource_owner_id {
        authorize(user, own_action)
    } else {
        authorize(user, any_action)
    }
}

fn is_allowed(role: ConsoleRole, action: Action) -> bool {
    use Action::*;
    use ConsoleRole::*;

    match action {
        // Everyone
        BrowseTaxonomy | ListOwnProfiles | ViewOwnProfile | ExportProfile
        | ListOwnSessions | ViewOwnOversight | ListOwnTokens => true,

        // Operator+
        CreateProfile | EditOwnProfile | DeleteOwnProfile | ImportProfile
        | CreateSession | CancelOwnSession | ApproveOwnGates
        | HandleOwnEscalations | InjectOwnDirectives
        | CreateOwnToken | RevokeOwnToken | ViewSettings => {
            matches!(role, Operator | Admin)
        }

        // Admin only
        ListAllProfiles | ViewAnyProfile | EditAnyProfile | DeleteAnyProfile
        | ListAllSessions | ViewAnyOversight | CancelAnySession
        | ApproveAnyGates | HandleAnyEscalations | InjectAnyDirectives
        | ListUsers | CreateUser | DisableUser | ChangeUserRole | ResetUserPassword
        | ManageAnyTokens | ViewAuditLog | ModifySettings => {
            matches!(role, Admin)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(role: ConsoleRole) -> AuthenticatedUser {
        AuthenticatedUser {
            user_id: "test".into(),
            username: "test".into(),
            console_role: role,
        }
    }

    #[test]
    fn viewer_can_browse() {
        authorize(&user(ConsoleRole::Viewer), Action::BrowseTaxonomy).unwrap();
        authorize(&user(ConsoleRole::Viewer), Action::ListOwnProfiles).unwrap();
        authorize(&user(ConsoleRole::Viewer), Action::ListOwnTokens).unwrap();
    }

    #[test]
    fn viewer_cannot_create() {
        assert!(authorize(&user(ConsoleRole::Viewer), Action::CreateProfile).is_err());
        assert!(authorize(&user(ConsoleRole::Viewer), Action::CreateSession).is_err());
    }

    #[test]
    fn operator_can_create_and_manage_own() {
        let op = user(ConsoleRole::Operator);
        authorize(&op, Action::CreateProfile).unwrap();
        authorize(&op, Action::EditOwnProfile).unwrap();
        authorize(&op, Action::CreateSession).unwrap();
        authorize(&op, Action::CancelOwnSession).unwrap();
        authorize(&op, Action::ViewSettings).unwrap();
    }

    #[test]
    fn operator_cannot_manage_others() {
        let op = user(ConsoleRole::Operator);
        assert!(authorize(&op, Action::EditAnyProfile).is_err());
        assert!(authorize(&op, Action::CancelAnySession).is_err());
        assert!(authorize(&op, Action::ListUsers).is_err());
        assert!(authorize(&op, Action::ModifySettings).is_err());
    }

    #[test]
    fn admin_can_do_everything() {
        let admin = user(ConsoleRole::Admin);
        let all_actions = [
            Action::BrowseTaxonomy, Action::ListAllProfiles, Action::ViewAnyProfile,
            Action::EditAnyProfile, Action::DeleteAnyProfile, Action::ListAllSessions,
            Action::ViewAnyOversight, Action::CancelAnySession, Action::ListUsers,
            Action::CreateUser, Action::DisableUser, Action::ChangeUserRole,
            Action::ResetUserPassword, Action::ManageAnyTokens, Action::ViewAuditLog,
            Action::ModifySettings,
        ];
        for action in all_actions {
            authorize(&admin, action).unwrap_or_else(|_| panic!("admin should be allowed {action:?}"));
        }
    }

    #[test]
    fn authorize_owned_uses_own_action_for_owner() {
        let mut u = user(ConsoleRole::Operator);
        u.user_id = "owner1".into();
        authorize_owned(&u, Action::EditOwnProfile, Action::EditAnyProfile, "owner1").unwrap();
    }

    #[test]
    fn authorize_owned_falls_back_to_any_for_non_owner() {
        let mut u = user(ConsoleRole::Operator);
        u.user_id = "owner1".into();
        assert!(authorize_owned(&u, Action::EditOwnProfile, Action::EditAnyProfile, "owner2").is_err());
    }
}
