## ADDED Requirements

### Requirement: Expose synrepo_node as a raw graph lookup tool
The MCP server SHALL provide a `synrepo_node` tool that accepts a node ID string and returns the full stored metadata for that node as JSON. The node ID SHALL be parsed using the display-format convention (`file_`, `symbol_`, `concept_` prefix). If the ID does not parse or no node exists, the tool SHALL return a structured error.

#### Scenario: Agent looks up a file node by display ID
- **WHEN** agent calls `synrepo_node` with `id = "file_0000000000000042"`
- **THEN** the tool returns JSON with the FileNode fields: id, path, language, content_hash, file_class, path_history, git_intelligence, provenance

#### Scenario: Agent looks up a symbol node by display ID
- **WHEN** agent calls `synrepo_node` with `id = "symbol_0000000000000024"`
- **THEN** the tool returns JSON with the SymbolNode fields: id, file_id, qualified_name, kind, signature, doc_comment, body_hash, last_change, provenance

#### Scenario: Agent provides an invalid node ID
- **WHEN** agent calls `synrepo_node` with `id = "invalid_123"`
- **THEN** the tool returns an error message listing the valid ID prefixes (file_, symbol_, concept_)

#### Scenario: Agent provides a valid ID for a non-existent node
- **WHEN** agent calls `synrepo_node` with `id = "file_9999999999999999"`
- **THEN** the tool returns an error stating the node was not found

### Requirement: Expose synrepo_edges as a raw edge traversal tool
The MCP server SHALL provide a `synrepo_edges` tool that accepts a node ID string, an optional direction (`outbound` or `inbound`, defaulting to `outbound`), and an optional list of edge type filters. It SHALL return all matching edges with their full metadata including provenance.

#### Scenario: Agent traverses outbound edges from a node
- **WHEN** agent calls `synrepo_edges` with `id = "file_0000000000000042"` and no direction
- **THEN** the tool returns all outbound edges from that node, each with edge_kind, target node ID, and provenance

#### Scenario: Agent traverses inbound edges filtered by type
- **WHEN** agent calls `synrepo_edges` with `id = "symbol_0000000000000024"`, `direction = "inbound"`, and `edge_types = ["Calls"]`
- **THEN** the tool returns only inbound `Calls` edges targeting that symbol

#### Scenario: Agent traverses with multiple edge type filters
- **WHEN** agent calls `synrepo_edges` with `id = "file_0000000000000042"` and `edge_types = ["Defines", "Imports"]`
- **THEN** the tool returns only outbound edges of kind `Defines` or `Imports`

#### Scenario: Node has no matching edges
- **WHEN** agent calls `synrepo_edges` for a valid node that has no edges matching the filters
- **THEN** the tool returns an empty edges array

### Requirement: Expose synrepo_query as a structured graph query tool
The MCP server SHALL provide a `synrepo_query` tool that accepts a query string in the existing CLI graph query syntax (`outbound <id> [edge_kind]`, `inbound <id> [edge_kind]`) and returns the matching edges as JSON. This reuses the same query DSL already supported by the CLI `synrepo graph query` command.

#### Scenario: Agent queries outbound edges with kind filter
- **WHEN** agent calls `synrepo_query` with `query = "outbound file_0000000000000042 Defines"`
- **THEN** the tool returns all `Defines` edges from that file node

#### Scenario: Agent queries inbound edges without kind filter
- **WHEN** agent calls `synrepo_query` with `query = "inbound symbol_0000000000000024"`
- **THEN** the tool returns all inbound edges to that symbol

#### Scenario: Agent provides a malformed query string
- **WHEN** agent calls `synrepo_query` with `query = "sideways file_123"`
- **THEN** the tool returns an error describing the expected syntax

### Requirement: Expose synrepo_overlay as an overlay inspection tool
The MCP server SHALL provide a `synrepo_overlay` tool that accepts a node ID string and returns all overlay data associated with that node: commentary entry (if present) and proposed links with their status and confidence. If no overlay data exists, the tool SHALL return `{"overlay": null}` to distinguish absence from an error.

#### Scenario: Agent inspects a node with commentary and proposed links
- **WHEN** agent calls `synrepo_overlay` with `id = "file_0000000000000042"` and overlay data exists
- **THEN** the tool returns the commentary entry (text, confidence, freshness) and all proposed links with status, confidence tier, and source/target spans

#### Scenario: Agent inspects a node with no overlay data
- **WHEN** agent calls `synrepo_overlay` with `id = "symbol_0000000000000024"` and no overlay data exists
- **THEN** the tool returns `{"overlay": null}`

#### Scenario: Agent inspects a non-existent node
- **WHEN** agent calls `synrepo_overlay` with `id = "file_9999999999999999"`
- **THEN** the tool returns an error stating the node was not found in the graph

### Requirement: Expose synrepo_provenance as a provenance audit tool
The MCP server SHALL provide a `synrepo_provenance` tool that accepts a node ID string and returns the full provenance chain for that node and its incident edges. This includes the node's own provenance (source, created_by, source_ref) and for each incident edge, the edge's provenance and the peer node ID.

#### Scenario: Agent audits provenance for a node with edges
- **WHEN** agent calls `synrepo_provenance` with `id = "file_0000000000000042"`
- **THEN** the tool returns the node's provenance, plus a list of incident edges each with their provenance and the peer node ID

#### Scenario: Agent audits provenance for a node with no edges
- **WHEN** agent calls `synrepo_provenance` with `id = "concept_0000000000000099"` and the concept has no edges
- **THEN** the tool returns the node's provenance with an empty edges list

#### Scenario: Agent audits provenance for a non-existent node
- **WHEN** agent calls `synrepo_provenance` with `id = "file_9999999999999999"`
- **THEN** the tool returns an error stating the node was not found
