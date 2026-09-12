//! Vector index profile: the parameters that determine whether an existing
//! on-disk index is reusable, and where it lives.
//!
//! The profile key is `(provider, model, dim, precision, chunk_size,
//! normalizer_version)`. Switching any of these must produce a distinct
//! subdirectory so a stale index from a previous configuration never gets
//! silently reused.
//!
//! Storage layout: `.synrepo/index/vectors/<profile_key>/index.bin`
//! where `<profile_key>` is the first 16 hex chars of `blake3(...)` of the
//! canonicalized key string.

use serde::{Deserialize, Serialize};

use crate::config::SemanticEmbeddingProvider;

/// On-disk precision for stored vectors.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorPrecision {
    /// 32-bit float per dimension. Default.
    #[default]
    Float32,
    /// 8-bit signed integer per dimension plus a per-vector scale. Lower
    /// storage, slightly lossy; covered by the existing compression gate in
    /// `docs/EMBEDDINGS.md`.
    Int8,
}

impl VectorPrecision {
    /// Stable label used in profile keys, headers, and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Float32 => "float32",
            Self::Int8 => "int8",
        }
    }

    /// Single byte used in the on-disk header to tag the precision.
    pub fn header_tag(self) -> u8 {
        match self {
            Self::Float32 => 0,
            Self::Int8 => 1,
        }
    }

    /// Inverse of [`header_tag`]. Returns `None` for unknown bytes so the
    /// load path can fail closed with a precise error.
    pub fn from_header_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Float32),
            1 => Some(Self::Int8),
            _ => None,
        }
    }
}

/// Maximum number of source-text chars per embedding chunk. This module owns
/// the constant (rather than the feature-gated `chunk` module) so the profile
/// key stays available without the `semantic-triage` feature; `chunk.rs`
/// consumes it. Bumping it requires bumping [`NORMALIZER_VERSION`] so old and
/// new profiles never share a subdirectory.
pub const MAX_CHUNK_CHARS: usize = 512;

/// Format version of the embedder output that produced the stored vectors.
/// Today every supported provider normalizes to unit length, but if a future
/// provider stops normalizing the stored dot-product math would silently
/// degrade. The version forces a rebuild on semantic change.
pub const NORMALIZER_VERSION: u16 = 1;

/// Inputs that uniquely identify a vector profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VectorProfile {
    /// Embedding backend (onnx or ollama).
    pub provider: SemanticEmbeddingProvider,
    /// Configured model name.
    pub model: String,
    /// Expected vector dimension for this model.
    pub dim: u16,
    /// On-disk precision for stored vectors.
    pub precision: VectorPrecision,
    /// Maximum source-text characters per chunk used at build time.
    pub chunk_chars: usize,
    /// Embedder normalization format version; bumps when the semantic of
    /// stored vectors changes.
    pub normalizer_version: u16,
}

impl VectorProfile {
    /// Build the profile from the runtime inputs the embedding pipeline knows
    /// about. Chunk size and normalizer version are constants today; future
    /// tunables land here.
    pub fn from_inputs(
        provider: SemanticEmbeddingProvider,
        model: &str,
        dim: u16,
        precision: VectorPrecision,
    ) -> Self {
        Self {
            provider,
            model: model.to_string(),
            dim,
            precision,
            chunk_chars: MAX_CHUNK_CHARS,
            normalizer_version: NORMALIZER_VERSION,
        }
    }

    /// Resolve the on-disk profile for the current runtime configuration.
    /// This is the single authority for how config fields map to the profile
    /// tuple; build, load, health, watch, and cleanup paths all route
    /// through it.
    pub fn for_config(config: &crate::config::Config) -> Self {
        Self::from_inputs(
            config.semantic_embedding_provider,
            config.semantic_model.as_str(),
            config.embedding_dim,
            config.semantic_vector_precision,
        )
    }

    /// Canonical, stable, human-readable key used as the input to the hash.
    pub fn key_string(&self) -> String {
        format!(
            "{}-{}-d{}-p{}-c{}-n{}",
            self.provider.as_str(),
            self.model,
            self.dim,
            self.precision.as_str(),
            self.chunk_chars,
            self.normalizer_version
        )
    }

    /// First 16 hex chars of `blake3(key_string())`. Short enough to be
    /// readable in a path, long enough to make accidental collision across
    /// distinct profiles infeasible.
    pub fn profile_key(&self) -> String {
        let hash = blake3::hash(self.key_string().as_bytes());
        let bytes = hash.as_bytes();
        let mut out = String::with_capacity(16);
        for byte in &bytes[..8] {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    /// The directory this profile's index lives in, relative to the vectors
    /// root (`.synrepo/index/vectors`).
    pub fn relative_dir(&self) -> String {
        format!("{}-{}", self.profile_key(), self.short_label())
    }

    /// Short human-readable suffix appended to the hash so that `ls` of the
    /// vectors root makes the most-recently-used profile obvious.
    pub fn short_label(&self) -> String {
        let model_short: String = self
            .model
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        format!(
            "{provider}-{model}-d{dim}-{prec}",
            provider = self.provider.as_str(),
            model = model_short,
            dim = self.dim,
            prec = self.precision.as_str()
        )
    }
}

/// Path to a profile's `index.bin`, given the vectors root. Doesn't check
/// whether the file exists.
pub fn profile_index_path(
    vectors_root: &std::path::Path,
    profile: &VectorProfile,
) -> std::path::PathBuf {
    vectors_root.join(profile.relative_dir()).join("index.bin")
}

/// Path to the active config's vector index under
/// `<synrepo_dir>/index/vectors/<profile>/index.bin`. Pure path arithmetic;
/// does not check existence.
pub fn profile_index_path_for_config(
    synrepo_dir: &std::path::Path,
    config: &crate::config::Config,
) -> std::path::PathBuf {
    profile_index_path(
        &synrepo_dir.join("index/vectors"),
        &VectorProfile::for_config(config),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(precision: VectorPrecision, dim: u16) -> VectorProfile {
        VectorProfile::from_inputs(
            SemanticEmbeddingProvider::Onnx,
            "all-MiniLM-L6-v2",
            dim,
            precision,
        )
    }

    #[test]
    fn precision_round_trip_header_tag() {
        for tag in 0u8..=1 {
            let p = VectorPrecision::from_header_tag(tag).expect("known tag");
            assert_eq!(p.header_tag(), tag);
        }
        assert!(VectorPrecision::from_header_tag(2).is_none());
        assert!(VectorPrecision::from_header_tag(255).is_none());
    }

    #[test]
    fn precision_default_is_float32() {
        assert_eq!(VectorPrecision::default(), VectorPrecision::Float32);
        assert_eq!(VectorPrecision::default().as_str(), "float32");
    }

    #[test]
    fn profile_key_stable_and_distinguishes_dim() {
        let p384 = profile(VectorPrecision::Float32, 384);
        let p768 = profile(VectorPrecision::Float32, 768);
        assert_eq!(p384.profile_key(), p384.profile_key());
        assert_ne!(p384.profile_key(), p768.profile_key());
        assert_eq!(p384.profile_key().len(), 16);
    }

    #[test]
    fn profile_key_distinguishes_precision() {
        let f32 = profile(VectorPrecision::Float32, 384);
        let i8 = profile(VectorPrecision::Int8, 384);
        assert_ne!(f32.profile_key(), i8.profile_key());
    }

    #[test]
    fn profile_key_distinguishes_provider() {
        let onnx = VectorProfile::from_inputs(
            SemanticEmbeddingProvider::Onnx,
            "all-minilm",
            384,
            VectorPrecision::Float32,
        );
        let ollama = VectorProfile::from_inputs(
            SemanticEmbeddingProvider::Ollama,
            "all-minilm",
            384,
            VectorPrecision::Float32,
        );
        assert_ne!(onnx.profile_key(), ollama.profile_key());
    }

    #[test]
    fn profile_key_distinguishes_normalizer_version() {
        let mut p1 = profile(VectorPrecision::Float32, 384);
        p1.normalizer_version = 1;
        let mut p2 = p1.clone();
        p2.normalizer_version = 2;
        assert_ne!(p1.profile_key(), p2.profile_key());
    }

    #[test]
    fn relative_dir_includes_hash_and_label() {
        let p = profile(VectorPrecision::Int8, 384);
        let dir = p.relative_dir();
        assert!(dir.starts_with(&p.profile_key()));
        assert!(dir.contains("onnx"));
        assert!(dir.contains("d384"));
        assert!(dir.contains("int8"));
    }

    #[test]
    fn profile_index_path_is_vectors_root_relative_dir_index_bin() {
        let p = profile(VectorPrecision::Float32, 384);
        let root = std::path::Path::new("/tmp/vectors");
        let path = profile_index_path(root, &p);
        assert!(path.ends_with(format!("{}/index.bin", p.relative_dir())));
    }

    #[test]
    fn short_label_sanitizes_model_name() {
        let p = VectorProfile::from_inputs(
            SemanticEmbeddingProvider::Ollama,
            "namespace/odd name:v1",
            384,
            VectorPrecision::Float32,
        );
        let label = p.short_label();
        assert!(label.contains("namespace-odd-name-v1"));
        assert!(!label.contains('/'));
        assert!(!label.contains(' '));
        assert!(!label.contains(':'));
    }
}
