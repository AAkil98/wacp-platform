pub mod anthropic;
pub mod openai;
pub mod stub;

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::adapter::LlmAdapter;
use crate::error::LlmError;

pub use stub::{StubAdapter, StubFixtures, StubMatcher, StubResponse, StubToolCall};

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
    /// Deterministic stub backed by a YAML fixture. For integration and E2E
    /// tests only. See `providers::stub` and `wcon-llm-stub` for the design.
    Stub {
        /// Path to a fixture YAML file. Mutually exclusive with `fixtures_inline`.
        #[serde(default)]
        fixtures_path: Option<PathBuf>,
        /// Inline fixtures (used by unit tests that don't want a tempdir).
        /// Takes precedence over `fixtures_path` when both are set.
        #[serde(default)]
        fixtures_inline: Option<StubFixtures>,
        /// Model name echoed back in `CompletionResult.model` when the caller
        /// does not pass `CompletionOptions.model`.
        #[serde(default = "default_stub_model")]
        default_model: String,
        /// Inter-event delay for streamed completions, in milliseconds. `0`
        /// (the default) streams as fast as the async executor can drive it.
        #[serde(default)]
        token_delay_ms: u64,
    },
}

fn default_anthropic_url() -> String {
    "https://api.anthropic.com".to_string()
}

fn default_openai_url() -> String {
    "https://api.openai.com".to_string()
}

fn default_stub_model() -> String {
    "stub-model-1".to_string()
}

/// Construct a live `LlmAdapter` from a `ProviderConfig`.
///
/// Only the `Stub` variant is implemented today; the other variants return a
/// structural error. This is the integration point the runtime / agents will
/// call when Anthropic / OpenAI / Generic adapters land.
pub fn build_adapter(cfg: &ProviderConfig) -> Result<Arc<dyn LlmAdapter>, LlmError> {
    match cfg {
        ProviderConfig::Stub {
            fixtures_path,
            fixtures_inline,
            default_model,
            token_delay_ms,
        } => {
            let fixtures = if let Some(inline) = fixtures_inline {
                inline.clone()
            } else if let Some(path) = fixtures_path {
                StubFixtures::load(path)?
            } else {
                StubFixtures::default()
            };
            Ok(Arc::new(StubAdapter::new(
                fixtures,
                default_model.clone(),
                *token_delay_ms,
            )))
        }
        ProviderConfig::Anthropic { .. } => Err(LlmError::structural(
            "Anthropic adapter not yet wired through build_adapter — \
             use the provider helpers in wacp-llm::providers::anthropic directly.",
        )),
        ProviderConfig::Openai { .. } => Err(LlmError::structural(
            "OpenAI adapter not yet wired through build_adapter — \
             use the provider helpers in wacp-llm::providers::openai directly.",
        )),
        ProviderConfig::Generic { .. } => Err(LlmError::structural(
            "Generic adapter not yet wired through build_adapter.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_config_serde() {
        let json = r#"{"provider":"anthropic","api_key":"sk-test"}"#;
        let config: ProviderConfig = serde_json::from_str(json).unwrap();
        match config {
            ProviderConfig::Anthropic {
                api_key, base_url, ..
            } => {
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
            ProviderConfig::Generic {
                api_key, base_url, ..
            } => {
                assert!(api_key.is_none());
                assert_eq!(base_url, "http://localhost:11434");
            }
            _ => panic!("expected Generic"),
        }
    }

    #[test]
    fn stub_config_serde_with_path() {
        let json = r#"{"provider":"stub","fixtures_path":"/tmp/fx.yaml","token_delay_ms":5}"#;
        let config: ProviderConfig = serde_json::from_str(json).unwrap();
        match config {
            ProviderConfig::Stub {
                fixtures_path,
                token_delay_ms,
                default_model,
                ..
            } => {
                assert_eq!(
                    fixtures_path.as_deref(),
                    Some(std::path::Path::new("/tmp/fx.yaml"))
                );
                assert_eq!(token_delay_ms, 5);
                assert_eq!(default_model, "stub-model-1");
            }
            _ => panic!("expected Stub"),
        }
    }

    #[test]
    fn stub_config_defaults_when_empty() {
        let json = r#"{"provider":"stub"}"#;
        let config: ProviderConfig = serde_json::from_str(json).unwrap();
        match config {
            ProviderConfig::Stub {
                fixtures_path,
                fixtures_inline,
                default_model,
                token_delay_ms,
            } => {
                assert!(fixtures_path.is_none());
                assert!(fixtures_inline.is_none());
                assert_eq!(default_model, "stub-model-1");
                assert_eq!(token_delay_ms, 0);
            }
            _ => panic!("expected Stub"),
        }
    }

    #[tokio::test]
    async fn build_adapter_stub_from_inline() {
        let inline = StubFixtures::from_yaml(
            "version: 1\n\
             default:\n  content: \"ok\"\n  output_tokens: 1\n\
             entries: []\n",
        )
        .unwrap();
        let cfg = ProviderConfig::Stub {
            fixtures_path: None,
            fixtures_inline: Some(inline),
            default_model: "m".into(),
            token_delay_ms: 0,
        };
        let adapter = build_adapter(&cfg).unwrap();
        let health = adapter.health().await;
        assert!(health.healthy);
    }

    #[tokio::test]
    async fn build_adapter_stub_from_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fx.yaml");
        std::fs::write(
            &path,
            "version: 1\n\
             default:\n  content: \"ok\"\n  output_tokens: 1\n\
             entries: []\n",
        )
        .unwrap();
        let cfg = ProviderConfig::Stub {
            fixtures_path: Some(path),
            fixtures_inline: None,
            default_model: "m".into(),
            token_delay_ms: 0,
        };
        let adapter = build_adapter(&cfg).unwrap();
        let health = adapter.health().await;
        assert!(health.healthy);
    }

    #[tokio::test]
    async fn build_adapter_stub_inline_takes_precedence() {
        // Point fixtures_path at a non-existent file but set fixtures_inline —
        // the inline fixtures should win and the loader should not try to
        // read the path.
        let inline = StubFixtures {
            version: 1,
            default: Some(StubResponse {
                content: "inline".into(),
                output_tokens: 1,
                tool_calls: vec![],
            }),
            entries: vec![],
        };
        let cfg = ProviderConfig::Stub {
            fixtures_path: Some(PathBuf::from("/does/not/exist.yaml")),
            fixtures_inline: Some(inline),
            default_model: "m".into(),
            token_delay_ms: 0,
        };
        let adapter = build_adapter(&cfg).unwrap();
        let models = adapter.models().await.unwrap();
        assert_eq!(models[0].id, "m");
    }

    #[tokio::test]
    async fn build_adapter_anthropic_not_ready() {
        let cfg = ProviderConfig::Anthropic {
            api_key: "sk".into(),
            base_url: "https://api.anthropic.com".into(),
            default_model: None,
        };
        match build_adapter(&cfg) {
            Err(err) => assert!(err.message.contains("Anthropic adapter")),
            Ok(_) => panic!("expected structural error"),
        }
    }

    #[tokio::test]
    async fn build_adapter_openai_not_ready() {
        let cfg = ProviderConfig::Openai {
            api_key: "sk".into(),
            base_url: "https://api.openai.com".into(),
            default_model: None,
        };
        match build_adapter(&cfg) {
            Err(err) => assert!(err.message.contains("OpenAI adapter")),
            Ok(_) => panic!("expected structural error"),
        }
    }

    #[tokio::test]
    async fn build_adapter_generic_not_ready() {
        let cfg = ProviderConfig::Generic {
            api_key: None,
            base_url: "http://x".into(),
            default_model: None,
        };
        match build_adapter(&cfg) {
            Err(err) => assert!(err.message.contains("Generic adapter")),
            Ok(_) => panic!("expected structural error"),
        }
    }
}
