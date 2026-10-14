# RepoSlice

Analizador estático de repositorios para entender tecnologías, componentes,
entrypoints y dependencias sin ejecutar el código analizado. Permite consultar
arquitectura, relacionar proyectos y crear cápsulas verificables a partir de un
alcance seleccionado.

[![Rust](https://img.shields.io/badge/Rust-000000?style=flat&logo=rust)](https://www.rust-lang.org/)
[![CI](https://github.com/YeremiTech/RepoSlice/actions/workflows/ci.yml/badge.svg)](https://github.com/YeremiTech/RepoSlice/actions/workflows/ci.yml)
[![Release](https://github.com/YeremiTech/RepoSlice/actions/workflows/release-desktop.yml/badge.svg)](https://github.com/YeremiTech/RepoSlice/actions/workflows/release-desktop.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Versión base actual: `0.4.0`. La CI valida el motor Rust, la aplicación Desktop,
los contratos frontend y los artefactos de release. El estado de cada workflow
debe consultarse en GitHub antes de asumir que una ejecución está activa.

## Características

- Analiza repositorios localmente sin ejecutar su código.
- Detecta lenguajes, frameworks, tecnologías, componentes y entrypoints con evidencia.
- Descubre unidades de proyecto en repositorios multi-proyecto y monorepos.
- Relaciona servicios mediante rutas HTTP y dependencias explícitas de paquetes.
- Calcula dependencias requeridas (`slice`) e impacto inverso (`impact`) sobre el grafo.
- Exporta modelos de workspace a JSON, Mermaid y GraphML.
- Crea y verifica cápsulas con rutas portables, inventario, fingerprints y SHA-256.
- Expone el motor mediante CLI, SDK Rust, MCP local de solo lectura y Desktop Tauri.

## Instalación

Requisitos mínimos:

- Rust estable con Cargo, `rustfmt` y `clippy`.
- Git disponible en el `PATH`.
- Node.js 22 o posterior y npm para Desktop.
- Dependencias nativas de Tauri 2. En Windows se requieren WebView2 y las herramientas
  de compilación de Microsoft.

Clonar y verificar el motor:

```powershell
git clone https://github.com/YeremiTech/RepoSlice.git
Set-Location RepoSlice
cargo check --workspace --all-features --locked
```

## Uso rápido

### CLI

Analizar un repositorio local:

```powershell
cargo run --locked -p reposlice-cli -- scan .\ruta\al\proyecto
```

Obtener resultados para automatización:

```powershell
cargo run --locked -p reposlice-cli -- graph .\ruta\al\proyecto --format json
cargo run --locked -p reposlice-cli -- entrypoints .\ruta\al\proyecto --format json
```

Calcular alcance e impacto de un componente o entrypoint:

```powershell
cargo run --locked -p reposlice-cli -- slice .\ruta\al\proyecto --target component:client
cargo run --locked -p reposlice-cli -- impact .\ruta\al\proyecto --target component:client
```

Los objetivos aceptan identificadores `component:<id>` o el identificador de un
entrypoint. Las consultas principales aceptan `--format json` o `--json`.

### Workspaces

```powershell
cargo run --locked -p reposlice-cli -- create-workspace "Mi Workspace"
cargo run --locked -p reposlice-cli -- add-repository WORKSPACE_ID .\ruta\al\repositorio
cargo run --locked -p reposlice-cli -- scan-workspace WORKSPACE_ID
cargo run --locked -p reposlice-cli -- export-workspace WORKSPACE_ID --format mermaid --output workspace.mmd
```

Los workspaces persisten sus modelos localmente y validan su integridad antes de
reutilizar una cache.

### Desktop

```powershell
Set-Location .\apps\desktop
npm ci
npm run tauri:dev
```

Desktop ofrece las vistas **Resumen**, **Tecnologías** y **Endpoints**. Analiza una
ruta local o una URL HTTPS de un repositorio GitHub. La interfaz consume el mismo
modelo Rust que la CLI, pero no expone todas las operaciones avanzadas del motor.

Para desarrollo frontend y pruebas:

```powershell
npm run typecheck
npm test
npm run build
npm run test:browser
```

Para construir el instalador Tauri:

```powershell
npm run tauri:build
```

## Arquitectura e integraciones

- `reposlice-core` y `reposlice-scanner`: modelo universal y orquestación del análisis.
- `reposlice-workspace` y `reposlice-graph`: persistencia, integridad, slices e impacto.
- `reposlice-capsule`: creación, inventario y verificación de cápsulas.
- `reposlice-cli` y `reposlice-sdk`: interfaces de línea de comandos y API Rust.
- `reposlice-mcp`: consulta local de workspaces mediante MCP, solo lectura.
- `parsers`, `adapters` y `apps/desktop`: lenguajes, frameworks, Tauri y React.

La integración de VS Code delega el análisis a la CLI local mediante `execFile`; no
envía repositorios a un servicio externo. Todas las interfaces requieren las
herramientas locales del proyecto.

El núcleo mantiene la UI y Tauri fuera de sus dependencias; consulta
[ARCHITECTURE.md](ARCHITECTURE.md) para el pipeline completo.

## Compatibilidad y límites

La matriz detallada de lenguajes, frameworks, componentes y niveles está en
[SUPPORTED_TECHNOLOGIES.md](SUPPORTED_TECHNOLOGIES.md). El scanner produce L0–L3;
runtime y cápsulas requieren comprobaciones u operaciones posteriores.

El análisis es estático y no garantiza que el proyecto compile, arranque o se
comporte correctamente. Las relaciones HTTP externas, dinámicas o ambiguas se
omiten para evitar dependencias inventadas. Las URLs de GitHub usadas por Desktop
deben ser HTTPS y apuntar directamente a un repositorio.

## Verificación

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
node scripts/check-repository-hygiene.mjs
node scripts/check-release-version.mjs
```

Validaciones adicionales están documentadas en [quality/PERFORMANCE.md](quality/PERFORMANCE.md);
los fixtures viven en [`fixtures/analysis`](fixtures/analysis) y el corpus real se ejecuta en CI.

## Documentación

- [Arquitectura](ARCHITECTURE.md) y [tecnologías soportadas](SUPPORTED_TECHNOLOGIES.md)
- [Contribuir](CONTRIBUTING.md), [seguridad](SECURITY.md) y [release](docs/RELEASE_AND_SECURITY.md)
- [Iconos](docs/TECHNOLOGY_ICONS.md) y [changelog](CHANGELOG.md)
- [Integración VS Code](integrations/vscode/README.md) y [fixtures](fixtures/analysis/README.md)

## Licencia

RepoSlice se distribuye bajo la licencia [MIT](LICENSE).
