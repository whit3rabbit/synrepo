#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SemanticPresence {
    pub(super) provider: bool,
    pub(super) model: bool,
    pub(super) dim: bool,
    pub(super) ollama_endpoint: bool,
    pub(super) batch_size: bool,
}

impl SemanticPresence {
    pub(super) fn from_toml(text: &str) -> crate::Result<Self> {
        let value =
            toml::from_str::<toml::Value>(text).map_err(|e| crate::Error::Config(e.to_string()))?;
        let Some(table) = value.as_table() else {
            return Ok(Self::default());
        };
        Ok(Self {
            provider: table.contains_key("semantic_embedding_provider"),
            model: table.contains_key("semantic_model"),
            dim: table.contains_key("embedding_dim"),
            ollama_endpoint: table.contains_key("semantic_ollama_endpoint"),
            batch_size: table.contains_key("semantic_embedding_batch_size"),
        })
    }
}
