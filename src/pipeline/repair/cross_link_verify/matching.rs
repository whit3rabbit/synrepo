use crate::overlay::CitedSpan;

/// Maximum length (in bytes) of a normalized span allowed for LCS verification.
/// Prevents O(N*M) CPU exhaustion on poisoned or oversized inputs.
pub const MAX_LCS_INPUT_LEN: usize = 4096;

pub fn verify_spans(source_text: &str, spans: &[CitedSpan]) -> Option<Vec<CitedSpan>> {
    let normalized_source = normalize_text(source_text);
    spans
        .iter()
        .map(|span| verify_span(source_text, &normalized_source, span))
        .collect()
}

pub fn verify_span(
    source_text: &str,
    normalized_source: &str,
    span: &CitedSpan,
) -> Option<CitedSpan> {
    let normalized_span = normalize_text(&span.normalized_text);
    if normalized_span.is_empty() || normalized_span.len() > MAX_LCS_INPUT_LEN {
        if normalized_span.len() > MAX_LCS_INPUT_LEN {
            tracing::warn!(
                len = normalized_span.len(),
                limit = MAX_LCS_INPUT_LEN,
                "skipping LCS verification for oversized span"
            );
        }
        return None;
    }

    // Stage A: exact substring match (normalised). O(N + M) — fast path for
    // verbatim citations, even in large sources.
    if normalized_source.contains(&normalized_span) {
        let offset = normalized_source
            .find(&normalized_span)
            .expect("contains check passed; qed");
        tracing::debug!(stage = "A", "exact substring match");
        return Some(CitedSpan {
            normalized_text: normalized_span,
            verified_at_offset: offset as u32,
            lcs_ratio: 1.0,
            ..span.clone()
        });
    }

    // Stage B: anchored partial match — find an anchor from the needle in the
    // source, then verify LCS on a window around each hit.
    if let Some((offset, ratio)) = anchored_partial_match(normalized_source, &normalized_span) {
        if ratio >= 0.9 {
            tracing::debug!(stage = "B", ratio, "anchored partial match");
            return Some(CitedSpan {
                normalized_text: normalized_span,
                verified_at_offset: offset as u32,
                lcs_ratio: ratio,
                ..span.clone()
            });
        }
    }

    // Stage C: windowed LCS fallback (budgeted). Only reached when stages A
    // and B produce no ratio >= 0.9.
    let (offset, ratio) = windowed_lcs_match(source_text, &normalized_span)?;
    if ratio < 0.9 {
        return None;
    }

    // Upgrade ratio to 1.0 when the span is an exact substring (handles
    // punctuation differences at word edges that cause the window-based LCS
    // to dip below 1.0).
    let final_ratio = if normalized_source.contains(&normalized_span) {
        1.0
    } else {
        ratio
    };

    tracing::debug!(stage = "C", ratio, "windowed LCS fallback");
    Some(CitedSpan {
        normalized_text: normalized_span,
        verified_at_offset: offset as u32,
        lcs_ratio: final_ratio,
        ..span.clone()
    })
}

/// Stage B: anchored partial match. Picks an anchor from the needle (first 16
/// bytes after trimming leading whitespace) and finds all occurrences in the
/// source. For each hit, evaluates LCS on a window of `needle.len() + 32`
/// bytes anchored at the hit. Returns the best ratio among hits >= 0.9,
/// or None if none qualify. Applies a 10 ms soft budget on the verification
/// loop.
pub fn anchored_partial_match(source: &str, needle: &str) -> Option<(usize, f32)> {
    let trimmed = needle.trim_start();
    if trimmed.len() < 16 {
        return None;
    }
    let anchor = &trimmed[..16];

    let mut best: Option<(usize, f32)> = None;
    let window_size = needle.len() + 32;
    let start = std::time::Instant::now();
    let mut hit_offset = 0;

    while let Some(pos) = source[hit_offset..].find(anchor) {
        hit_offset += pos;
        let window_start = hit_offset.saturating_sub(8);
        let window_end = (hit_offset + window_size).min(source.len());
        let window = &source[window_start..window_end];

        let ratio = lcs_ratio(window, needle);
        match best {
            Some((_, best_ratio)) if ratio <= best_ratio => {}
            _ => best = Some((window_start, ratio)),
        }

        // 10 ms soft budget on the per-hit loop.
        if start.elapsed() > std::time::Duration::from_millis(10) {
            tracing::warn!(
                stage = "B",
                anchor_hits = hit_offset,
                source_len = source.len(),
                needle_len = needle.len(),
                "Stage B budget trip"
            );
            break;
        }

        hit_offset += 1; // Move past this hit to find the next
    }

    // Return best match only if ratio >= 0.9, otherwise fall through to Stage C.
    best.and_then(|(offset, ratio)| {
        if ratio >= 0.9 {
            Some((offset, ratio))
        } else {
            None
        }
    })
}

/// Stage C: windowed LCS fallback. Same word-boundary windowed LCS algorithm,
/// but with the size cap removed and a soft time budget (50 ms). Returns the
/// best-so-far ratio (or None if none evaluated) on budget trip.
pub fn windowed_lcs_match(original_source: &str, needle: &str) -> Option<(usize, f32)> {
    // Normalize once and work with word boundaries on the normalized text.
    // This avoids calling normalize_text per-window inside the nested loop.
    let normalized = normalize_text(original_source);
    let words = word_boundaries(&normalized);
    let needle_words = needle.split(' ').count();
    if words.is_empty() || needle_words == 0 {
        return None;
    }

    let mut best: Option<(usize, f32)> = None;
    let min_words = needle_words.saturating_sub(2).max(1);
    let max_words = needle_words + 2;

    let started = std::time::Instant::now();
    let check_interval = 32;

    for (i, word_start) in words.iter().enumerate() {
        for width in min_words..=max_words {
            let end_idx = i + width;
            if end_idx > words.len() {
                break;
            }
            let start_byte = word_start.0;
            let end_byte = words[end_idx - 1].1;
            let window = &normalized[start_byte..end_byte];
            let ratio = lcs_ratio(window, needle);
            match best {
                Some((_, best_ratio)) if ratio <= best_ratio => {}
                _ => best = Some((start_byte, ratio)),
            }
        }

        // 50 ms soft budget on the full windowed LCS pass, checked every 32 iterations.
        if i > 0
            && i % check_interval == 0
            && started.elapsed() > std::time::Duration::from_millis(50)
        {
            tracing::warn!(
                stage = "C",
                iterations = i,
                source_len = original_source.len(),
                needle_len = needle.len(),
                best_ratio = best.map(|(_, r)| r),
                "Stage C budget trip"
            );
            return best;
        }
    }

    best
}

pub fn lcs_ratio(left: &str, right: &str) -> f32 {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let mut prev = vec![0usize; right.len() + 1];
    let mut curr = vec![0usize; right.len() + 1];
    for &left_byte in left {
        for (index, &right_byte) in right.iter().enumerate() {
            curr[index + 1] = if left_byte == right_byte {
                prev[index] + 1
            } else {
                prev[index + 1].max(curr[index])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }

    prev[right.len()] as f32 / left.len().max(right.len()) as f32
}

pub fn normalize_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn word_boundaries(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if start < index {
            out.push((start, index));
        }
    }

    out
}
