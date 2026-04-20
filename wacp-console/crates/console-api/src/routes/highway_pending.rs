use super::*;
use crate::pagination::{PaginationParams, encode_cursor};
use console_core::ConsoleRole;
use console_core::event_enricher::{EnrichedEscalation, EnrichedGate};
use console_core::refusal_synthesizer::Refusal;
use console_db::queries::sessions as session_queries;
use serde::Serialize;

/// Page envelope returned by all three pending endpoints. Mirrors
/// `pagination::PaginatedResponse` but adds an explicit `session_id` per
/// item so frontends can group / link without an extra lookup.
#[derive(Debug, Serialize)]
pub struct PendingPage<T: Serialize + Clone + std::fmt::Debug> {
    pub items: Vec<PendingItem<T>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct PendingItem<T: Serialize + Clone + std::fmt::Debug> {
    pub session_id: String,
    #[serde(flatten)]
    pub inner: T,
}

/// Sort key derived from `(session_id, item_id)`. Cursor is a base64 of
/// this string (encoded once for the page tail).
fn sort_key(session_id: &str, item_id: &str) -> String {
    format!("{session_id}\x1f{item_id}")
}

/// Resolve the caller's ownership scope: which session ids may they see?
/// - Admin → unrestricted (returns None).
/// - Otherwise → the set of session ids they own (looked up once via DB).
async fn owned_sessions(
    state: &AppState,
    auth: &Auth,
) -> Result<Option<std::collections::HashSet<String>>, ApiError> {
    if auth.console_role == ConsoleRole::Admin {
        return Ok(None);
    }
    let rows = session_queries::list_by_owner(&state.db, &auth.user_id, None, 1000, None)
        .await
        .map_err(|e| ApiError::from(ConsoleError::Database(e.to_string())))?;
    Ok(Some(rows.into_iter().map(|r| r.id).collect()))
}

/// Validate an explicit `session_id` filter against the caller's scope.
/// Non-admin asking for a session they don't own → 403, by spec §4.2
/// ("don't silently return empty — make the authz failure explicit").
fn enforce_session_scope(
    owned: &Option<std::collections::HashSet<String>>,
    sid: &str,
) -> Result<(), ApiError> {
    match owned {
        None => Ok(()),
        Some(set) if set.contains(sid) => Ok(()),
        Some(_) => Err(ApiError::from(ConsoleError::Forbidden(
            "Cannot view pending items for a session you do not own".into(),
        ))),
    }
}

fn build_page<T: Serialize + Clone + std::fmt::Debug>(
    items: Vec<PendingItem<T>>,
    params: PaginationParams,
) -> PendingPage<T> {
    let limit = params.effective_limit();
    let cursor_value = params.decode_cursor();

    let mut filtered: Vec<&PendingItem<T>> = items
        .iter()
        .filter(|item| match cursor_value.as_ref() {
            Some(c) => {
                // The page-keys are sort_key(sid, item_id); tail item's
                // key is what the cursor stores. Skip rows ≤ cursor.
                item_sort_key(item).as_str() > c.as_str()
            }
            None => true,
        })
        .collect();
    filtered.sort_by_key(|a| item_sort_key(a));

    let has_more = filtered.len() > limit;
    let page: Vec<PendingItem<T>> = filtered.into_iter().take(limit).cloned().collect();
    let cursor = if has_more {
        page.last().map(|item| encode_cursor(&item_sort_key(item)))
    } else {
        None
    };
    PendingPage {
        items: page,
        cursor,
        has_more,
    }
}

fn item_sort_key<T: Serialize + Clone + std::fmt::Debug>(item: &PendingItem<T>) -> String {
    // Round-trip via JSON to extract the canonical id field. Each kind
    // (gate / escalation / refusal) has a different name; use their
    // serialized shape to pick the first non-empty id.
    let v = serde_json::to_value(&item.inner).unwrap_or(serde_json::Value::Null);
    let id = ["gate_id", "escalation_id", "id"]
        .iter()
        .find_map(|k| v.get(k).and_then(|x| x.as_str()).map(String::from))
        .unwrap_or_default();
    sort_key(&item.session_id, &id)
}

pub async fn aggregate_gates(
    state: &AppState,
    auth: &Auth,
    params: PendingParams,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owned = owned_sessions(state, auth).await?;
    if let Some(sid) = params.session_id.as_deref() {
        enforce_session_scope(&owned, sid)?;
    }

    let mut items: Vec<PendingItem<EnrichedGate>> = Vec::new();
    let active = state.active_sessions.read().await;
    for (sid, handle) in active.iter() {
        if !visible(&owned, sid) {
            continue;
        }
        if let Some(scope) = params.session_id.as_deref()
            && scope != sid
        {
            continue;
        }
        let gates = handle.pending.gates.read().await;
        for g in gates.iter() {
            items.push(PendingItem {
                session_id: sid.clone(),
                inner: g.clone(),
            });
        }
    }
    drop(active);

    let page = build_page(
        items,
        PaginationParams {
            limit: params.limit,
            cursor: params.cursor,
        },
    );
    Ok(Json(serde_json::to_value(page).unwrap_or_default()))
}

pub async fn aggregate_escalations(
    state: &AppState,
    auth: &Auth,
    params: PendingParams,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owned = owned_sessions(state, auth).await?;
    if let Some(sid) = params.session_id.as_deref() {
        enforce_session_scope(&owned, sid)?;
    }

    let mut items: Vec<PendingItem<EnrichedEscalation>> = Vec::new();
    let active = state.active_sessions.read().await;
    for (sid, handle) in active.iter() {
        if !visible(&owned, sid) {
            continue;
        }
        if let Some(scope) = params.session_id.as_deref()
            && scope != sid
        {
            continue;
        }
        let escalations = handle.pending.escalations.read().await;
        for e in escalations.iter() {
            items.push(PendingItem {
                session_id: sid.clone(),
                inner: e.clone(),
            });
        }
    }
    drop(active);

    let page = build_page(
        items,
        PaginationParams {
            limit: params.limit,
            cursor: params.cursor,
        },
    );
    Ok(Json(serde_json::to_value(page).unwrap_or_default()))
}

pub async fn aggregate_refusals(
    state: &AppState,
    auth: &Auth,
    params: PendingParams,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owned = owned_sessions(state, auth).await?;
    if let Some(sid) = params.session_id.as_deref() {
        enforce_session_scope(&owned, sid)?;
    }

    let mut items: Vec<PendingItem<Refusal>> = Vec::new();
    let active = state.active_sessions.read().await;
    for (sid, handle) in active.iter() {
        if !visible(&owned, sid) {
            continue;
        }
        if let Some(scope) = params.session_id.as_deref()
            && scope != sid
        {
            continue;
        }
        let refusals = handle.pending.refusals.read().await;
        for r in refusals.iter() {
            items.push(PendingItem {
                session_id: sid.clone(),
                inner: r.clone(),
            });
        }
    }
    drop(active);

    let page = build_page(
        items,
        PaginationParams {
            limit: params.limit,
            cursor: params.cursor,
        },
    );
    Ok(Json(serde_json::to_value(page).unwrap_or_default()))
}

fn visible(owned: &Option<std::collections::HashSet<String>>, sid: &str) -> bool {
    match owned {
        None => true,
        Some(set) => set.contains(sid),
    }
}

#[cfg(test)]
pub(crate) fn sort_key_for_test(sid: &str, item_id: &str) -> String {
    sort_key(sid, item_id)
}

// ----------------------------------------------------------------------
// Tests — W6 pending aggregation
// ----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use crate::middleware::Auth;
    use arc_swap::ArcSwap;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use console_core::auth::{AuthenticatedUser, ConsoleRole};
    use console_core::authenticator;
    use console_core::config::RuntimeConfig;
    use console_core::event_enricher::{EnrichedEscalation, EnrichedGate};
    use console_core::refusal_synthesizer::{Refusal, RefusalLayer};
    use console_core::session_monitor::{Frame, MonitorCmd, PendingState, SessionMonitorHandle};
    use console_core::taxonomy_builder;
    use console_db::DbPool;
    use console_db::create_test_pool;
    use console_db::queries::api_tokens;
    use console_runtime::grpc_pool::GrpcPool;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::{RwLock, broadcast, mpsc};
    use tower::ServiceExt;

    // ---- fixtures ----------------------------------------------------

    async fn build_state() -> Arc<AppState> {
        let pool = GrpcPool::new("[::1]:1", "[::1]:1", "[::1]:1");
        let db = create_test_pool().await.expect("db");
        let taxonomy = Arc::new(ArcSwap::from_pointee(
            taxonomy_builder::build_index(None, &[], &[]).index,
        ));
        Arc::new(AppState {
            db,
            taxonomy,
            runtime_config: RuntimeConfig {
                agent_address: "[::1]:1".into(),
                highway_address: "[::1]:1".into(),
                coordinator_address: "[::1]:1".into(),
                rest_address: "[::1]:1".into(),
            },
            grpc_pool: pool,
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    async fn seed_user(db: &DbPool, id: &str, role: &str) {
        sqlx::query(
            "INSERT INTO users (id, username, username_lower, display_name, password_hash,
                console_role, must_change_password, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .bind(id)
        .bind("h")
        .bind(role)
        .bind(0_i64)
        .bind("2026-04-15T00:00:00Z")
        .bind("2026-04-15T00:00:00Z")
        .execute(db)
        .await
        .expect("insert user");
    }

    async fn seed_session(db: &DbPool, sid: &str, owner: &str) {
        let row = console_db::queries::sessions::SessionRow {
            id: sid.into(),
            name: Some(sid.into()),
            owner_user_id: owner.into(),
            vertical: "swe".into(),
            workflow: "wf".into(),
            context: None,
            coordinator_workspace_id: Some("ws-root".into()),
            state: console_core::session_state::ACTIVE.into(),
            created_at: "2026-04-15T00:00:00Z".into(),
            launched_at: Some("2026-04-15T00:00:00Z".into()),
            closed_at: None,
            budget_max_cost_micros: None,
            budget_max_tokens: None,
            budget_max_wall_time_ms: None,
        };
        console_db::queries::sessions::insert_session(db, &row)
            .await
            .expect("insert");
    }

    async fn mint_bearer(db: &DbPool, owner: &str) -> String {
        let plain = format!("wcon_t_{}", uuid::Uuid::new_v4());
        let hash = authenticator::hash_token(&plain);
        api_tokens::insert_token(
            db,
            &format!("tok-{}", uuid::Uuid::new_v4()),
            owner,
            "test",
            &hash,
            "2026-04-15T00:00:00Z",
            None,
        )
        .await
        .expect("insert token");
        plain
    }

    async fn install_handle(state: &AppState, sid: &str) -> SessionMonitorHandle {
        let handle = SessionMonitorHandle {
            session_id: sid.into(),
            cmd_tx: mpsc::channel::<MonitorCmd>(1).0,
            broadcast_tx: broadcast::channel::<Frame>(8).0,
            pending: Arc::new(PendingState::default()),
        };
        state
            .active_sessions
            .write()
            .await
            .insert(sid.into(), handle.clone());
        handle
    }

    fn dummy_gate(id: &str) -> EnrichedGate {
        EnrichedGate {
            gate_id: id.into(),
            type_: "task_approval".into(),
            workspace_id: "ws-1".into(),
            workspace_label: "ws-1".into(),
            task_id: "t-1".into(),
            timeout_ms: 0,
            fallback_action: String::new(),
            created_at: String::new(),
            subject_len: 0,
        }
    }

    fn dummy_escalation(id: &str) -> EnrichedEscalation {
        EnrichedEscalation {
            escalation_id: id.into(),
            workspace_id: "ws-1".into(),
            workspace_label: "ws-1".into(),
            owner: "u".into(),
            created_at: String::new(),
            context_len: 0,
        }
    }

    fn dummy_refusal(id: &str) -> Refusal {
        Refusal {
            id: id.into(),
            layer: RefusalLayer::ToolLayer,
            workspace_id: "ws-1".into(),
            actor: "agent".into(),
            code: None,
            reason: None,
            sequence_number: 0,
        }
    }

    fn auth(user: &str, role: ConsoleRole) -> Auth {
        Auth(AuthenticatedUser {
            user_id: user.into(),
            username: user.into(),
            console_role: role,
        })
    }

    // ---- pure unit ---------------------------------------------------

    #[test]
    fn cursor_round_trips_through_base64() {
        let key = sort_key_for_test("s-1", "g-7");
        let encoded = encode_cursor(&key);
        let params = PaginationParams {
            limit: None,
            cursor: Some(encoded),
        };
        assert_eq!(params.decode_cursor(), Some(key));
    }

    #[test]
    fn item_sort_key_groups_by_session_then_id() {
        let a = PendingItem {
            session_id: "s-2".into(),
            inner: dummy_gate("g-1"),
        };
        let b = PendingItem {
            session_id: "s-1".into(),
            inner: dummy_gate("g-9"),
        };
        assert!(item_sort_key(&b) < item_sort_key(&a));
    }

    #[test]
    fn build_page_sorts_paginates_and_emits_cursor() {
        let items: Vec<PendingItem<EnrichedGate>> = (0..5)
            .map(|i| PendingItem {
                session_id: "s-1".into(),
                inner: dummy_gate(&format!("g-{i:02}")),
            })
            .collect();
        let page = build_page(
            items,
            PaginationParams {
                limit: Some(2),
                cursor: None,
            },
        );
        assert_eq!(page.items.len(), 2);
        assert!(page.has_more);
        assert!(page.cursor.is_some());
        assert_eq!(page.items[0].inner.gate_id, "g-00");
        assert_eq!(page.items[1].inner.gate_id, "g-01");
    }

    #[test]
    fn enforce_session_scope_blocks_non_owners() {
        let owned = Some(["s-mine".to_string()].into_iter().collect());
        assert!(enforce_session_scope(&owned, "s-mine").is_ok());
        let err = enforce_session_scope(&owned, "s-theirs").unwrap_err();
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn enforce_session_scope_passes_admin() {
        let owned: Option<std::collections::HashSet<String>> = None;
        assert!(enforce_session_scope(&owned, "s-anything").is_ok());
    }

    // ---- handler / aggregation tests ---------------------------------

    #[tokio::test]
    async fn aggregate_gates_returns_owned_session_pending_items() {
        let state = build_state().await;
        seed_user(&state.db, "u-1", "operator").await;
        seed_session(&state.db, "s-mine", "u-1").await;

        let handle = install_handle(&state, "s-mine").await;
        handle.pending.gates.write().await.extend(vec![
            dummy_gate("g-1"),
            dummy_gate("g-2"),
            dummy_gate("g-3"),
        ]);

        let v = aggregate_gates(
            &state,
            &auth("u-1", ConsoleRole::Operator),
            PendingParams {
                limit: None,
                cursor: None,
                session_id: None,
            },
        )
        .await
        .expect("ok");
        let body = v.0;
        let items = body["items"].as_array().expect("items array");
        assert_eq!(items.len(), 3);
        assert!(items[0]["gate_id"].as_str().is_some());
        assert_eq!(items[0]["session_id"], "s-mine");
    }

    #[tokio::test]
    async fn aggregate_gates_hides_other_owners_from_non_admin() {
        let state = build_state().await;
        seed_user(&state.db, "u-mine", "operator").await;
        seed_user(&state.db, "u-other", "operator").await;
        seed_session(&state.db, "s-mine", "u-mine").await;
        seed_session(&state.db, "s-other", "u-other").await;

        let h_mine = install_handle(&state, "s-mine").await;
        let h_other = install_handle(&state, "s-other").await;
        h_mine
            .pending
            .gates
            .write()
            .await
            .push(dummy_gate("mine-1"));
        h_other
            .pending
            .gates
            .write()
            .await
            .push(dummy_gate("other-1"));

        let v = aggregate_gates(
            &state,
            &auth("u-mine", ConsoleRole::Operator),
            PendingParams {
                limit: None,
                cursor: None,
                session_id: None,
            },
        )
        .await
        .expect("ok");
        let items = v.0["items"].as_array().expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["gate_id"], "mine-1");
    }

    #[tokio::test]
    async fn aggregate_gates_admin_sees_everything() {
        let state = build_state().await;
        seed_user(&state.db, "u-mine", "operator").await;
        seed_user(&state.db, "u-other", "operator").await;
        seed_user(&state.db, "u-admin", "admin").await;
        seed_session(&state.db, "s-mine", "u-mine").await;
        seed_session(&state.db, "s-other", "u-other").await;
        let h_a = install_handle(&state, "s-mine").await;
        let h_b = install_handle(&state, "s-other").await;
        h_a.pending.gates.write().await.push(dummy_gate("a"));
        h_b.pending.gates.write().await.push(dummy_gate("b"));

        let v = aggregate_gates(
            &state,
            &auth("u-admin", ConsoleRole::Admin),
            PendingParams {
                limit: None,
                cursor: None,
                session_id: None,
            },
        )
        .await
        .expect("ok");
        let items = v.0["items"].as_array().expect("items");
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn aggregate_gates_explicit_scope_to_owned_session() {
        let state = build_state().await;
        seed_user(&state.db, "u-1", "operator").await;
        seed_session(&state.db, "s-a", "u-1").await;
        seed_session(&state.db, "s-b", "u-1").await;
        let ha = install_handle(&state, "s-a").await;
        let hb = install_handle(&state, "s-b").await;
        ha.pending.gates.write().await.push(dummy_gate("a-1"));
        hb.pending.gates.write().await.push(dummy_gate("b-1"));

        let v = aggregate_gates(
            &state,
            &auth("u-1", ConsoleRole::Operator),
            PendingParams {
                limit: None,
                cursor: None,
                session_id: Some("s-a".into()),
            },
        )
        .await
        .expect("ok");
        let items = v.0["items"].as_array().expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["session_id"], "s-a");
    }

    #[tokio::test]
    async fn aggregate_gates_explicit_scope_to_unowned_session_403() {
        let state = build_state().await;
        seed_user(&state.db, "u-mine", "operator").await;
        seed_user(&state.db, "u-other", "operator").await;
        seed_session(&state.db, "s-mine", "u-mine").await;
        seed_session(&state.db, "s-other", "u-other").await;

        let err = aggregate_gates(
            &state,
            &auth("u-mine", ConsoleRole::Operator),
            PendingParams {
                limit: None,
                cursor: None,
                session_id: Some("s-other".into()),
            },
        )
        .await
        .expect_err("must 403");
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn aggregate_gates_paginates_across_sessions() {
        let state = build_state().await;
        seed_user(&state.db, "u-1", "operator").await;
        // 3 sessions × 40 gates each = 120 items.
        for s in 0..3 {
            let sid = format!("s-{s:02}");
            seed_session(&state.db, &sid, "u-1").await;
            let h = install_handle(&state, &sid).await;
            let mut g = h.pending.gates.write().await;
            for i in 0..40 {
                g.push(dummy_gate(&format!("g-{i:03}")));
            }
        }

        let mut cursor: Option<String> = None;
        let mut total = 0usize;
        for _ in 0..4 {
            let v = aggregate_gates(
                &state,
                &auth("u-1", ConsoleRole::Operator),
                PendingParams {
                    limit: Some(50),
                    cursor: cursor.clone(),
                    session_id: None,
                },
            )
            .await
            .expect("ok");
            let items = v.0["items"].as_array().unwrap();
            total += items.len();
            cursor = v.0["cursor"].as_str().map(String::from);
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(total, 120);
        assert!(cursor.is_none());
    }

    #[tokio::test]
    async fn aggregate_gates_empty_active_returns_empty_list() {
        let state = build_state().await;
        seed_user(&state.db, "u-1", "operator").await;
        let v = aggregate_gates(
            &state,
            &auth("u-1", ConsoleRole::Operator),
            PendingParams {
                limit: None,
                cursor: None,
                session_id: None,
            },
        )
        .await
        .expect("ok");
        assert_eq!(v.0["items"].as_array().unwrap().len(), 0);
        assert_eq!(v.0["has_more"], false);
    }

    #[tokio::test]
    async fn aggregate_escalations_returns_owned_items() {
        let state = build_state().await;
        seed_user(&state.db, "u-1", "operator").await;
        seed_session(&state.db, "s-mine", "u-1").await;
        let h = install_handle(&state, "s-mine").await;
        h.pending
            .escalations
            .write()
            .await
            .extend(vec![dummy_escalation("e-1"), dummy_escalation("e-2")]);

        let v = aggregate_escalations(
            &state,
            &auth("u-1", ConsoleRole::Operator),
            PendingParams {
                limit: None,
                cursor: None,
                session_id: None,
            },
        )
        .await
        .expect("ok");
        assert_eq!(v.0["items"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn aggregate_refusals_returns_owned_items() {
        let state = build_state().await;
        seed_user(&state.db, "u-1", "operator").await;
        seed_session(&state.db, "s-mine", "u-1").await;
        let h = install_handle(&state, "s-mine").await;
        h.pending.refusals.write().await.extend(vec![
            dummy_refusal("r-1"),
            dummy_refusal("r-2"),
            dummy_refusal("r-3"),
        ]);

        let v = aggregate_refusals(
            &state,
            &auth("u-1", ConsoleRole::Operator),
            PendingParams {
                limit: None,
                cursor: None,
                session_id: None,
            },
        )
        .await
        .expect("ok");
        assert_eq!(v.0["items"].as_array().unwrap().len(), 3);
    }

    // ---- end-to-end via router ---------------------------------------

    #[tokio::test]
    async fn http_pending_gates_returns_paginated_json() {
        let state = build_state().await;
        seed_user(&state.db, "u-1", "operator").await;
        seed_session(&state.db, "s-mine", "u-1").await;
        let h = install_handle(&state, "s-mine").await;
        h.pending.gates.write().await.push(dummy_gate("g-1"));
        let token = mint_bearer(&state.db, "u-1").await;
        let app = router().with_state(state.clone());

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/gates/pending")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["items"][0]["gate_id"], "g-1");
        assert_eq!(v["items"][0]["session_id"], "s-mine");
    }
}
