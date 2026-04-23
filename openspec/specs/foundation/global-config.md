# Global Configuration

Synrepo supports both global (user-scoped) and local (project-scoped) configuration files. This allows users to share sensitive settings, such as LLM API keys and endpoints, across multiple repositories while maintaining granular control over repo-specific behavior.

## Precedence

1.  **Environment Variables**: Highest priority. Overrides both local and global config.
2.  **Local Config (`.synrepo/config.toml`)**: Repository-specific overrides.
3.  **Global Config (`~/.synrepo/config.toml`)**: Shared defaults.
4.  **Compiled Defaults**: Fallback when no configuration is provided.

## Configuration Merging

When a repository is initialized, `synrepo` loads the global config first and then merges the local config on top of it.

### Merging Rules

- **Boolean/Numeric/String Fields**: The local value completely replaces the global value if it differs from the compiled default.
- **Vectors (e.g., `roots`, `redact_globs`)**: The local vector replaces the global vector if it differs from the compiled default.
- **Explain Config (`[explain]`)**: Merged field-by-field. For example, if the global config has `provider = "anthropic"` and local has `enabled = true`, the resulting config will be enabled with the Anthropic provider.

## Gitignore Integration

During `synrepo init` or `synrepo setup`, users can pass the `--gitignore` flag to automatically add `.synrepo/` to the repository's root `.gitignore` file. This prevents accidental commits of local state or large graph databases while still allowing `config.toml` to be tracked if desired (though `.synrepo/.gitignore` by default ignores everything except `config.toml` and itself).
