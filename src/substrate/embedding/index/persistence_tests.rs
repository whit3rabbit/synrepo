use std::fs;

use super::{
    persistence::{INDEX_FORMAT_VERSION, MAX_INDEX_CHUNK_TEXT_BYTES, MAX_INDEX_METADATA_LEN},
    FlatVecIndex,
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

fn header_bytes(metadata_len: u32, dim: u16) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&INDEX_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&metadata_len.to_le_bytes());
    bytes.extend_from_slice(&dim.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.push(1);
    bytes
}
