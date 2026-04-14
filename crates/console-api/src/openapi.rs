//! OpenAPI specification generation.
//!
//! Spec: ADR-008
//!
//! Builds the OpenAPI 3.1.0 spec as structured JSON, serialized to YAML.
//! Handlers use dynamic serde_json::Value responses, so schemas are opaque.

/// Generate the OpenAPI YAML specification string.
pub fn generate_openapi_yaml() -> String {
    let spec = build_spec();
    serde_yaml_ng::to_string(&spec).expect("failed to serialize OpenAPI spec to YAML")
}

fn build_spec() -> serde_json::Value {
    let mut paths = serde_json::Map::new();

    // Phase 1 endpoints
    add_op(&mut paths, "/api/auth/login", "post", "auth", "Login", &[200]);
    add_op(&mut paths, "/api/auth/logout", "post", "auth", "Logout", &[204, 401]);
    add_op(&mut paths, "/api/auth/whoami", "get", "auth", "Get current user", &[200, 401]);
    add_op(&mut paths, "/api/auth/change-password", "post", "auth", "Change password", &[204, 401]);

    add_op(&mut paths, "/api/users", "get", "users", "List users", &[200, 401, 403]);
    add_op(&mut paths, "/api/users", "post", "users", "Create user", &[201, 401, 403]);
    add_op(&mut paths, "/api/users/{id}", "get", "users", "Get user", &[200, 401, 403]);
    add_op(&mut paths, "/api/users/{id}", "patch", "users", "Update user", &[204, 401, 403]);
    add_op(&mut paths, "/api/users/{id}/reset-password", "post", "users", "Reset user password", &[204, 401, 403]);
    add_op(&mut paths, "/api/users/{id}/unlock", "post", "users", "Unlock user", &[204, 401, 403]);

    add_op(&mut paths, "/api/tokens", "get", "tokens", "List own tokens", &[200, 401]);
    add_op(&mut paths, "/api/tokens", "post", "tokens", "Create token", &[201, 401]);
    add_op(&mut paths, "/api/tokens/{id}", "delete", "tokens", "Revoke token", &[204, 401]);
    add_op(&mut paths, "/api/users/{user_id}/tokens", "get", "tokens", "List user tokens (admin)", &[200, 401, 403]);
    add_op(&mut paths, "/api/users/{user_id}/tokens/{id}", "delete", "tokens", "Revoke user token (admin)", &[204, 401, 403]);

    add_op(&mut paths, "/api/audit-log", "get", "audit", "List audit log", &[200, 401, 403]);

    add_op(&mut paths, "/api/settings", "get", "settings", "Get all settings", &[200, 401, 403]);
    add_op(&mut paths, "/api/settings/{key}", "get", "settings", "Get setting", &[200, 401, 403]);
    add_op(&mut paths, "/api/settings/{key}", "put", "settings", "Set setting", &[204, 401, 403]);
    add_op(&mut paths, "/api/settings/{key}", "delete", "settings", "Delete setting", &[204, 401, 403]);

    add_op(&mut paths, "/api/health", "get", "health", "Health check", &[200]);

    // Phase 2 endpoints
    add_op(&mut paths, "/api/roles", "get", "discovery", "List roles", &[200, 401, 403]);
    add_op(&mut paths, "/api/roles/{id}", "get", "discovery", "Get role", &[200, 401, 403]);
    add_op(&mut paths, "/api/tools", "get", "discovery", "List tools", &[200, 401, 403]);
    add_op(&mut paths, "/api/tools/{name}", "get", "discovery", "Get tool", &[200, 401, 403]);
    add_op(&mut paths, "/api/verticals", "get", "discovery", "List verticals", &[200, 401, 403]);
    add_op(&mut paths, "/api/verticals/{id}", "get", "discovery", "Get vertical", &[200, 401, 403]);
    add_op(&mut paths, "/api/envelope-types", "get", "discovery", "List envelope types", &[200, 401, 403]);
    add_op(&mut paths, "/api/envelope-types/{name}", "get", "discovery", "Get envelope type", &[200, 401, 403]);
    add_op(&mut paths, "/api/checkpoint-types", "get", "discovery", "List checkpoint types", &[200, 401, 403]);
    add_op(&mut paths, "/api/checkpoint-types/{name}", "get", "discovery", "Get checkpoint type", &[200, 401, 403]);

    add_op(&mut paths, "/api/verticals/{id}/workflows", "get", "verticals", "List workflows", &[200, 401, 403]);
    add_op(&mut paths, "/api/verticals/{id}/workflows/{wf_id}", "get", "verticals", "Get workflow", &[200, 401, 403]);
    add_op(&mut paths, "/api/verticals/{id}/task-types", "get", "verticals", "List task types", &[200, 401, 403]);
    add_op(&mut paths, "/api/verticals/{id}/context-schema", "get", "verticals", "Get context schema", &[200, 401, 403]);
    add_op(&mut paths, "/api/verticals/{id}/tool-policies", "get", "verticals", "Get tool policies", &[200, 401, 403]);
    add_op(&mut paths, "/api/verticals/{id}/checkpoint-types", "get", "verticals", "Get vertical checkpoint types", &[200, 401, 403]);
    add_op(&mut paths, "/api/verticals/{id}/quality-criteria", "get", "verticals", "Get quality criteria", &[200, 401, 403]);

    // Phase 3: Profiles
    add_op(&mut paths, "/api/profiles", "get", "profiles", "List profiles", &[200, 401]);
    add_op(&mut paths, "/api/profiles", "post", "profiles", "Create profile", &[201, 401, 403]);
    add_op(&mut paths, "/api/profiles/{id}", "get", "profiles", "Get profile", &[200, 401, 403]);
    add_op(&mut paths, "/api/profiles/{id}", "put", "profiles", "Update profile", &[200, 401, 403]);
    add_op(&mut paths, "/api/profiles/{id}", "delete", "profiles", "Delete profile", &[200, 401, 403]);
    add_op(&mut paths, "/api/profiles/{id}/versions", "get", "profiles", "List profile versions", &[200, 401]);
    add_op(&mut paths, "/api/profiles/{id}/rollback", "post", "profiles", "Rollback profile", &[200, 401, 403]);
    add_op(&mut paths, "/api/profiles/{id}/clone", "post", "profiles", "Clone profile", &[201, 401, 403]);
    add_op(&mut paths, "/api/profiles/{id}/export", "get", "profiles", "Export profile (YAML)", &[200, 401]);
    add_op(&mut paths, "/api/profiles/import", "post", "profiles", "Import profile (YAML)", &[201, 401, 403]);
    // Phase 4: Sessions
    add_op(&mut paths, "/api/sessions", "get", "sessions", "List sessions", &[200, 401]);
    add_op(&mut paths, "/api/sessions", "post", "sessions", "Create session", &[201, 401, 403]);
    add_op(&mut paths, "/api/sessions/{id}", "get", "sessions", "Get session", &[200, 401, 403]);
    add_op(&mut paths, "/api/sessions/{id}", "patch", "sessions", "Update session config", &[204, 401, 403]);
    add_op(&mut paths, "/api/sessions/{id}/assignments", "put", "sessions", "Set assignments", &[204, 401, 403]);
    add_op(&mut paths, "/api/sessions/{id}/launch", "post", "sessions", "Launch session", &[200, 401, 403]);
    add_op(&mut paths, "/api/sessions/{id}/cancel", "post", "sessions", "Cancel session", &[200, 401, 403]);
    add_op(&mut paths, "/api/sessions/{id}/clone", "post", "sessions", "Clone session", &[201, 401, 403]);
    // Phase 4: Highway actions
    add_op(&mut paths, "/api/sessions/{sid}/gates/{gid}", "post", "highway", "Resolve gate", &[204, 401, 403]);
    add_op(&mut paths, "/api/sessions/{sid}/gates/batch-resolve", "post", "highway", "Batch resolve gates", &[200, 401, 403]);
    add_op(&mut paths, "/api/sessions/{sid}/escalations/{eid}", "post", "highway", "Respond to escalation", &[204, 401, 403]);
    add_op(&mut paths, "/api/sessions/{sid}/inject", "post", "highway", "Inject directive", &[204, 401, 403]);
    // Phase 4: Cross-session
    add_op(&mut paths, "/api/gates/pending", "get", "highway", "Pending gates", &[200, 401]);
    add_op(&mut paths, "/api/escalations/pending", "get", "highway", "Pending escalations", &[200, 401]);
    add_op(&mut paths, "/api/refusals/pending", "get", "highway", "Pending refusals", &[200, 401]);
    // Phase 4: WebSocket
    add_op(&mut paths, "/api/sessions/{id}/ws", "get", "sessions", "WebSocket upgrade", &[101, 401, 403]);
    // Search + Taxonomy
    add_op(&mut paths, "/api/search", "get", "search", "Search taxonomy", &[200, 401, 403]);
    add_op(&mut paths, "/api/taxonomy/reload", "post", "taxonomy", "Reload taxonomy", &[200, 401, 403]);

    serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "WACP Console API",
            "version": "0.1.0",
            "description": "REST API for the WACP Console — coordination workbench for the WACP ecosystem.",
            "license": { "name": "Apache-2.0" }
        },
        "paths": serde_json::Value::Object(paths),
        "tags": [
            { "name": "auth", "description": "Authentication and session management" },
            { "name": "users", "description": "User management (admin)" },
            { "name": "tokens", "description": "API token management" },
            { "name": "audit", "description": "Audit log" },
            { "name": "settings", "description": "Application settings" },
            { "name": "health", "description": "Health checks" },
            { "name": "sessions", "description": "Session lifecycle and coordination" },
            { "name": "highway", "description": "Gates, escalations, refusals, directives" },
            { "name": "profiles", "description": "Agent profile lifecycle" },
            { "name": "discovery", "description": "Taxonomy discovery and browsing" },
            { "name": "verticals", "description": "Per-vertical sub-resources" },
            { "name": "search", "description": "Cross-entity search" },
            { "name": "taxonomy", "description": "Taxonomy management" },
        ]
    })
}

fn add_op(
    paths: &mut serde_json::Map<String, serde_json::Value>,
    path: &str,
    method: &str,
    tag: &str,
    summary: &str,
    status_codes: &[u16],
) {
    let mut responses = serde_json::Map::new();
    for &code in status_codes {
        let desc = match code {
            200 => "Success",
            201 => "Created",
            204 => "No content",
            401 => "Unauthenticated",
            403 => "Forbidden",
            404 => "Not found",
            _ => "Response",
        };
        responses.insert(
            code.to_string(),
            serde_json::json!({ "description": desc }),
        );
    }

    let operation = serde_json::json!({
        "tags": [tag],
        "summary": summary,
        "responses": serde_json::Value::Object(responses),
    });

    let entry = paths
        .entry(path.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let serde_json::Value::Object(map) = entry {
        map.insert(method.to_string(), operation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_yaml() {
        let yaml = generate_openapi_yaml();
        assert!(yaml.contains("openapi: 3.1.0"));
        assert!(yaml.contains("WACP Console API"));
    }

    #[test]
    fn all_40_endpoints_present() {
        let spec = build_spec();
        let paths = spec["paths"].as_object().unwrap();
        // Count total operations across all paths
        let op_count: usize = paths.values().map(|p| {
            p.as_object().map_or(0, |m| m.len())
        }).sum();
        assert_eq!(op_count, 66, "expected 66 operations, got {op_count}");
    }
}
