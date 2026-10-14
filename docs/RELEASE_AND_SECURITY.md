# Release, integrity and operational security

## Release gate

A tagged release must pass the same preflight before any bundle is uploaded:

1. Rust format, check, Clippy with warnings denied and workspace tests with `--locked`.
2. Deterministic framework quality gate.
3. Multi-repository lifecycle smoke test, including cached workspace integrity and JSON/Mermaid/GraphML export.
4. Desktop contract tests and production frontend build.
5. Tauri backend check, Clippy and tests.
6. Windows, Linux and macOS Desktop bundles.
7. Windows, Linux and macOS CLI/MCP binaries.

Every artifact set contains `BUILD_INFO.json` and `SHA256SUMS.txt`.

## Signing and notarization

RepoSlice does not embed private signing credentials. Authenticode certificates, Apple Developer identities/notarization credentials and any updater signing keys must be supplied by the release owner through the CI secret store. Checksums prove artifact integrity after publication but do not replace platform code signing or prove authorship.

## Update policy

The source tree does not enable an automatic updater with a placeholder endpoint or key. Shipping an unsigned/updatable client before an operator controls the signing key and release endpoint would weaken the security model. Add an updater only after the production release channel, public verification key and rollback policy exist.

## Local crash reporting

Crash reporting is disabled by default. Setting `REPOSLICE_CRASH_REPORTING=local` installs a local panic reporter under the RepoSlice home directory. Reports include RepoSlice version, OS/architecture, panic location and panic message. They are never uploaded automatically and intentionally do not collect repository source or file contents.

## Repository execution boundary

Static analysis, MCP workspace status, generic workspace export and ordinary capsule verification never run code from an analyzed repository. Capsule verification may return a sandbox validation plan; those commands are suggestions only and must be executed explicitly by the operator inside an isolated environment.
