/// YAML frontmatter block required by the Agent Skills standard. Hosts
/// (Claude Code, Codex CLI, Cursor 2.4+, Windsurf, Gemini CLI) scan skill
/// directories at startup and read this block to decide when to lazy-load
/// the skill body. `name` and `description` are both required.
macro_rules! skill_frontmatter {
    () => {
        "---
name: synrepo
description: Use synrepo in repositories with a .synrepo/ directory. Prefer synrepo cards and search before reading source files cold.
---

"
    };
}

/// Shim content for new targets that don't have automatic MCP registration.
/// These use basic markdown with the synrepo doctrine embedded.
macro_rules! define_basic_shim {
    ($name:ident, $title:expr) => {
        pub(crate) const $name: &str = concat!(
            $title,
            "

synrepo precomputes a structural graph of this codebase from tree-sitter parsing and git history.

",
            crate::cli_support::agent_shims::doctrine::doctrine_block!(),
            "

## Using synrepo

- Run `synrepo init` to initialize the graph.
- Use `synrepo status` to check operational health.
- Use `synrepo search <query>` to find symbols and files.
- Use `synrepo node <id>` to inspect node metadata.
- Use `synrepo graph query \"outbound <node_id>\"` to see dependencies.
- Use `synrepo graph query \"inbound <node_id>\"` to see dependents.

For full MCP tool support, register the synrepo MCP server in your client configuration.
"
        );
    };
}

pub(crate) use define_basic_shim;
pub(crate) use skill_frontmatter;
