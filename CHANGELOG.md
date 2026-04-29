# Changelog

## Unreleased

- `synrepo setup <tool>` now uses global agent config by default when the target supports it. Use `--project` to keep MCP registration repo-local.
- Agent setup now delegates MCP, skill, and instruction ownership to `agent-config = "0.1"`, including upgrade adoption for legacy unowned installs.
