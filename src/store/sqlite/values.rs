use rusqlite::types::Type;

pub(super) fn row_usize(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<usize> {
    let value: i64 = row.get(idx)?;
    usize::try_from(value)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(idx, Type::Integer, Box::new(err)))
}
