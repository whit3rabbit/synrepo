## ADDED Requirements

### Requirement: Expose synrepo_graph_neighborhood as a bounded graph model tool
The MCP server SHALL provide `synrepo_graph_neighborhood` as a read-only graph-backed tool. The tool SHALL accept optional `target`, optional `direction` (`both`, `inbound`, `outbound`), optional `edge_types`, optional `depth`, and optional `limit`, and SHALL return the shared graph-neighborhood model.

#### Scenario: Agent requests a target-centered graph neighborhood
- **WHEN** an agent calls `synrepo_graph_neighborhood` with `target = "handle_query"`
- **THEN** the tool resolves the target using the same target-resolution behavior as cards and CLI graph query
- **AND** it returns bounded graph-backed nodes and edges with provenance and epistemic labels

#### Scenario: Agent requests graph overview
- **WHEN** an agent calls `synrepo_graph_neighborhood` without a target
- **THEN** the tool returns a deterministic top-degree overview bounded by the default limit

#### Scenario: Agent requests too much graph context
- **WHEN** an agent calls `synrepo_graph_neighborhood` with depth or limit above the supported maximum
- **THEN** synrepo clamps depth to 3 and limit to 500
- **AND** marks the response as truncated when records were omitted

## MODIFIED Requirements

### Requirement: Expose synrepo_query as a structured graph query tool
The MCP server SHALL provide a `synrepo_query` tool that accepts a query string in the existing CLI graph query syntax (`outbound <target> [edge_kind]`, `inbound <target> [edge_kind]`) and returns the matching edges as JSON. This reuses the same query DSL already supported by the CLI `synrepo graph query` command. The `<target>` SHALL accept a node ID, file path, qualified symbol name, or short symbol name.

#### Scenario: Agent queries outbound edges with kind filter
- **WHEN** agent calls `synrepo_query` with `query = "outbound file_0000000000000042 Defines"`
- **THEN** the tool returns all `Defines` edges from that file node

#### Scenario: Agent queries inbound edges without kind filter
- **WHEN** agent calls `synrepo_query` with `query = "inbound sym_0000000000000024"`
- **THEN** the tool returns all inbound edges to that symbol

#### Scenario: Agent queries by symbol name
- **WHEN** agent calls `synrepo_query` with `query = "outbound handle_query"`
- **THEN** the tool resolves `handle_query` to its graph node
- **AND** the tool returns outbound edges from that node

#### Scenario: Agent provides a malformed query string
- **WHEN** agent calls `synrepo_query` with `query = "sideways file_123"`
- **THEN** the tool returns an error describing the expected syntax

### Requirement: Expose synrepo_node as a raw graph lookup tool
The MCP server SHALL provide a `synrepo_node` tool that accepts a node ID string and returns the full stored metadata for that node as JSON. The node ID SHALL be parsed using the display-format convention (`file_`, `sym_`, `concept_` prefix). Legacy `symbol_` IDs SHALL be accepted as input aliases for `sym_` IDs. If the ID does not parse or no node exists, the tool SHALL return a structured error.

#### Scenario: Agent looks up a file node by display ID
- **WHEN** agent calls `synrepo_node` with `id = "file_0000000000000042"`
- **THEN** the tool returns JSON with the FileNode fields: id, path, language, content_hash, file_class, path_history, git_intelligence, provenance

#### Scenario: Agent looks up a symbol node by display ID
- **WHEN** agent calls `synrepo_node` with `id = "sym_0000000000000024"`
- **THEN** the tool returns JSON with the SymbolNode fields: id, file_id, qualified_name, kind, signature, doc_comment, body_hash, last_change, provenance

#### Scenario: Agent provides a legacy symbol node ID
- **WHEN** agent calls `synrepo_node` with `id = "symbol_0000000000000024"`
- **THEN** the tool resolves the legacy alias
- **AND** the response uses canonical `sym_` output

#### Scenario: Agent provides an invalid node ID
- **WHEN** agent calls `synrepo_node` with `id = "invalid_123"`
- **THEN** the tool returns an error message listing the valid ID prefixes (file_, sym_, concept_)

#### Scenario: Agent provides a valid ID for a non-existent node
- **WHEN** agent calls `synrepo_node` with `id = "file_9999999999999999"`
- **THEN** the tool returns an error stating the node was not found
