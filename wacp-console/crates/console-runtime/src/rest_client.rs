//! REST client for the WACP runtime's vertical registry.
//!
//! Spec: `wcon-discovery` §2.2, ADR-001

use serde::Deserialize;
use tracing::{info, warn};

use crate::VerticalManifest;

/// Summary returned by `GET /v1/verticals`.
#[derive(Debug, Clone, Deserialize)]
pub struct VerticalSummary {
    pub id: String,
    pub name: String,
    pub defining_constraint: String,
    pub task_type_count: u32,
    pub workflow_count: u32,
    pub tool_count: u32,
}

/// Result of loading verticals from the runtime REST API.
pub struct VerticalLoadResult {
    pub manifests: Vec<VerticalManifest>,
    pub failed: Vec<(String, String)>,
}

/// Fetch all vertical manifests from the WACP runtime REST API.
///
/// Tolerates per-vertical failures: each failed manifest is recorded in `failed`
/// and the rest continue loading.
pub async fn load_verticals(rest_address: &str) -> Result<VerticalLoadResult, String> {
    let base_url = rest_address.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))?;

    // Step 1: List all verticals.
    let list_url = format!("{base_url}/v1/verticals");
    let summaries: Vec<VerticalSummary> = match client.get(&list_url).send().await {
        Ok(resp) => {
            if !resp.status().is_success() {
                return Err(format!("GET /v1/verticals returned {}", resp.status()));
            }
            resp.json()
                .await
                .map_err(|e| format!("failed to parse vertical list: {e}"))?
        }
        Err(e) => {
            return Err(format!(
                "failed to reach runtime REST API at {list_url}: {e}"
            ));
        }
    };

    info!(
        count = summaries.len(),
        "fetched vertical summaries from runtime"
    );

    // Step 2: Fetch full manifest for each vertical.
    let mut manifests = Vec::new();
    let mut failed = Vec::new();

    for summary in &summaries {
        let detail_url = format!("{base_url}/v1/verticals/{}", summary.id);
        match client.get(&detail_url).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let msg = format!(
                        "GET /v1/verticals/{} returned {}",
                        summary.id,
                        resp.status()
                    );
                    warn!(vertical_id = %summary.id, status = %resp.status(), "manifest fetch failed");
                    failed.push((summary.id.clone(), msg));
                    continue;
                }
                match resp.json::<VerticalManifest>().await {
                    Ok(manifest) => manifests.push(manifest),
                    Err(e) => {
                        let msg = format!("failed to parse manifest for '{}': {e}", summary.id);
                        warn!(vertical_id = %summary.id, "manifest parse failed");
                        failed.push((summary.id.clone(), msg));
                    }
                }
            }
            Err(e) => {
                let msg = format!("failed to fetch manifest for '{}': {e}", summary.id);
                warn!(vertical_id = %summary.id, "manifest fetch error");
                failed.push((summary.id.clone(), msg));
            }
        }
    }

    info!(
        loaded = manifests.len(),
        failed = failed.len(),
        "vertical manifest loading complete"
    );

    Ok(VerticalLoadResult { manifests, failed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_summary_deserializes() {
        let json = r#"{
            "id": "finance",
            "name": "Finance",
            "defining_constraint": "Regulatory compliance",
            "task_type_count": 5,
            "workflow_count": 2,
            "tool_count": 8
        }"#;
        let summary: VerticalSummary = serde_json::from_str(json).unwrap();
        assert_eq!(summary.id, "finance");
        assert_eq!(summary.tool_count, 8);
    }
}
