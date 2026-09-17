# Contributing to RepoSlice

## Development setup

RepoSlice uses a Rust workspace for the analysis engine and a Tauri + React application for Desktop.

Requirements:

- stable Rust toolchain with `rustfmt` and `clippy`
- Node.js 22 or later and npm
- Git
- platform dependencies required by Tauri 2

Install the Desktop dependencies with:

```powershell
cd apps/desktop
npm ci
```

## Quality gate

Before opening a pull request, run the checks relevant to the files you changed. The complete gate used by CI is:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
node scripts/check-repository-hygiene.mjs
```

For Desktop:

```powershell
cd apps/desktop
npm ci
npm test
npm run build
```

## Repository hygiene

Do not commit build outputs, dependency directories, local runtime state, editor/assistant metadata, credentials, environment files, temporary files, exported conversations, generator banners, or generated Tauri schema/mobile-icon output. The root `.gitignore` and the repository hygiene check enforce these rules.

`Cargo.lock` and `apps/desktop/package-lock.json` are intentionally committed to keep application and CI builds reproducible.

Fixtures under `fixtures/analysis` are intentionally small and deterministic. Do not replace them with full third-party repositories.

## Security

Do not open a public issue containing credentials, private source code or sensitive repository contents. Follow `SECURITY.md` for security reports.
