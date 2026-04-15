use serde::{Deserialize, Serialize};

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Content,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Content::Text(text.into()),
        }
    }
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Content::Text(text.into()),
        }
    }
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::Text(text.into()),
        }
    }
    pub fn tool_result(
        tool_use_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: Content::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.into(),
                content: content.into(),
                is_error,
            }]),
        }
    }
}

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Message content — text or structured blocks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Content {
    /// Plain text content.
    Text(String),
    /// Structured content blocks.
    Blocks(Vec<ContentBlock>),
}

impl Content {
    /// Extract as plain text (first text block or the string itself).
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Content::Text(s) => Some(s),
            Content::Blocks(blocks) => blocks.iter().find_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            }),
        }
    }
}

/// A content block within a message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

/// Tool definition for LLM function-calling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn message_constructors() {
        let sys = Message::system("you are helpful");
        assert_eq!(sys.role, Role::System);
        assert_eq!(sys.content.as_text().unwrap(), "you are helpful");

        let user = Message::user("hello");
        assert_eq!(user.role, Role::User);

        let asst = Message::assistant("hi there");
        assert_eq!(asst.role, Role::Assistant);

        let tool = Message::tool_result("call_1", "result data", false);
        assert_eq!(tool.role, Role::Tool);
    }

    #[test]
    fn role_serde_roundtrip() {
        for role in [Role::System, Role::User, Role::Assistant, Role::Tool] {
            let json = serde_json::to_string(&role).unwrap();
            let parsed: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, role);
        }
    }

    #[test]
    fn role_serde_snake_case() {
        assert_eq!(serde_json::to_string(&Role::System).unwrap(), "\"system\"");
        assert_eq!(serde_json::to_string(&Role::Tool).unwrap(), "\"tool\"");
    }

    #[test]
    fn content_text_serde() {
        let content = Content::Text("hello".into());
        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json, json!("hello"));
        let parsed: Content = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, content);
    }

    #[test]
    fn content_blocks_serde() {
        let content = Content::Blocks(vec![
            ContentBlock::Text {
                text: "hello".into(),
            },
            ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "read_file".into(),
                input: json!({"path": "/src/main.rs"}),
            },
        ]);
        let json = serde_json::to_value(&content).unwrap();
        let parsed: Content = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, content);
    }

    #[test]
    fn content_as_text_from_string() {
        let c = Content::Text("hello".into());
        assert_eq!(c.as_text(), Some("hello"));
    }

    #[test]
    fn content_as_text_from_blocks() {
        let c = Content::Blocks(vec![ContentBlock::Text {
            text: "world".into(),
        }]);
        assert_eq!(c.as_text(), Some("world"));
    }

    #[test]
    fn content_as_text_no_text_block() {
        let c = Content::Blocks(vec![ContentBlock::ToolResult {
            tool_use_id: "x".into(),
            content: "y".into(),
            is_error: false,
        }]);
        assert_eq!(c.as_text(), None);
    }

    #[test]
    fn message_serde_roundtrip() {
        let msg = Message::user("hello world");
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn tool_result_block_serde() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "call_1".into(),
            content: "file contents here".into(),
            is_error: false,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_result");
        assert_eq!(json["tool_use_id"], "call_1");
        let parsed: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, block);
    }

    #[test]
    fn tool_use_block_serde() {
        let block = ContentBlock::ToolUse {
            id: "call_1".into(),
            name: "read_file".into(),
            input: json!({"path": "/tmp"}),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_use");
        let parsed: ContentBlock = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, block);
    }

    #[test]
    fn message_with_empty_content() {
        let msg = Message::user("");
        assert_eq!(msg.content.as_text(), Some(""));
    }

    #[test]
    fn message_with_unicode_content() {
        let msg = Message::user("Hello 🌍! Ñoño café résumé");
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.content.as_text().unwrap(),
            "Hello 🌍! Ñoño café résumé"
        );
    }

    #[test]
    fn content_blocks_multiple_text_as_text_returns_first() {
        let c = Content::Blocks(vec![
            ContentBlock::Text {
                text: "first".into(),
            },
            ContentBlock::Text {
                text: "second".into(),
            },
        ]);
        assert_eq!(c.as_text(), Some("first"));
    }

    #[test]
    fn tool_result_is_error_true() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "call_1".into(),
            content: "error message".into(),
            is_error: true,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["is_error"], true);
    }

    #[test]
    fn tool_definition_serde() {
        let def = ToolDefinition {
            name: "read_file".into(),
            description: "Read a file".into(),
            input_schema: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        };
        let json = serde_json::to_string(&def).unwrap();
        let parsed: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, def);
    }
}
