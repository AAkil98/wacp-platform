pub mod anthropic;
pub mod openai;

use serde::{Deserialize, Serialize};

/// Provider configuration — which provider and how to connect.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum ProviderConfig {
    Anthropic {
        api_key: String,
        #[serde(default = "default_anthropic_url")]
        base_url: String,
        #[serde(default)]
        default_model: Option<String>,
    },
    Openai {
        api_key: String,
        #[serde(default = "default_openai_url")]
        base_url: String,
        #[serde(default)]
        default_model: Option<String>,
    },
    Generic {
        #[serde(default)]
        api_key: Option<String>,
        base_url: String,
        #[serde(default)]
        default_model: Option<String>,
    },
}

fn default_anthropic_url() -> String {
    "https://api.anthropic.com".to_string()
}

fn default_openai_url() -> String {
    "https://api.openai.com".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_config_serde() {
        let json = r#"{"provider":"anthropic","api_key":"sk-test"}"#;
        let config: ProviderConfig = serde_json::from_str(json).unwrap();
        match config {
            ProviderConfig::Anthropic { api_key, base_url, .. } => {
                assert_eq!(api_key, "sk-test");
                assert_eq!(base_url, "https://api.anthropic.com");
            }
            _ => panic!("expected Anthropic"),
        }
    }

    #[test]
    fn openai_config_serde() {
        let json = r#"{"provider":"openai","api_key":"sk-test"}"#;
        let config: ProviderConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, ProviderConfig::Openai { .. }));
    }

    #[test]
    fn generic_config_serde() {
        let json = r#"{"provider":"generic","base_url":"http://localhost:11434"}"#;
        let config: ProviderConfig = serde_json::from_str(json).unwrap();
        match config {
            ProviderConfig::Generic { api_key, base_url, .. } => {
                assert!(api_key.is_none());
                assert_eq!(base_url, "http://localhost:11434");
            }
            _ => panic!("expected Generic"),
        }
    }
}
