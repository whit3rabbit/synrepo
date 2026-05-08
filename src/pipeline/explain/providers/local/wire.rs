// Local-provider response types.

use serde::Deserialize;

#[derive(Deserialize)]
struct OpenAiCompatResponse {
    choices: Vec<OpenAiCompatChoice>,
    #[serde(default)]
    usage: Option<OpenAiCompatUsage>,
}

#[derive(Deserialize)]
struct OpenAiCompatChoice {
    message: OpenAiCompatMessage,
}

#[derive(Deserialize)]
struct OpenAiCompatMessage {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct OpenAiCompatUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}
