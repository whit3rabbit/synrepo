//! Per-vector quantization and payload sizing for the on-disk index.
//!
//! In-memory vectors are always `f32`; quantization is a persistence concern
//! applied at write time and inverted at load time.

use std::io::{Read, Write};

use super::super::profile::VectorPrecision;

pub(super) const F32_BYTES: u64 = 4;
const I8_BYTES: u64 = 1;

/// Bytes on disk for one vector under a given precision. Int8 stores one
/// `f32` scale per vector plus `dim` signed bytes; float32 stores `dim`
/// floats.
pub(super) fn vector_bytes_per_chunk(precision: VectorPrecision, dim: u16) -> u64 {
    let dim = dim as u64;
    match precision {
        VectorPrecision::Float32 => dim * F32_BYTES,
        VectorPrecision::Int8 => dim * I8_BYTES + F32_BYTES,
    }
}

/// Bytes on disk for the full vector payload. Fails closed on overflow so a
/// hostile header cannot panic the load path.
pub(super) fn vector_payload_bytes(
    metadata_len: usize,
    precision: VectorPrecision,
    dim: u16,
) -> crate::Result<u64> {
    (metadata_len as u64)
        .checked_mul(vector_bytes_per_chunk(precision, dim))
        .ok_or_else(|| {
            super::persistence::invalid_index("vector payload byte length overflows u64")
        })
}

/// Write one vector in the declared precision. Float32 is the literal `f32`
/// little-endian payload; int8 is `[scale: f32 LE][dim i8 bytes]` symmetric
/// quantization that uses the per-vector max absolute value as the scale.
pub(super) fn write_vector_payload<W: Write>(
    file: &mut W,
    vector: &[f32],
    precision: VectorPrecision,
) -> crate::Result<()> {
    match precision {
        VectorPrecision::Float32 => {
            for v in vector {
                file.write_all(&v.to_le_bytes())?;
            }
        }
        VectorPrecision::Int8 => {
            let scale = max_abs(vector);
            // Always write a scale; a zero vector would otherwise produce a
            // 0.0 scale and decode lossily back to all zeros (which is
            // correct, but we want the encoding to be the inverse of
            // `read_vector_payload` for non-zero vectors too).
            file.write_all(&scale.to_le_bytes())?;
            if scale == 0.0 {
                for _ in 0..vector.len() {
                    file.write_all(&[0i8 as u8])?;
                }
            } else {
                let inv_scale = 127.0_f32 / scale;
                for v in vector {
                    let q = (v * inv_scale).round();
                    let q = q.clamp(-127.0, 127.0) as i8;
                    file.write_all(&[q as u8])?;
                }
            }
        }
    }
    Ok(())
}

/// Inverse of [`write_vector_payload`]. Quantized int8 payloads are
/// dequantized back to `f32` so the in-memory representation is uniform.
pub(super) fn read_vector_payload<R: Read>(
    file: &mut R,
    dest: &mut [f32],
    precision: VectorPrecision,
    bytes_read: &mut u64,
) -> crate::Result<()> {
    match precision {
        VectorPrecision::Float32 => {
            let mut bits = [0u8; 4];
            for slot in dest.iter_mut() {
                super::persistence::read_exact_counted(file, &mut bits, bytes_read)?;
                *slot = f32::from_le_bytes(bits);
            }
        }
        VectorPrecision::Int8 => {
            let mut scale_bits = [0u8; 4];
            super::persistence::read_exact_counted(file, &mut scale_bits, bytes_read)?;
            let scale = f32::from_le_bytes(scale_bits);
            let mut q = [0u8; 1];
            for slot in dest.iter_mut() {
                super::persistence::read_exact_counted(file, &mut q, bytes_read)?;
                let q_signed = q[0] as i8;
                *slot = (q_signed as f32) * (scale / 127.0);
            }
        }
    }
    Ok(())
}

fn max_abs(vector: &[f32]) -> f32 {
    let mut m = 0.0_f32;
    for v in vector {
        let a = v.abs();
        if a > m {
            m = a;
        }
    }
    m
}
