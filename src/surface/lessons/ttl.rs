use crate::overlay::AgentNoteEvidence;

use super::{MAX_EVIDENCE_TEXT_CHARS, MAX_TTL_SECONDS};

pub fn parse_cli_ttl(raw: &str) -> crate::Result<u64> {
    let value = raw.trim();
    let Some(unit) = value.chars().last() else {
        return invalid_ttl(raw);
    };
    let number = &value[..value.len().saturating_sub(unit.len_utf8())];
    let amount: u64 = number.parse().map_err(|_| {
        crate::Error::Other(anyhow::anyhow!(
            "invalid ttl `{raw}`; expected a positive duration like 30d"
        ))
    })?;
    if amount == 0 {
        return invalid_ttl(raw);
    }
    let multiplier = match unit {
        'm' => 60,
        'h' => 60 * 60,
        'd' => 24 * 60 * 60,
        'w' => 7 * 24 * 60 * 60,
        _ => return invalid_ttl(raw),
    };
    let seconds = amount.checked_mul(multiplier).ok_or_else(|| {
        crate::Error::Other(anyhow::anyhow!("ttl `{raw}` exceeds the supported range"))
    })?;
    validate_ttl_seconds(seconds)?;
    Ok(seconds)
}

pub fn validate_ttl_seconds(seconds: u64) -> crate::Result<()> {
    if seconds == 0 || seconds > MAX_TTL_SECONDS {
        return Err(crate::Error::Other(anyhow::anyhow!(
            "ttl_seconds must be between 1 and {MAX_TTL_SECONDS}"
        )));
    }
    Ok(())
}

pub fn text_evidence(texts: &[String]) -> crate::Result<Vec<AgentNoteEvidence>> {
    texts
        .iter()
        .map(|text| {
            let trimmed = text.trim();
            if trimmed.chars().count() > MAX_EVIDENCE_TEXT_CHARS {
                return Err(crate::Error::Other(anyhow::anyhow!(
                    "evidence text exceeds {MAX_EVIDENCE_TEXT_CHARS} characters"
                )));
            }
            Ok(AgentNoteEvidence {
                kind: "text".to_string(),
                id: trimmed.to_string(),
            })
        })
        .collect()
}

fn invalid_ttl(raw: &str) -> crate::Result<u64> {
    Err(crate::Error::Other(anyhow::anyhow!(
        "invalid ttl `{raw}`; expected a positive duration like 30d"
    )))
}
