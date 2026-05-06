## ADDED Requirements

### Requirement: Detect edited single-file renames without unbounded content LCS
The identity cascade SHALL attempt single-file rename detection before split/merge detection. It SHALL require same-root candidates and clear high overlap. Symbol-rich files SHALL use exact symbol-set Jaccard. Symbol-poor files MAY use bounded sampled-content shingle similarity with a hard file-size cap. The cascade SHALL NOT run full byte LCS in the hot path.

#### Scenario: One new file has high symbol overlap
- **WHEN** one disappeared file and one same-root new file share high symbol-set overlap
- **THEN** the old `FileNodeId` is preserved for the new path
- **AND** the old path is appended to `path_history`

#### Scenario: Large symbol-poor file is considered
- **WHEN** a symbol-poor disappeared file or new file exceeds the sampled-similarity size cap
- **THEN** sampled content similarity is skipped
- **AND** the cascade proceeds to later identity steps without quadratic work
