# Auditoria de estabilizacion

Fecha: 2026-10-14

## Alcance

Se reviso el repositorio local en Windows, conservando los cambios que ya estaban presentes en el arbol de trabajo. La migracion previa de los logos tecnologicos de PNG a WebP, incluyendo las eliminaciones de PNG, no se revirtio.

## Cambios realizados

### Motor Rust y analizadores

- La cache de fuentes y modelos ahora depende del contenido SHA-256, no solo del tamano o del estado superficial de Git.
- El scanner invalida caches cuando encuentra cambios con el mismo tamano y respeta las marcas `assume-unchanged` y `skip-worktree`.
- El workspace valida origenes de clonacion, rechaza credenciales, query strings, `file://` y HTTP inseguro, reutiliza repositorios por identidad remota y usa destinos deterministas.
- La validacion del grafo comprueba referencias a componentes y entrypoints entre proyectos.
- El adaptador Rust Web detecta metodos Axum repetidos y prefijos de rutas anidadas mediante `.nest(...)`.
- La salida JSON del CLI rechaza modelos con integridad invalida.
- La trazabilidad Desktop solo publica lineas de fuente unicas y verificadas.

### Seguridad y transporte

- MCP tiene handshake `initialize`, limites de cabeceras y conexiones, y origenes locales permitidos de forma explicita.
- Desktop acepta solamente URLs HTTPS validas de repositorios GitHub.
- Tauri usa CSP explicita y permisos de dialogo declarados.
- Los errores de clonacion no exponen stderr potencialmente sensible.

### Desktop y assets

- Los logos tecnologicos especificos se sirven localmente como WebP; los fallbacks de categoria conservan PNG.
- Se corrigieron resolucion por categoria, aliases ambiguos y pruebas de carga de assets.
- Se ajusto el test Playwright para seleccionar nombres de tecnologia exactos, evitando que `CSS` coincidiera con `SCSS`.
- La documentacion de README e iconos refleja el formato real de los assets.

### Calidad y CI

- `validate-real-corpus.ps1` valida filas, URLs GitHub HTTPS, rutas contenidas, JSON, umbrales y limpieza segura.
- `check-repository-hygiene.mjs` ignora arboles locales generados cuando no contienen archivos versionados, pero sigue detectando artefactos prohibidos versionados.
- CI y release incluyen typecheck, build y Playwright.
- `package-lock.json` actualiza `source-map-js` de `1.2.1` a `1.2.2`.

## Verificacion

### Rust

- `cargo fmt --all -- --check`: PASS
- `cargo check --workspace --all-features --locked`: PASS
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: PASS
- `cargo test --workspace --all-features --locked`: PASS
- Tauri `fmt`, `check`, `clippy` y `test`: PASS
- Tests Tauri: 6 passed, 0 failed

### Frontend

- `npm ci`: PASS
- `npm test`: 41 passed, 0 failed
- `npm run typecheck`: PASS
- `npm run build`: PASS
- Playwright: 7 passed, 0 failed
- `npm audit --audit-level=high`: 0 vulnerabilidades

### Calidad funcional

- `scripts/smoke-workspace.ps1`: PASS
- `scripts/validate-real-corpus.ps1`: 7/7 casos PASS
- `scripts/benchmark-real-corpus.ps1`: 3/3 casos PASS
- Benchmark local: 409 archivos, 1318 componentes, 1146 dependencias, 50.6 archivos/s
- `scripts/check-repository-hygiene.mjs`: PASS, 418 archivos revisados

## Limitaciones restantes

- `cargo audit` no se pudo ejecutar porque el subcomando no esta instalado en este entorno. `npm audit` si paso sin vulnerabilidades.
- La clonacion y validacion remota autenticada no se probaron con credenciales reales; las URLs con credenciales se rechazan por diseno.
- No se hizo commit ni push. El arbol sigue en `main` y conserva los cambios previos del usuario.
