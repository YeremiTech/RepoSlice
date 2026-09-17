# RepoSlice analysis architecture

## Stable hierarchy

RepoSlice keeps the existing hierarchy and UI contract:

`Workspace -> Repository -> Project Unit -> ProjectModel`

`ProjectModel` is the universal model consumed by the graph, capsule, CLI, Tauri and
React layers. Existing fields remain stable. `AnalysisMetadata` adds evidence,
confidence, framework-specific route details, runtime requirements and diagnostics
without forcing consumers to understand a particular framework.

## Analysis pipeline

1. The scanner walks a repository with one shared exclusion policy, ignores symlinks,
   and records files larger than 2 MiB as diagnostics instead of parsing them.
2. Project-unit discovery finds manifest boundaries. Application roots such as
   Laravel and Spring remain valid units even when they contain a nested frontend;
   descendant units are excluded from the parent model when scanning a workspace.
3. Language parsers produce syntax-level components and dependencies. They never
   decide that a framework is present.
4. Framework detectors score independent evidence. An adapter runs only after its
   detection is actionable: direct manifest/configuration/source evidence plus the
   required confidence threshold. Inference-only detections never activate an adapter.
5. Contributions are merged, deduplicated and sorted by stable IDs. The graph and
   capsule layers continue to consume the same universal collections.
6. Two bounded in-process cache paths avoid unnecessary rescans. Clean Git repositories
   first use a commit-aware model cache; RepoSlice verifies that the working tree has no
   tracked or untracked changes before reusing the model. Dirty or non-Git repositories
   fall back to the full fingerprint cache, whose fingerprint includes normalized relative
   paths, sizes and modification timestamps. On cache misses, common detectors share a
   bounded lowercase text index and the shared source-text cache uses hash-based lookup.
   Above that layer, `reposlice-workspace` persists the last complete `WorkspaceModel` and
   reuses each repository independently when its Git state/filesystem signature, engine
   version and analyzer-registry signature are still current. The persisted model carries a
   SHA-256 integrity value; malformed caches are quarantined and valid atomic-write backups can
   be recovered. A forced single-repository refresh replaces only that repository model and
   then recomputes global cross-project edges.
7. Workspace cross-project matching builds HTTP and package-relation indexes. HTTP matching rejects unresolved or external origins, resolves exact and parameterized routes deterministically, and omits ambiguous targets. Package matching correlates local project identities with explicit manifest dependencies and raises confidence when source imports corroborate the relation.
8. `reposlice-graph` validates dependency references, detects architectural cycles and owns both forward dependency slices and reverse impact slices. Workspace traversal composes internal edges with unambiguous HTTP and workspace-package relations.

## Extension contracts

`LanguageAnalyzer` and `FrameworkAdapter` are public contracts in `reposlice-core`.
Parsers and adapters return `AnalysisContribution`. `AnalyzerRegistry` accepts external
`LanguageAnalyzer` and `FrameworkAdapter` implementations without modifying the scanner
source, while `reposlice-sdk` exposes that registration through one public facade. Built-in
analyzers remain statically linked; this is an extension API, not runtime dynamic plugin loading.

Current separation:

- `reposlice-parser-java`: generic Java declarations and references.
- `reposlice-parser-typescript`: generic TS/TSX declarations and imports plus conservative
  HTTP-call discovery in TypeScript, JavaScript, Vue, Svelte and Astro sources.
- `reposlice-parser-php`: token-based PHP declarations, namespaces, imports,
  inheritance, traits, typed constructor parameters and method-call structure.
- `reposlice-parser-polyglot`: conservative generic declarations and internal module
  references for JavaScript, Python, C#, Ruby, Go and Rust.
- `reposlice-adapter-spring`: Spring component classification and HTTP mappings.
- `reposlice-adapter-laravel`: evidence-based detection, components, routes, resource
  expansion, constructor injection, Eloquent relationships, migrations and runtime.
- `reposlice-adapter-web`: isolated analyzers for Angular, React, NestJS, Express,
  Django, FastAPI, ASP.NET, Symfony, Rails, Go HTTP frameworks and Rust web frameworks.
  Each analyzer has its own evidence score and only contributes after reaching the
  activation threshold.

The PHP implementation uses a lexical token stream with balanced syntax structures;
it does not use regular expressions as a substitute for a parser. Its public index is
deliberately compatible with replacing the tokenizer by tree-sitter or another full
AST provider later without changing the universal model.

## Evidence and confidence

Evidence records its kind (`manifest`, `configuration`, `source`, `convention` or
`inference`), source path, explanation and local confidence. Framework confidence is
the bounded sum of independent signals. Laravel currently weighs the Composer
dependency most heavily and corroborates it with `artisan`, bootstrap, routes and
providers. A `.php` extension alone contributes no Laravel confidence.

Every framework entrypoint and inferred relationship carries metadata evidence. When
the analyzer cannot resolve a target safely, it omits the edge instead of inventing a
component. Recoverable limitations are returned as diagnostics.

## Determinism and safety

- IDs are derived from qualified names or a normalized-path FNV-1a hash.
- Components, entrypoints, dependencies, technologies and metadata are sorted and
  deduplicated before returning.
- `.git`, `.reposlice`, `vendor`, `node_modules`, build outputs, virtual environments,
  IDE caches and common framework caches are excluded.
- Symlinks are not followed.
- Git remotes are sanitized before entering the model or a capsule manifest.
- Capsule names include a bounded scope hash, builds are staged before replacement, common
  secret/key files are rejected, configuration-like manifests are checked for probable
  embedded secrets, and single-project manifests store a relative source root.
- New capsule manifests include deterministic source-tree and complete-payload fingerprints plus
  SHA-256 digests for both scopes. Verification recalculates all declared integrity values, validates
  the inventory, rejects symlinks and rejects absolute source roots. SHA-256 detects content drift
  with a cryptographic hash, but it is not a digital signature or proof of authorship.
- Parser input is capped at 2 MiB per source file; files remain counted and receive a
  diagnostic.
- Malformed source is handled as partial analysis; no fabricated data is emitted.

## Compatibility policy

Spring Boot support and the visual structure remain unchanged. New fields are additive
in the Rust model, Tauri DTO and TypeScript types. Fixed runtime booleans are retained
for existing UI code while a dynamic `tools` collection supports project-specific
capabilities.

Capsule verification is structural plus content-integrity verification. It validates the
manifest, target, environment marker, declared file count, deterministic source/payload
fingerprints, optional SHA-256 values, portable source root and symlink safety. Capsules that
already provide the legacy deterministic fingerprints remain compatible when SHA-256 is absent;
artifacts older than the base integrity fields must be regenerated. Verification does not claim
that application behavior, compilation or runtime startup has been verified.

## Analysis audit and Desktop session model

A repository scan persists its latest `AnalysisRecord` under the workspace storage. The record binds the result to an analysis ID, engine version, analyzer-registry signature, Git commit when available, model fingerprint, duration, coverage counters, diagnostics and graph-integrity counters. A successful scan propagates persistence failures instead of reporting a durable completion that cannot be recovered later.

The workspace layer persists the most recent complete `WorkspaceModel` in an atomic cache keyed by workspace. The cache is accepted only when its schema, engine version, analyzer-registry signature, registered repository set/path, per-repository state and model SHA-256 still match. Before committing a scan, the workspace layer revalidates the repository registry and every state signature so edits made during analysis cannot produce a model that mixes source revisions. Cross-project graph integrity must also pass before persistence. Tauri keeps the active model in bounded session state and restores a valid persisted model on demand/startup. Full scans reuse unchanged repository models; a single-repository refresh preserves all other repositories and then recomputes cross-project links. Repository mutations invalidate the persisted/session model. Dependency slices, reverse impact queries and workspace-capsule creation consume the loaded model instead of silently rescanning the repository.

Desktop exposes this information through the Audit view. Repository cards use the persisted record even when the full model is not currently loaded, while detailed evidence, framework detections, diagnostics and cycle paths require the current session model. Workspace graph integrity validates cross-project source and target units/components/entrypoints independently of each project-unit graph.


## Repository lifecycle, progress and cancellation

Managed clones live under RepoSlice storage. `update-repository` performs `git pull --ff-only` only for those managed clones and refuses to update when local tracked or untracked changes exist. Clone/update commands disable interactive Git credential prompts so unattended Desktop/CI operations fail cleanly instead of hanging. Removing an external local repository only unregisters it; physical deletion is restricted to managed clone paths. Desktop emits per-repository analysis progress, including a final snapshot-validation phase, and uses cooperative cancellation between repository phases, so cancellation does not persist a partially composed workspace model.



## Agent boundary and MCP

`reposlice-mcp` is a local-only read surface over cached workspace models. It binds to loopback, rejects non-loopback peers, caps request bodies, and exposes a bounded workspace-status tool. Mutation stays in the CLI/Desktop lifecycle APIs.


## Release and quality gates

The deterministic fixture gate, real-repository corpus, multi-repository lifecycle smoke test, frontend contracts and platform-specific Tauri checks are independent layers. The real performance corpus records cold/warm/P95 latency, throughput, model size and structural counts. Tagged releases are blocked until the preflight passes, then produce Desktop plus CLI/MCP artifacts on Windows, Linux and macOS with build provenance and SHA-256 checksums. Platform signing and notarization remain release-operator responsibilities because their private credentials are intentionally not stored in the repository.
