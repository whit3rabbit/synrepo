# Keychain integration design

This note defines the intended credential-storage upgrade for explain provider keys. It is not implemented yet.

## Goals

- Keep existing `~/.synrepo/config.toml` readable.
- Preserve environment-variable precedence.
- Move newly saved cloud provider keys out of plaintext config when an OS credential store is available.
- Keep local endpoints in config, because they are routing choices rather than secrets.
- Provide an explicit plaintext fallback for headless systems and unsupported platforms.

## Lookup order

1. Provider environment variables, for example `ANTHROPIC_API_KEY`.
2. Legacy provider environment variables, for example `SYNREPO_ANTHROPIC_API_KEY`.
3. OS keychain item for service `synrepo`, account `explain:<provider>`.
4. Plaintext user-global config field, for backward compatibility.

Repo-local `.synrepo/config.toml` remains limited to explain enablement, provider, model, and local endpoint settings.

## Platform backends

- macOS: Keychain Services, scoped to the current login keychain.
- Windows: Credential Manager, using a generic credential target such as `synrepo/explain/<provider>`.
- Linux: Secret Service over D-Bus, using collection default and schema fields `{ app = "synrepo", kind = "explain", provider = "<provider>" }`.

If a backend is unavailable, setup should ask before falling back to plaintext config. Non-interactive setup should fail unless passed an explicit plaintext opt-in flag.

## Migration

On first setup after keychain support lands:

1. Detect provider key fields in `~/.synrepo/config.toml`.
2. If an OS keychain backend is available, offer to move each key into the keychain.
3. After a successful write and read-back verification, remove the plaintext field from config.
4. If write-back fails, leave the plaintext config unchanged and report the backend error.

Normal reads should continue accepting plaintext config until a later major compatibility window says otherwise.

## Operational behavior

- `synrepo setup --explain` saves entered cloud keys to the keychain by default when possible.
- `synrepo setup --explain --plaintext-keys` can be added for CI or unsupported hosts that need file-backed secrets.
- `synrepo status --json` must never include key material or keychain item names beyond provider availability.
- Explain telemetry must continue to record provider, model, usage, and outcome only.

## Testing

- Unit-test lookup precedence with fake backends.
- Integration-test migration against temporary plaintext config and a fake keychain implementation.
- Gate real platform-backend tests behind target cfgs and skip them when the host store is unavailable.
