# Changelog

## 0.4.0 RC1 release hardening

- Added CI concurrency, timeouts and integrated Tauri no-bundle builds on Linux, Windows and macOS.
- Added a release version contract across npm, Tauri and all Cargo packages.
- Added downloadable real-corpus validation reports alongside performance baselines.
- Release tags now package Desktop and CLI/MCP artifacts per platform with build provenance and SHA-256 checksums, then publish a GitHub Release automatically.
- Added public CI, corpus and release status badges to the README.
- Removed the Architecture Intelligence module and its dedicated report, explainability, rules, history/diff and agent-context surfaces; generic workspace exports remain available.

## 0.4.0 RC1 Hotfix 1

- Added a contract regression test so local graph fields (`missing_sources` / `missing_targets`) cannot be reused accidentally for workspace-level integrity.

## 0.4.0 - V9 engineering candidate

- Added JSON, Mermaid and GraphML workspace exports.
- Added local-only, read-only `reposlice-mcp` using MCP revision 2026-07-28.
- Added a thin VS Code integration over the CLI.
- Added static capsule readiness and optional sandbox validation plans without automatic repository execution.
- Expanded observability and performance benchmarks to a real-project performance corpus.
- Added opt-in local crash reports and extended release artifacts to CLI/MCP binaries with checksums and provenance.
- Unified project package version at 0.4.0.
