//! Search endpoint — cross-entity substring matching.
//!
//! Spec: `wcon-discovery` §5

use axum::extract::{Query, State};
use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use console_core::authorizer::{self, Action};
use console_core::taxonomy::TaxonomyIndex;

use crate::AppState;
use crate::error::ApiError;
use crate::middleware::Auth;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/search", get(search))
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    vertical: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct SearchHit {
    id: String,
    name: String,
    description: String,
    match_field: String,
    vertical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    has_policy: Option<bool>,
    rank: u8,
}

#[derive(Debug, Serialize)]
struct SearchResponse {
    query: String,
    results: SearchResults,
}

#[derive(Debug, Default, Serialize)]
struct SearchResults {
    roles: Vec<SearchHit>,
    tools: Vec<SearchHit>,
    envelope_types: Vec<SearchHit>,
    checkpoint_types: Vec<SearchHit>,
    vertical_checkpoint_types: Vec<SearchHit>,
    verticals: Vec<SearchHit>,
    workflows: Vec<SearchHit>,
    task_types: Vec<SearchHit>,
    context_fields: Vec<SearchHit>,
    tool_policies: Vec<SearchHit>,
}

async fn search(
    State(state): State<Arc<AppState>>,
    auth: Auth,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, ApiError> {
    authorizer::authorize(&auth, Action::BrowseTaxonomy).map_err(ApiError::from)?;

    if params.q.len() < 2 {
        return Err(ApiError::from(console_core::error::ConsoleError::validation(
            "QUERY_TOO_SHORT",
            "Search query must be at least 2 characters",
        )));
    }

    let limit = params.limit.unwrap_or(10).min(50);
    let query_lower = params.q.to_lowercase();
    let index = state.taxonomy.load();

    let mut results = SearchResults::default();
    let filter_type = params.entity_type.as_deref();

    if should_search(filter_type, "role") {
        results.roles = search_roles(&index, &query_lower, params.vertical.as_deref(), limit);
    }
    if should_search(filter_type, "tool") {
        results.tools = search_tools(&index, &query_lower, params.vertical.as_deref(), limit);
    }
    if should_search(filter_type, "envelope_type") {
        results.envelope_types = search_envelope_types(&index, &query_lower, limit);
    }
    if should_search(filter_type, "checkpoint_type") {
        results.checkpoint_types = search_checkpoint_types(&index, &query_lower, limit);
    }
    if should_search(filter_type, "vertical") {
        results.verticals = search_verticals(&index, &query_lower, limit);
    }
    if should_search(filter_type, "vertical_checkpoint_type") {
        results.vertical_checkpoint_types =
            search_vertical_checkpoint_types(&index, &query_lower, params.vertical.as_deref(), limit);
    }
    if should_search(filter_type, "workflow") {
        results.workflows =
            search_workflows(&index, &query_lower, params.vertical.as_deref(), limit);
    }
    if should_search(filter_type, "task_type") {
        results.task_types =
            search_task_types(&index, &query_lower, params.vertical.as_deref(), limit);
    }
    if should_search(filter_type, "context_field") {
        results.context_fields =
            search_context_fields(&index, &query_lower, params.vertical.as_deref(), limit);
    }
    if should_search(filter_type, "tool_policy") {
        results.tool_policies =
            search_tool_policies(&index, &query_lower, params.vertical.as_deref(), limit);
    }

    Ok(Json(SearchResponse {
        query: params.q,
        results,
    }))
}

fn should_search(filter: Option<&str>, entity: &str) -> bool {
    filter.is_none_or(|f| f == entity)
}

fn rank_any(fields: &[&str], query: &str) -> Option<(u8, String)> {
    // Check name fields first (indices 0 usually).
    for (i, field) in fields.iter().enumerate() {
        let lower = field.to_lowercase();
        if lower == query {
            return Some((1, format!("field_{i}")));
        }
    }
    for (i, field) in fields.iter().enumerate() {
        let lower = field.to_lowercase();
        if lower.starts_with(query) {
            return Some((2, format!("field_{i}")));
        }
    }
    for (i, field) in fields.iter().enumerate() {
        let lower = field.to_lowercase();
        if lower.contains(query) {
            return Some((if i == 0 { 3 } else { 4 }, format!("field_{i}")));
        }
    }
    None
}

fn search_roles(
    index: &TaxonomyIndex,
    query: &str,
    vertical: Option<&str>,
    limit: usize,
) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = index
        .roles
        .values()
        .filter(|r| vertical.is_none_or(|v| r.vertical.as_deref() == Some(v)))
        .filter_map(|r| {
            let caps_str = r.capabilities_added.join(" ");
            rank_any(&[&r.name, &r.base_role, &caps_str], query).map(|(rank, _)| SearchHit {
                id: r.name.clone(),
                name: r.name.clone(),
                description: format!("extends {}", r.base_role),
                match_field: "name".to_string(),
                vertical: r.vertical.clone(),
                has_policy: None,
                rank,
            })
        })
        .collect();
    hits.sort_by_key(|h| (h.rank, h.name.clone()));
    hits.truncate(limit);
    hits
}

fn search_tools(
    index: &TaxonomyIndex,
    query: &str,
    vertical: Option<&str>,
    limit: usize,
) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = index
        .tools
        .values()
        .filter(|t| vertical.is_none_or(|v| t.vertical.as_deref() == Some(v)))
        .filter_map(|t| {
            rank_any(&[&t.name, &t.description], query).map(|(rank, field)| SearchHit {
                id: t.name.clone(),
                name: t.name.clone(),
                description: t.description.clone(),
                match_field: if field == "field_0" {
                    "name".to_string()
                } else {
                    "description".to_string()
                },
                vertical: t.vertical.clone(),
                has_policy: Some(t.policy.is_some()),
                rank,
            })
        })
        .collect();
    hits.sort_by_key(|h| (h.rank, h.name.clone()));
    hits.truncate(limit);
    hits
}

fn search_envelope_types(
    index: &TaxonomyIndex,
    query: &str,
    limit: usize,
) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = index
        .envelope_types
        .values()
        .filter_map(|e| {
            rank_any(&[&e.name, &e.description], query).map(|(rank, _)| SearchHit {
                id: e.name.clone(),
                name: e.name.clone(),
                description: e.description.clone(),
                match_field: "name".to_string(),
                vertical: None,
                has_policy: None,
                rank,
            })
        })
        .collect();
    hits.sort_by_key(|h| (h.rank, h.name.clone()));
    hits.truncate(limit);
    hits
}

fn search_checkpoint_types(
    index: &TaxonomyIndex,
    query: &str,
    limit: usize,
) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = index
        .checkpoint_types
        .values()
        .filter_map(|c| {
            rank_any(&[&c.name, &c.description], query).map(|(rank, _)| SearchHit {
                id: c.name.clone(),
                name: c.name.clone(),
                description: c.description.clone(),
                match_field: "name".to_string(),
                vertical: None,
                has_policy: None,
                rank,
            })
        })
        .collect();
    hits.sort_by_key(|h| (h.rank, h.name.clone()));
    hits.truncate(limit);
    hits
}

fn search_verticals(
    index: &TaxonomyIndex,
    query: &str,
    limit: usize,
) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = index
        .verticals
        .values()
        .filter_map(|v| {
            rank_any(&[&v.name, &v.id, &v.defining_constraint], query).map(|(rank, field)| {
                SearchHit {
                    id: v.id.clone(),
                    name: v.name.clone(),
                    description: v.defining_constraint.clone(),
                    match_field: if field == "field_2" {
                        "defining_constraint".to_string()
                    } else {
                        "name".to_string()
                    },
                    vertical: None,
                    has_policy: None,
                    rank,
                }
            })
        })
        .collect();
    hits.sort_by_key(|h| (h.rank, h.name.clone()));
    hits.truncate(limit);
    hits
}

fn search_vertical_checkpoint_types(
    index: &TaxonomyIndex,
    query: &str,
    vertical: Option<&str>,
    limit: usize,
) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = Vec::new();
    for v in index.verticals.values() {
        if vertical.is_some_and(|vf| vf != v.id) {
            continue;
        }
        for (name, cp) in &v.checkpoint_types {
            let field_names = cp.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(" ");
            if let Some((rank, _)) = rank_any(&[name, &cp.description, &field_names], query) {
                hits.push(SearchHit {
                    id: format!("{}:{}", v.id, name),
                    name: name.clone(),
                    description: cp.description.clone(),
                    match_field: "name".to_string(),
                    vertical: Some(v.id.clone()),
                    has_policy: None,
                    rank,
                });
            }
        }
    }
    hits.sort_by_key(|h| (h.rank, h.name.clone()));
    hits.truncate(limit);
    hits
}

fn search_workflows(
    index: &TaxonomyIndex,
    query: &str,
    vertical: Option<&str>,
    limit: usize,
) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = Vec::new();
    for v in index.verticals.values() {
        if vertical.is_some_and(|vf| vf != v.id) {
            continue;
        }
        for wf in &v.workflows {
            if let Some((rank, _)) = rank_any(&[&wf.name, &wf.description], query) {
                hits.push(SearchHit {
                    id: format!("{}:{}", v.id, wf.id),
                    name: wf.name.clone(),
                    description: wf.description.clone(),
                    match_field: "name".to_string(),
                    vertical: Some(v.id.clone()),
                    has_policy: None,
                    rank,
                });
            }
        }
    }
    hits.sort_by_key(|h| (h.rank, h.name.clone()));
    hits.truncate(limit);
    hits
}

fn search_task_types(
    index: &TaxonomyIndex,
    query: &str,
    vertical: Option<&str>,
    limit: usize,
) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = Vec::new();
    for v in index.verticals.values() {
        if vertical.is_some_and(|vf| vf != v.id) {
            continue;
        }
        for tt in &v.task_types {
            let keywords = tt.keywords.join(" ");
            if let Some((rank, _)) =
                rank_any(&[&tt.name, &tt.description, &keywords], query)
            {
                hits.push(SearchHit {
                    id: format!("{}:{}", v.id, tt.id),
                    name: tt.name.clone(),
                    description: tt.description.clone(),
                    match_field: "name".to_string(),
                    vertical: Some(v.id.clone()),
                    has_policy: None,
                    rank,
                });
            }
        }
    }
    hits.sort_by_key(|h| (h.rank, h.name.clone()));
    hits.truncate(limit);
    hits
}

fn search_context_fields(
    index: &TaxonomyIndex,
    query: &str,
    vertical: Option<&str>,
    limit: usize,
) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = Vec::new();
    for v in index.verticals.values() {
        if vertical.is_some_and(|vf| vf != v.id) {
            continue;
        }
        for (name, field) in &v.context_schema {
            if let Some((rank, _)) = rank_any(&[name, &field.description], query) {
                hits.push(SearchHit {
                    id: format!("{}:{}", v.id, name),
                    name: name.clone(),
                    description: field.description.clone(),
                    match_field: "name".to_string(),
                    vertical: Some(v.id.clone()),
                    has_policy: None,
                    rank,
                });
            }
        }
    }
    hits.sort_by_key(|h| (h.rank, h.name.clone()));
    hits.truncate(limit);
    hits
}

fn search_tool_policies(
    index: &TaxonomyIndex,
    query: &str,
    vertical: Option<&str>,
    limit: usize,
) -> Vec<SearchHit> {
    let mut hits: Vec<SearchHit> = Vec::new();
    for v in index.verticals.values() {
        if vertical.is_some_and(|vf| vf != v.id) {
            continue;
        }
        for (tool_name, policy) in &v.tool_policies {
            if let Some((rank, _)) = rank_any(&[tool_name, &policy.description], query) {
                hits.push(SearchHit {
                    id: format!("{}:{}", v.id, tool_name),
                    name: tool_name.clone(),
                    description: policy.description.clone(),
                    match_field: "name".to_string(),
                    vertical: Some(v.id.clone()),
                    has_policy: None,
                    rank,
                });
            }
        }
    }
    hits.sort_by_key(|h| (h.rank, h.name.clone()));
    hits.truncate(limit);
    hits
}
