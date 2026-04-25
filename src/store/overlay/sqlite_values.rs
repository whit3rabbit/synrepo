use rusqlite::types::Type;

pub(super) fn row_usize(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<usize> {
    let value: i64 = row.get(idx)?;
    usize::try_from(value)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(idx, Type::Integer, Box::new(err)))
}

pub(super) fn row_optional_u64(
    row: &rusqlite::Row<'_>,
    idx: usize,
) -> rusqlite::Result<Option<u64>> {
    let value: Option<i64> = row.get(idx)?;
    value
        .map(|value| {
            u64::try_from(value).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(idx, Type::Integer, Box::new(err))
            })
        })
        .transpose()
}

pub(super) fn usize_to_i64(value: usize, label: &str) -> crate::Result<i64> {
    i64::try_from(value).map_err(|err| {
        crate::Error::Other(anyhow::anyhow!(
            "{label} value {value} does not fit in SQLite INTEGER: {err}"
        ))
    })
}

pub(super) fn optional_u64_to_i64(value: Option<u64>, label: &str) -> crate::Result<Option<i64>> {
    value
        .map(|value| {
            i64::try_from(value).map_err(|err| {
                crate::Error::Other(anyhow::anyhow!(
                    "{label} value {value} does not fit in SQLite INTEGER: {err}"
                ))
            })
        })
        .transpose()
}
