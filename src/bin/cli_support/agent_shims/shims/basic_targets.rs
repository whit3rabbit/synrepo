use super::shared::{define_basic_shim, skill_frontmatter};

pub(crate) const GEMINI_SHIM: &str = concat!(
    skill_frontmatter!(),
    "\
# synrepo context (Gemini CLI)

synrepo precomputes a structural graph of this codebase from tree-sitter parsing and git history.

",
    crate::cli_support::agent_shims::doctrine::doctrine_block!(),
    "
## Using synrepo

- Run `synrepo init` to initialize the graph.
- Use `synrepo status` to check operational health.
- Use `synrepo search <query>` to find symbols and files.
- Use `synrepo node <target>` to inspect node metadata (`<target>` accepts file paths, qualified symbol names, or node IDs).
- Use `synrepo graph query \"outbound <target>\"` to see dependencies.
- Use `synrepo graph query \"inbound <target>\"` to see dependents.

For full MCP tool support, register the synrepo MCP server in your client configuration.
"
);

define_basic_shim!(
    GOOSE_SHIM,
    "# synrepo context (Goose)
"
);

define_basic_shim!(
    KIRO_SHIM,
    "# synrepo context (Kiro CLI)
"
);

define_basic_shim!(
    QWEN_SHIM,
    "# synrepo context (Qwen Code)
"
);

define_basic_shim!(
    JUNIE_SHIM,
    "# synrepo context (Junie)
"
);

define_basic_shim!(
    TABNINE_SHIM,
    "# synrepo context (Tabnine CLI)
"
);

define_basic_shim!(
    TRAE_SHIM,
    "# synrepo context (Trae)
"
);
