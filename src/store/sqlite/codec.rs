use rusqlite::{Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

pub(crate) fn load_row<T, P>(conn: &Connection, sql: &str, params: P) -> crate::Result<Option<T>>
where
    T: DeserializeOwned,
    P: rusqlite::Params,
{
    let mut stmt = conn.prepare_cached(sql)?;
    stmt.query_row(params, |row| row.get::<_, String>(0))
        .optional()?
        .map(|json| decode_json(&json))
        .transpose()
}

pub(crate) fn load_rows<T, P>(conn: &Connection, sql: &str, params: P) -> crate::Result<Vec<T>>
where
    T: DeserializeOwned,
    P: rusqlite::Params,
{
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt
        .query_map(params, |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    rows.into_iter().map(|json| decode_json(&json)).collect()
}

pub(crate) fn encode_json<T: Serialize>(value: &T) -> crate::Result<String> {
    serde_json::to_string(value).map_err(|error| {
        crate::Error::Other(anyhow::anyhow!("failed to encode graph row: {error}"))
    })
}

fn decode_json<T: DeserializeOwned>(json: &str) -> crate::Result<T> {
    serde_json::from_str(json).map_err(|error| {
        crate::Error::Other(anyhow::anyhow!("failed to decode graph row: {error}"))
    })
}

pub(crate) fn encode_label<T: Serialize>(value: &T) -> crate::Result<String> {
    let json = serde_json::to_value(value).map_err(|error| {
        crate::Error::Other(anyhow::anyhow!("failed to encode graph label: {error}"))
    })?;

    json.as_str().map(ToOwned::to_owned).ok_or_else(|| {
        crate::Error::Other(anyhow::anyhow!("graph label did not serialize to a string"))
    })
}
