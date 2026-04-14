//! Cursor-based pagination — reusable across all list endpoints.
//!
//! Spec: `wcon-discovery` §4.2, `wcon-api` §5.3

use base64::Engine;
use serde::{Deserialize, Serialize};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

/// Query parameters for paginated list endpoints.
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default)]
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

impl PaginationParams {
    /// Returns the effective limit, clamped to [1, MAX_LIMIT].
    pub fn effective_limit(&self) -> usize {
        self.limit
            .unwrap_or(DEFAULT_LIMIT)
            .clamp(1, MAX_LIMIT)
    }

    /// Decode the cursor string. Returns None if absent or invalid.
    pub fn decode_cursor(&self) -> Option<String> {
        let cursor = self.cursor.as_ref()?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(cursor)
            .ok()?;
        String::from_utf8(bytes).ok()
    }
}

/// Paginated response envelope.
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub has_more: bool,
}

/// Encode a sort key into an opaque cursor string (base64url).
pub fn encode_cursor(sort_key: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sort_key.as_bytes())
}

/// Apply cursor-based pagination to a pre-sorted slice.
///
/// `sort_key_fn` extracts the sort key from each item (must match the sort order).
/// Items are filtered to those after the cursor, then limited.
pub fn paginate<T: Clone + Serialize>(
    items: &[T],
    params: &PaginationParams,
    sort_key_fn: impl Fn(&T) -> &str,
) -> PaginatedResponse<T> {
    let limit = params.effective_limit();
    let cursor_value = params.decode_cursor();

    let filtered: Vec<&T> = items
        .iter()
        .filter(|item| {
            if let Some(ref cursor) = cursor_value {
                sort_key_fn(item) > cursor.as_str()
            } else {
                true
            }
        })
        .collect();

    let has_more = filtered.len() > limit;
    let page: Vec<T> = filtered.into_iter().take(limit).cloned().collect();
    let next_cursor = if has_more {
        page.last().map(|item| encode_cursor(sort_key_fn(item)))
    } else {
        None
    };

    PaginatedResponse {
        items: page,
        cursor: next_cursor,
        has_more,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize)]
    struct Item {
        name: String,
    }

    fn items() -> Vec<Item> {
        (1..=10)
            .map(|i| Item {
                name: format!("item_{i:02}"),
            })
            .collect()
    }

    #[test]
    fn first_page_default_limit() {
        let result = paginate(
            &items(),
            &PaginationParams { limit: None, cursor: None },
            |i| &i.name,
        );
        assert_eq!(result.items.len(), 10);
        assert!(!result.has_more);
        assert!(result.cursor.is_none());
    }

    #[test]
    fn pagination_with_limit() {
        let result = paginate(
            &items(),
            &PaginationParams { limit: Some(3), cursor: None },
            |i| &i.name,
        );
        assert_eq!(result.items.len(), 3);
        assert!(result.has_more);
        assert!(result.cursor.is_some());
        assert_eq!(result.items[2].name, "item_03");
    }

    #[test]
    fn second_page_via_cursor() {
        let first = paginate(
            &items(),
            &PaginationParams { limit: Some(3), cursor: None },
            |i| &i.name,
        );
        let second = paginate(
            &items(),
            &PaginationParams { limit: Some(3), cursor: first.cursor },
            |i| &i.name,
        );
        assert_eq!(second.items.len(), 3);
        assert_eq!(second.items[0].name, "item_04");
        assert!(second.has_more);
    }

    #[test]
    fn last_page_has_no_cursor() {
        let result = paginate(
            &items(),
            &PaginationParams { limit: Some(10), cursor: None },
            |i| &i.name,
        );
        assert_eq!(result.items.len(), 10);
        assert!(!result.has_more);
        assert!(result.cursor.is_none());
    }

    #[test]
    fn limit_capped_at_max() {
        let params = PaginationParams { limit: Some(999), cursor: None };
        assert_eq!(params.effective_limit(), MAX_LIMIT);
    }

    #[test]
    fn cursor_roundtrip() {
        let encoded = encode_cursor("item_05");
        let params = PaginationParams { limit: None, cursor: Some(encoded) };
        assert_eq!(params.decode_cursor().unwrap(), "item_05");
    }
}
