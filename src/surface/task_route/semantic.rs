//! Feature-gated semantic task routing.

/// Best semantic intent match.
#[derive(Clone, Debug)]
pub(super) struct SemanticRouteMatch {
    pub(super) intent: String,
    pub(super) score: f32,
}

#[cfg(not(feature = "semantic-triage"))]
pub(super) fn classify(
    _task: &str,
    _config: &crate::config::Config,
    _synrepo_dir: &std::path::Path,
) -> Option<SemanticRouteMatch> {
    None
}

#[cfg(feature = "semantic-triage")]
mod enabled {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    use crate::substrate::embedding::model::{EmbeddingSession, ModelResolver};

    use super::SemanticRouteMatch;

    #[derive(Clone, Debug)]
    struct IntentCentroid {
        intent: &'static str,
        vector: Vec<f32>,
    }

    static CENTROIDS: OnceLock<Mutex<HashMap<String, Vec<IntentCentroid>>>> = OnceLock::new();

    pub(crate) fn classify(
        task: &str,
        config: &crate::config::Config,
        synrepo_dir: &std::path::Path,
    ) -> Option<SemanticRouteMatch> {
        if !config.enable_semantic_triage {
            return None;
        }

        let resolver = ModelResolver::new();
        let resolution = resolver.resolve_existing(config, synrepo_dir).ok()?;
        let session = EmbeddingSession::new_from_resolution(&resolution).ok()?;
        let key = format!(
            "{}:{}:{}:{}",
            config.semantic_embedding_provider.as_str(),
            config.semantic_ollama_endpoint,
            config.semantic_model,
            config.embedding_dim
        );
        let centroids = cached_centroids(&key, &session)?;
        let task_vec = session
            .embed(&[task.to_string()])
            .ok()?
            .into_iter()
            .next()?;

        centroids
            .iter()
            .map(|centroid| SemanticRouteMatch {
                intent: centroid.intent.to_string(),
                score: cosine(&task_vec, &centroid.vector),
            })
            .max_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    fn cached_centroids(key: &str, session: &EmbeddingSession) -> Option<Vec<IntentCentroid>> {
        let cache = CENTROIDS.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(found) = cache.lock().ok()?.get(key).cloned() {
            return Some(found);
        }

        let mut built = Vec::new();
        for (intent, examples) in intent_examples() {
            let texts = examples
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>();
            let vectors = session.embed(&texts).ok()?;
            built.push(IntentCentroid {
                intent,
                vector: mean_vector(&vectors)?,
            });
        }
        cache.lock().ok()?.insert(key.to_string(), built.clone());
        Some(built)
    }

    fn intent_examples() -> &'static [(&'static str, &'static [&'static str])] {
        &[
            (
                "var-to-const",
                &[
                    "convert all let bindings to constants",
                    "replace mutable var declarations with const",
                    "make local variables constant where possible",
                ],
            ),
            (
                "remove-debug-logging",
                &[
                    "remove debug logging statements",
                    "strip console logs from the code",
                    "delete temporary print debugging output",
                ],
            ),
            (
                "replace-literal",
                &[
                    "replace a repeated string literal",
                    "change this literal value everywhere",
                    "swap hardcoded literal with a new value",
                ],
            ),
            (
                "rename-local",
                &[
                    "rename a local variable",
                    "rename this local binding safely",
                    "change a local parameter name",
                ],
            ),
            (
                "test-surface",
                &[
                    "find tests likely to cover this change",
                    "show the test surface for this file",
                    "which tests should I run",
                ],
            ),
            (
                "risk-review",
                &[
                    "assess the risk of changing this module",
                    "review impact and dependencies",
                    "what could break if I edit this",
                ],
            ),
            (
                "context-search",
                &[
                    "find where this feature is implemented",
                    "locate the module that handles authentication",
                    "search the codebase for relevant files",
                ],
            ),
        ]
    }

    fn mean_vector(vectors: &[Vec<f32>]) -> Option<Vec<f32>> {
        let first = vectors.first()?;
        let mut out = vec![0.0; first.len()];
        for vector in vectors {
            if vector.len() != out.len() {
                return None;
            }
            for (slot, value) in out.iter_mut().zip(vector) {
                *slot += *value;
            }
        }
        let denom = vectors.len() as f32;
        for value in &mut out {
            *value /= denom;
        }
        Some(out)
    }

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        let a_norm = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let b_norm = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if a_norm <= f32::EPSILON || b_norm <= f32::EPSILON {
            0.0
        } else {
            dot / (a_norm * b_norm)
        }
    }
}

#[cfg(feature = "semantic-triage")]
pub(super) use enabled::classify;
