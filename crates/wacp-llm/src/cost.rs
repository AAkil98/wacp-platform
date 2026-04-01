use std::collections::HashMap;

use crate::result::{Cost, TokenUsage};

/// Pricing for a model: microdollars per million tokens.
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    /// Microdollars per million input tokens.
    pub input_price_micros_per_m: u64,
    /// Microdollars per million output tokens.
    pub output_price_micros_per_m: u64,
}

/// Calculate the cost of a completion from usage and pricing.
pub fn calculate_cost(usage: &TokenUsage, pricing: &ModelPricing) -> Cost {
    let input_cost =
        (usage.input_tokens as u64) * pricing.input_price_micros_per_m / 1_000_000;
    let output_cost =
        (usage.output_tokens as u64) * pricing.output_price_micros_per_m / 1_000_000;
    Cost::usd(input_cost + output_cost)
}

/// Lookup pricing for a model by prefix matching.
pub fn lookup_pricing(model: &str, table: &[(&str, ModelPricing)]) -> Option<ModelPricing> {
    // Try exact match first, then prefix match (longest prefix wins).
    table
        .iter()
        .filter(|(prefix, _)| model.starts_with(prefix))
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, pricing)| *pricing)
}

/// Accumulates costs per workspace and total.
#[derive(Debug, Default)]
pub struct CostTracker {
    workspace_costs: HashMap<String, u64>,
    total_micros: u64,
}

impl CostTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a cost, optionally attributed to a workspace.
    pub fn record(&mut self, workspace_id: Option<&str>, cost: &Cost) {
        self.total_micros += cost.amount_micros;
        if let Some(ws) = workspace_id {
            *self.workspace_costs.entry(ws.to_string()).or_default() += cost.amount_micros;
        }
    }

    /// Total cost for a specific workspace (microdollars).
    pub fn workspace_total(&self, workspace_id: &str) -> u64 {
        self.workspace_costs.get(workspace_id).copied().unwrap_or(0)
    }

    /// Total cost across all workspaces (microdollars).
    pub fn total(&self) -> u64 {
        self.total_micros
    }

    /// Number of workspaces with recorded costs.
    pub fn workspace_count(&self) -> usize {
        self.workspace_costs.len()
    }
}

// ---------------------------------------------------------------------------
// Provider pricing tables
// ---------------------------------------------------------------------------

/// Anthropic model pricing (as of 2025).
pub const ANTHROPIC_PRICING: &[(&str, ModelPricing)] = &[
    ("claude-opus-4", ModelPricing { input_price_micros_per_m: 15_000_000, output_price_micros_per_m: 75_000_000 }),
    ("claude-sonnet-4", ModelPricing { input_price_micros_per_m: 3_000_000, output_price_micros_per_m: 15_000_000 }),
    ("claude-haiku-4", ModelPricing { input_price_micros_per_m: 800_000, output_price_micros_per_m: 4_000_000 }),
];

/// OpenAI model pricing (as of 2025).
pub const OPENAI_PRICING: &[(&str, ModelPricing)] = &[
    ("gpt-4o", ModelPricing { input_price_micros_per_m: 2_500_000, output_price_micros_per_m: 10_000_000 }),
    ("gpt-4o-mini", ModelPricing { input_price_micros_per_m: 150_000, output_price_micros_per_m: 600_000 }),
    ("o3", ModelPricing { input_price_micros_per_m: 2_000_000, output_price_micros_per_m: 8_000_000 }),
    ("o3-mini", ModelPricing { input_price_micros_per_m: 1_100_000, output_price_micros_per_m: 4_400_000 }),
    ("o4-mini", ModelPricing { input_price_micros_per_m: 1_100_000, output_price_micros_per_m: 4_400_000 }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_cost_sonnet() {
        // 1000 input tokens at $3/M + 500 output tokens at $15/M
        let usage = TokenUsage { input_tokens: 1000, output_tokens: 500 };
        let pricing = ModelPricing {
            input_price_micros_per_m: 3_000_000,
            output_price_micros_per_m: 15_000_000,
        };
        let cost = calculate_cost(&usage, &pricing);
        // input: 1000 * 3_000_000 / 1_000_000 = 3000 microdollars = $0.003
        // output: 500 * 15_000_000 / 1_000_000 = 7500 microdollars = $0.0075
        // total: 10500 microdollars = $0.0105
        assert_eq!(cost.amount_micros, 10_500);
        assert_eq!(cost.currency, "USD");
    }

    #[test]
    fn calculate_cost_zero_tokens() {
        let usage = TokenUsage::default();
        let pricing = ModelPricing {
            input_price_micros_per_m: 3_000_000,
            output_price_micros_per_m: 15_000_000,
        };
        let cost = calculate_cost(&usage, &pricing);
        assert_eq!(cost.amount_micros, 0);
    }

    #[test]
    fn calculate_cost_large_usage() {
        // 1M input tokens at $15/M = $15
        let usage = TokenUsage { input_tokens: 1_000_000, output_tokens: 0 };
        let pricing = ModelPricing {
            input_price_micros_per_m: 15_000_000,
            output_price_micros_per_m: 75_000_000,
        };
        let cost = calculate_cost(&usage, &pricing);
        assert_eq!(cost.amount_micros, 15_000_000); // $15.00
    }

    #[test]
    fn lookup_pricing_exact_match() {
        let pricing = lookup_pricing("claude-sonnet-4", ANTHROPIC_PRICING);
        assert!(pricing.is_some());
        assert_eq!(pricing.unwrap().input_price_micros_per_m, 3_000_000);
    }

    #[test]
    fn lookup_pricing_prefix_match() {
        let pricing = lookup_pricing("claude-sonnet-4-20250514", ANTHROPIC_PRICING);
        assert!(pricing.is_some());
        assert_eq!(pricing.unwrap().input_price_micros_per_m, 3_000_000);
    }

    #[test]
    fn lookup_pricing_unknown_model() {
        let pricing = lookup_pricing("unknown-model-v99", ANTHROPIC_PRICING);
        assert!(pricing.is_none());
    }

    #[test]
    fn lookup_pricing_longest_prefix_wins() {
        // Both "gpt-4o" and "gpt-4o-mini" match "gpt-4o-mini-2024"
        let pricing = lookup_pricing("gpt-4o-mini-2024", OPENAI_PRICING);
        assert!(pricing.is_some());
        // Should match "gpt-4o-mini" (longer prefix), not "gpt-4o"
        assert_eq!(pricing.unwrap().input_price_micros_per_m, 150_000);
    }

    #[test]
    fn cost_tracker_empty() {
        let tracker = CostTracker::new();
        assert_eq!(tracker.total(), 0);
        assert_eq!(tracker.workspace_count(), 0);
        assert_eq!(tracker.workspace_total("ws-1"), 0);
    }

    #[test]
    fn cost_tracker_record_with_workspace() {
        let mut tracker = CostTracker::new();
        tracker.record(Some("ws-1"), &Cost::usd(1000));
        tracker.record(Some("ws-1"), &Cost::usd(2000));
        tracker.record(Some("ws-2"), &Cost::usd(500));

        assert_eq!(tracker.workspace_total("ws-1"), 3000);
        assert_eq!(tracker.workspace_total("ws-2"), 500);
        assert_eq!(tracker.total(), 3500);
        assert_eq!(tracker.workspace_count(), 2);
    }

    #[test]
    fn cost_tracker_record_without_workspace() {
        let mut tracker = CostTracker::new();
        tracker.record(None, &Cost::usd(1000));
        assert_eq!(tracker.total(), 1000);
        assert_eq!(tracker.workspace_count(), 0); // no workspace attributed
    }

    #[test]
    fn anthropic_pricing_table_has_entries() {
        assert!(!ANTHROPIC_PRICING.is_empty());
        for (prefix, pricing) in ANTHROPIC_PRICING {
            assert!(!prefix.is_empty());
            assert!(pricing.input_price_micros_per_m > 0);
            assert!(pricing.output_price_micros_per_m > 0);
        }
    }

    #[test]
    fn openai_pricing_table_has_entries() {
        assert!(!OPENAI_PRICING.is_empty());
        for (prefix, pricing) in OPENAI_PRICING {
            assert!(!prefix.is_empty());
            assert!(pricing.input_price_micros_per_m > 0);
            assert!(pricing.output_price_micros_per_m > 0);
        }
    }

    #[test]
    fn lookup_pricing_empty_table() {
        let pricing = lookup_pricing("any-model", &[]);
        assert!(pricing.is_none());
    }

    #[test]
    fn cost_tracker_many_workspaces() {
        let mut tracker = CostTracker::new();
        for i in 0..100 {
            tracker.record(Some(&format!("ws-{i}")), &Cost::usd(100));
        }
        assert_eq!(tracker.total(), 10_000);
        assert_eq!(tracker.workspace_count(), 100);
    }

    #[test]
    fn calculate_cost_output_only() {
        let usage = TokenUsage { input_tokens: 0, output_tokens: 1000 };
        let pricing = ModelPricing {
            input_price_micros_per_m: 3_000_000,
            output_price_micros_per_m: 15_000_000,
        };
        let cost = calculate_cost(&usage, &pricing);
        // output: 1000 * 15_000_000 / 1_000_000 = 15000
        assert_eq!(cost.amount_micros, 15_000);
    }
}
