use serde::{Deserialize, Serialize};

/// OpenAI chat completion request.
#[derive(Serialize)]
pub(super) struct ChatRequest<'a> {
    pub(super) model: &'a str,
    pub(super) max_tokens: u32,
    pub(super) messages: Vec<ChatMessage<'a>>,
}

/// A single message in a chat completion request.
#[derive(Serialize)]
pub(super) struct ChatMessage<'a> {
    pub(super) role: &'a str,
    pub(super) content: &'a str,
}

/// OpenAI chat completion response.
#[derive(Deserialize)]
pub struct ChatResponse {
    /// Response ID (used by OpenRouter for generation stats).
    #[serde(default)]
    pub id: Option<String>,
    /// Completion choices.
    pub choices: Vec<Choice>,
    /// Token usage, if reported.
    #[serde(default)]
    pub usage: Option<ChatUsage>,
}

/// Token usage reported by the API.
#[derive(Deserialize)]
pub struct ChatUsage {
    /// Input/prompt tokens.
    #[serde(default)]
    pub prompt_tokens: u32,
    /// Output/completion tokens.
    #[serde(default)]
    pub completion_tokens: u32,
}

/// A single completion choice.
#[derive(Deserialize)]
pub struct Choice {
    /// The generated message.
    pub message: MessageContent,
}

/// Message content within a choice.
#[derive(Deserialize)]
pub struct MessageContent {
    /// Generated text content.
    #[serde(default)]
    pub content: Option<String>,
}
