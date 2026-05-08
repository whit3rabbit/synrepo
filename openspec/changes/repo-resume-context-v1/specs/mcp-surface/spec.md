## ADDED Requirements

### Requirement: Expose Resume Context Through MCP
Synrepo SHALL expose `synrepo_resume_context(repo_root?, limit?, since_days?, budget_tokens?, include_notes?)` as a repo-addressable MCP tool that returns the repo resume packet.

#### Scenario: MCP caller requests resume context
- **WHEN** an MCP caller invokes `synrepo_resume_context`
- **THEN** synrepo returns the shared repo resume packet JSON shape
- **AND** the tool accepts `repo_root` for global/defaultless MCP sessions

#### Scenario: MCP caller uses default parameters
- **WHEN** the caller omits optional parameters
- **THEN** synrepo uses limit `10`, since-days `14`, budget-tokens `2000`, and includes note summaries
