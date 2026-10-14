# Security policy

RepoSlice is a local-first static analysis tool. Report security issues through a private channel provided by the repository owner rather than opening a public issue containing exploit details or credentials.

## Security boundaries

- Repository scanning does not execute analyzed code.
- Symlinks are not followed by the scanner or accepted in capsule payloads.
- Common secret/key files are excluded from capsules and configuration-like files receive conservative secret-value checks.
- Git commands use argument arrays, sanitize remotes in persisted metadata and disable interactive credential prompts for managed operations.
- External local repositories can be unregistered but are never physically removed by RepoSlice.
- MCP binds only to loopback and exposes read-only model queries.
- Cache/history payloads use SHA-256 integrity checks and invalid cache data is rejected/quarantined.

## Release security

Release bundles include SHA-256 checksums and build provenance. Platform signing/notarization depends on credentials owned by the release operator and is intentionally not faked or embedded in source control.

## Local diagnostics

`REPOSLICE_CRASH_REPORTING=local` enables local-only panic reports. RepoSlice does not transmit them automatically.
