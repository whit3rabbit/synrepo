use std::fs;

use super::super::chunk::{ChunkId, EmbeddingChunkSource};
use super::super::profile::{VectorPrecision, NORMALIZER_VERSION};
use super::{
    persistence::{INDEX_FORMAT_VERSION, MAX_INDEX_CHUNK_TEXT_BYTES, MAX_INDEX_METADATA_LEN},
    ChunkMeta, FlatVecIndex,
};

#[test]
fn load_rejects_huge_metadata_len_before_allocating() -> crate::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let path = temp_dir.path().join("index.bin");
    fs::write(&path, header_bytes(u32::MAX, 384))?;

    let err = FlatVecIndex::load(&path, 384).unwrap_err();
    assert!(err.to_string().contains("metadata count"));
    Ok(())
}

#[test]
fn load_rejects_vector_payload_over_cap_before_allocating() -> crate::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let path = temp_dir.path().join("index.bin");
    fs::write(&path, header_bytes(MAX_INDEX_METADATA_LEN as u32, u16::MAX))?;

    let err = FlatVecIndex::load(&path, u16::MAX).unwrap_err();
    assert!(err.to_string().contains("vector payload"));
    Ok(())
}

#[test]
fn load_rejects_truncated_chunk_text_before_allocating() -> crate::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let path = temp_dir.path().join("index.bin");
    let mut bytes = header_bytes(1, 1);
    bytes.extend_from_slice(&1_u128.to_le_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&10_u32.to_le_bytes());
    bytes.extend_from_slice(b"abc");
    bytes.extend_from_slice(&0_f32.to_le_bytes());
    fs::write(&path, bytes)?;

    let err = FlatVecIndex::load(&path, 1).unwrap_err();
    assert!(err.to_string().contains("truncated before chunk text"));
    Ok(())
}

#[test]
fn load_rejects_excessive_chunk_text_len_before_allocating() -> crate::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let path = temp_dir.path().join("index.bin");
    let mut bytes = header_bytes(1, 1);
    bytes.extend_from_slice(&1_u128.to_le_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&((MAX_INDEX_CHUNK_TEXT_BYTES + 1) as u32).to_le_bytes());
    bytes.extend_from_slice(&0_f32.to_le_bytes());
    fs::write(&path, bytes)?;

    let err = FlatVecIndex::load(&path, 1).unwrap_err();
    assert!(err.to_string().contains("chunk text length"));
    Ok(())
}

/// Build the v6-format fixed header. Precision is `float32`, normalizer
/// version is 1. Test bodies append chunk and vector payloads.
fn header_bytes(metadata_len: u32, dim: u16) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&INDEX_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&metadata_len.to_le_bytes());
    bytes.extend_from_slice(&dim.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes()); // model name len
    bytes.push(1); // normalized
    bytes.push(0); // precision = float32 (header tag)
    bytes.extend_from_slice(&1_u16.to_le_bytes()); // normalizer version
    bytes
}

// Tests moved from persistence.rs inline `mod tests` to keep that file
// under the 400-line cap.

#[test]
fn persistence_v3_round_trip() -> crate::Result<()> {
    let index = FlatVecIndex {
        dim: 384,
        model_name: "test-model".into(),
        format_version: INDEX_FORMAT_VERSION,
        normalized: true,
        precision: VectorPrecision::Float32,
        normalizer_version: NORMALIZER_VERSION,
        chunks: vec![ChunkMeta {
            id: ChunkId(1),
            source: EmbeddingChunkSource::Symbol {
                id: crate::core::ids::SymbolNodeId(1),
                file_id: crate::core::ids::FileNodeId(1),
                qualified_name: "test::func".into(),
                kind_label: "function".into(),
            },
            text: "test::func function".into(),
        }],
        vectors: vec![0.1f32; 384],
        session: None,
    };

    let temp_dir = tempfile::tempdir()?;
    let path = temp_dir.path().join("index.bin");
    index.save(&path)?;

    let loaded = FlatVecIndex::load(&path, 384)?;
    assert_eq!(loaded.format_version, INDEX_FORMAT_VERSION);
    assert!(loaded.normalized);
    assert_eq!(loaded.model_name, "test-model");
    assert_eq!(loaded.len(), 1);

    Ok(())
}

#[test]
fn load_rejects_mismatched_expected_dim() -> crate::Result<()> {
    let index = FlatVecIndex {
        dim: 384,
        model_name: "test-model".into(),
        format_version: INDEX_FORMAT_VERSION,
        normalized: true,
        precision: VectorPrecision::Float32,
        normalizer_version: NORMALIZER_VERSION,
        chunks: vec![],
        vectors: vec![],
        session: None,
    };
    let temp_dir = tempfile::tempdir()?;
    let path = temp_dir.path().join("index.bin");
    index.save(&path)?;

    let err = FlatVecIndex::load(&path, 768).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("does not match expected"),
        "expected dim-mismatch error, got: {msg}"
    );
    Ok(())
}

#[test]
fn persistence_int8_round_trip_recovers_vectors_within_tolerance() -> crate::Result<()> {
    // Two chunked unit-ish vectors so we exercise per-vector scaling.
    // The first vector is asymmetric (positive-heavy); the second has a
    // negative max abs to ensure the symmetric quantizer handles both
    // signs. Each input is a unit-length vector so quantization error is
    // bounded by ~1/127 per dim.
    let dim = 8;
    let mut v0 = vec![0.0f32; dim];
    v0[0] = 0.9;
    v0[1] = 0.1;
    let mut v1 = vec![0.0f32; dim];
    v1[3] = -0.5;
    v1[5] = 0.866;

    let mut vectors = v0.clone();
    vectors.extend_from_slice(&v1);
    let chunks = vec![
        ChunkMeta {
            id: ChunkId(10),
            source: EmbeddingChunkSource::Symbol {
                id: crate::core::ids::SymbolNodeId(10),
                file_id: crate::core::ids::FileNodeId(1),
                qualified_name: "a".into(),
                kind_label: "function".into(),
            },
            text: "a".into(),
        },
        ChunkMeta {
            id: ChunkId(20),
            source: EmbeddingChunkSource::Symbol {
                id: crate::core::ids::SymbolNodeId(20),
                file_id: crate::core::ids::FileNodeId(1),
                qualified_name: "b".into(),
                kind_label: "function".into(),
            },
            text: "b".into(),
        },
    ];

    // The on-disk comparison below measures int8 vs float32 on the same
    // data, which is the contract the compression gate actually cares
    // about.
    let model_name = "test-int8";

    let index = FlatVecIndex {
        dim: dim as u16,
        model_name: model_name.to_string(),
        format_version: INDEX_FORMAT_VERSION,
        normalized: true,
        precision: VectorPrecision::Int8,
        normalizer_version: NORMALIZER_VERSION,
        chunks: chunks.clone(),
        vectors: vectors.clone(),
        session: None,
    };

    let temp_dir = tempfile::tempdir()?;
    let path = temp_dir.path().join("index.bin");
    index.save(&path)?;

    // The exact on-disk byte count depends on model_name length and
    // per-chunk text, which the surrounding code already pins. What
    // matters here is that int8 storage is materially smaller than the
    // equivalent float32 storage on the same data, and that the bytes
    // round-trip back into the same precision tag.
    let size_on_disk = std::fs::metadata(&path)?.len();
    assert!(
        size_on_disk > 0,
        "int8 file should not be empty, got {size_on_disk} bytes"
    );

    // Float32 write must be larger for this exact data.
    let index_f32 = FlatVecIndex {
        dim: dim as u16,
        model_name: model_name.to_string(),
        format_version: INDEX_FORMAT_VERSION,
        normalized: true,
        precision: VectorPrecision::Float32,
        normalizer_version: NORMALIZER_VERSION,
        chunks: chunks.clone(),
        vectors: vectors.clone(),
        session: None,
    };
    let path_f32 = temp_dir.path().join("index_f32.bin");
    index_f32.save(&path_f32)?;
    let size_f32 = std::fs::metadata(&path_f32)?.len();
    assert!(
        size_on_disk < size_f32,
        "int8 ({size_on_disk} bytes) must be smaller than float32 ({size_f32} bytes)"
    );

    // And the saving-growth ratio is bounded: at worst, every int8 vector
    // costs one scale byte more per vector than float32 would have saved.
    // With per-vector overhead 4+dim vs 4*dim, the saving for dim=8 is
    // 16 vs 32 per vector: int8 should be substantially under half.
    let savings_ratio = size_on_disk as f64 / size_f32 as f64;
    assert!(
        savings_ratio < 0.75,
        "int8 storage should be at least 25% smaller than float32, got ratio {savings_ratio:.3}"
    );

    let loaded = FlatVecIndex::load(&path, dim as u16)?;
    assert_eq!(loaded.precision, VectorPrecision::Int8);
    assert_eq!(loaded.vectors.len(), 2 * dim);

    // The dequantized vectors should match within rounding tolerance.
    for (orig, got) in v0.iter().chain(v1.iter()).zip(loaded.vectors.iter()) {
        let err = (orig - got).abs();
        assert!(
            err <= 1.0 / 126.0 + 1e-6,
            "int8 round-trip error too large: orig={orig} got={got} err={err}"
        );
    }
    Ok(())
}

#[test]
fn persistence_zero_vector_int8_round_trip() -> crate::Result<()> {
    // A zero vector would make max_abs == 0.0; we must not divide by zero.
    let dim = 4;
    let index = FlatVecIndex {
        dim: dim as u16,
        model_name: "zero".into(),
        format_version: INDEX_FORMAT_VERSION,
        normalized: true,
        precision: VectorPrecision::Int8,
        normalizer_version: NORMALIZER_VERSION,
        chunks: vec![ChunkMeta {
            id: ChunkId(0),
            source: EmbeddingChunkSource::Symbol {
                id: crate::core::ids::SymbolNodeId(0),
                file_id: crate::core::ids::FileNodeId(0),
                qualified_name: "z".into(),
                kind_label: "function".into(),
            },
            text: "z".into(),
        }],
        vectors: vec![0.0f32; dim],
        session: None,
    };
    let temp_dir = tempfile::tempdir()?;
    let path = temp_dir.path().join("index.bin");
    index.save(&path)?;
    let loaded = FlatVecIndex::load(&path, dim as u16)?;
    assert_eq!(loaded.precision, VectorPrecision::Int8);
    for v in loaded.vectors {
        assert_eq!(v, 0.0);
    }
    Ok(())
}

#[test]
fn load_rejects_unknown_precision_tag() -> crate::Result<()> {
    // Hand-craft a v6 header with a bad precision tag so we can confirm
    // load fails closed with a precise message.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&INDEX_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes()); // metadata_len
    bytes.extend_from_slice(&4u16.to_le_bytes()); // dim
    bytes.extend_from_slice(&0u32.to_le_bytes()); // model name len
    bytes.extend_from_slice(&[1u8]); // normalized
    bytes.extend_from_slice(&[99u8]); // bogus precision tag
    bytes.extend_from_slice(&1u16.to_le_bytes()); // normalizer_version

    let temp_dir = tempfile::tempdir()?;
    let path = temp_dir.path().join("index.bin");
    std::fs::write(&path, &bytes)?;
    let err = FlatVecIndex::load(&path, 4).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unknown precision tag"),
        "expected precision-tag error, got: {msg}"
    );
    Ok(())
}
