#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ConfigPresence {
    pub(super) semantic_provider: bool,
    pub(super) semantic_model: bool,
    pub(super) embedding_dim: bool,
    pub(super) semantic_ollama_endpoint: bool,
    pub(super) semantic_embedding_batch_size: bool,
    pub(super) semantic_vector_precision: bool,
    pub(super) reconcile_keepalive_seconds: bool,
}

impl ConfigPresence {
    pub(super) fn from_toml(text: &str) -> crate::Result<Self> {
        let value =
            toml::from_str::<toml::Value>(text).map_err(|e| crate::Error::Config(e.to_string()))?;
        let Some(table) = value.as_table() else {
            return Ok(Self::default());
        };
        Ok(Self {
            semantic_provider: table.contains_key("semantic_embedding_provider"),
            semantic_model: table.contains_key("semantic_model"),
            embedding_dim: table.contains_key("embedding_dim"),
            semantic_ollama_endpoint: table.contains_key("semantic_ollama_endpoint"),
            semantic_embedding_batch_size: table.contains_key("semantic_embedding_batch_size"),
            semantic_vector_precision: table.contains_key("semantic_vector_precision"),
            reconcile_keepalive_seconds: table.contains_key("reconcile_keepalive_seconds"),
        })
    }
}
