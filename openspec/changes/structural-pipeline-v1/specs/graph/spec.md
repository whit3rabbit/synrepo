## ADDED Requirements

### Requirement: Populate persisted graph facts automatically from repository state
synrepo SHALL run a deterministic structural compile that discovers eligible repository inputs, parses supported code and configured concept markdown, and writes the resulting canonical graph facts into the persisted graph store without requiring manual graph seeding.

#### Scenario: Initialize a repository and inspect the graph
- **WHEN** a user initializes synrepo in a repository that contains supported source files or configured concept markdown
- **THEN** the structural compile writes the resulting canonical graph facts into `.synrepo/graph/`
- **AND** later `synrepo node` or `synrepo graph query` calls can read those persisted facts without requiring test-only or manual graph insertion

### Requirement: Define the initial structural producer set
synrepo SHALL define the first automatic producer set for the structural compile, including file nodes, symbol nodes, `defines` edges, and human-declared concept nodes from configured concept directories.

#### Scenario: Compile a supported code file and an ADR markdown file
- **WHEN** the structural compile processes a supported code file and a markdown file in a configured concept directory
- **THEN** the code file can produce file nodes, symbol nodes, and `defines` edges
- **AND** the markdown file can produce a concept node and only directly-observed prose facts allowed by the graph contract

### Requirement: Refresh the produced graph slice deterministically
synrepo SHALL refresh the graph facts produced by the initial structural compile deterministically so repeated runs converge on current repository state rather than accumulating duplicate stale facts.

#### Scenario: Re-run the structural compile after a source change
- **WHEN** a user reruns initialization or another structural compile trigger after editing or removing previously observed files
- **THEN** the produced graph slice is refreshed to match current repository state
- **AND** repeated runs over unchanged inputs do not accumulate duplicate nodes or duplicate edges
