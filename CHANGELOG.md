# Changelog

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
