## Purpose
Define the ModuleCard contract, budget-tier truncation rules, and source-labeling requirements for directory-scoped context packets.

## Requirements

### Requirement: Compile a ModuleCard from graph-derived directory facts
synrepo SHALL compile a `ModuleCard` for a given directory path by reading `FileNode` rows whose paths are direct children of that directory, listing the top-level public symbols in each file, and recording any immediate subdirectories as nested module references. All fields SHALL be sourced exclusively from the graph (`source_store: "graph"`). No LLM involvement and no overlay content SHALL appear in a `ModuleCard`.

#### Scenario: Request a ModuleCard for an existing directory
- **WHEN** `module_card(path)` is called with a directory path that contains indexed files
- **THEN** the returned card lists all files directly under that path
- **AND** each file entry includes its `FileNodeId` and repo-relative path
- **AND** the `source_store` field is `"graph"`

#### Scenario: Request a ModuleCard for a directory with no indexed files
- **WHEN** `module_card(path)` is called with a directory path that has no `FileNode` rows as direct children
- **THEN** the card is returned with an empty file list and a `files_count` of 0
- **AND** no error is raised

#### Scenario: ModuleCard does not recurse into subdirectories
- **WHEN** `module_card(path)` is called for a directory with nested subdirectories
- **THEN** files inside subdirectories are NOT included in the `files` list
- **AND** subdirectory paths are included as `nested_modules` references so callers can request their cards explicitly

### Requirement: Apply budget-tier truncation to ModuleCard
synrepo SHALL truncate `ModuleCard` content according to the requested budget tier, trimming lower-priority fields first to stay within the declared per-card token limit.

#### Scenario: Return a tiny ModuleCard
- **WHEN** a `ModuleCard` is requested at `tiny` budget
- **THEN** the card includes only the file list (paths and IDs) and the count of public symbols per file
- **AND** individual symbol names and signatures are omitted

#### Scenario: Return a normal ModuleCard
- **WHEN** a `ModuleCard` is requested at `normal` budget
- **THEN** the card includes the file list and the names and kinds of top-level public symbols in each file
- **AND** symbol signatures and doc comments are omitted

#### Scenario: Return a deep ModuleCard
- **WHEN** a `ModuleCard` is requested at `deep` budget
- **THEN** the card includes the full public symbol list with names, kinds, and one-line signatures
- **AND** doc comments are included, truncated to 120 characters per symbol
