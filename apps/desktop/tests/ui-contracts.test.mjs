import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const src = (file) => fs.readFileSync(path.join(root, file), 'utf8');

test('technology icons resolve from the local PNG catalog', () => {
  const catalog = JSON.parse(src('src/lib/assets.json'));
  assert.ok(catalog.technologies.length >= 50, 'expected broad technology catalog');
  assert.doesNotMatch(JSON.stringify(catalog), /https?:\/\//);
  for (const {slug, path: asset} of catalog.technologies) {
    assert.ok(asset.endsWith('.png'), `${slug} should use a PNG asset`);
    assert.ok(fs.existsSync(path.join(root, 'public', 'assets', asset)), `missing local technology icon ${asset}`);
  }
});

test('all frontend invoke commands are registered in the active Tauri handler', () => {
  const client = src('src/lib/client.ts');
  const app = src('src/App.tsx');
  const tauri = src('src-tauri/src/lib.rs');
  const invokes = new Set([...`${client}\n${app}`.matchAll(/invoke(?:<[^>]+>)?\(\s*"([^"]+)"/g)].map((m) => m[1]));
  const handler = tauri.match(/tauri::generate_handler!\[([\s\S]*?)\]\)/)?.[1] ?? '';
  assert.ok(invokes.size > 0);
  assert.ok(handler, 'Tauri generate_handler block was not found');
  for (const command of invokes) {
    assert.match(handler, new RegExp(`\\b${command}\\b`), `Tauri command is defined but not registered: ${command}`);
  }
  const registered = handler.split(',').map((value) => value.trim()).filter(Boolean);
  assert.equal(new Set(registered).size, registered.length, 'Tauri generate_handler contains duplicate commands');
});

test('desktop exposes all primary analysis views', () => {
  const app = src('src/App.tsx');
  for (const view of ['Resumen', 'Tecnologías', 'Endpoints']) assert.ok(app.includes(view));
  assert.doesNotMatch(app, /<Sidebar|CapsulesView|ArchitectureView|DependenciesView|AuditView/);
});

test('styles are layered and readability overrides protect dense views', () => {
  const entry = src('src/styles/index.css');
  assert.match(entry, /--surface-0/);
  assert.match(entry, /\.png-image[\s\S]*object-fit:\s*contain/);
  assert.match(entry, /:focus-visible/);
  assert.match(entry, /@media/);
});

test('legacy workspace lifecycle remains available beside focused analysis', () => {
  const client = src('src/lib/client.ts');
  const app = src('src/App.tsx');
  const tauri = src('src-tauri/src/lib.rs');

  for (const command of [
    'load_cached_workspace_command',
    'update_repository_command',
    'delete_repository_command',
    'cancel_analysis_command'
  ]) {
    assert.match(client, new RegExp(`"${command}"`), `client missing ${command}`);
    assert.match(tauri, new RegExp(`\\b${command}\\b`), `Tauri backend missing ${command}`);
  }

  assert.match(app, /analyzeSource/);
  assert.match(app, /busy/);
});

test('focused desktop interactions validate source and expose accessible state', () => {
  const app = src('src/App.tsx');
  assert.match(app, /aria-current/);
  assert.match(app, /role="alert"/);
  assert.match(app, /disabled=\{busy\}/);
  assert.match(app, /running\.current/);
  assert.match(app, /preventDefault/);
});

test('framework quality corpus covers the extended supported matrix', () => {
  const corpus = src('../../quality/framework-ground-truth.tsv');
  const cases = corpus.split(/\r?\n/).filter((line) => line.trim()).length;
  assert.ok(cases >= 34, `expected at least 34 framework quality cases, found ${cases}`);
  for (const framework of ['Next.js', 'Vue', 'Nuxt', 'SvelteKit', 'Astro', 'Fastify', 'Hono', 'Flask', 'Quarkus', 'Micronaut', 'Ktor', 'Blazor', 'Phoenix']) {
    assert.ok(corpus.includes(framework), `quality corpus missing ${framework}`);
  }
});

test('desktop release workflow gates, packages and publishes Linux Windows and macOS releases', () => {
  const workflow = src('../../.github/workflows/release-desktop.yml');
  assert.match(workflow, /ubuntu-latest/);
  assert.match(workflow, /windows-latest/);
  assert.match(workflow, /macos-latest/);
  assert.match(workflow, /needs: preflight/);
  assert.match(workflow, /check-release-version\.mjs/);
  assert.match(workflow, /cargo clippy --workspace --all-targets --all-features --locked -- -D warnings/);
  assert.match(workflow, /framework_quality_corpus_meets_thresholds/);
  assert.match(workflow, /npm run tauri:build/);
  assert.match(workflow, /SHA256SUMS\.txt/);
  assert.match(workflow, /BUILD_INFO\.json/);
  assert.match(workflow, /GITHUB_SHA/);
  assert.match(workflow, /actions\/upload-artifact@v4/);
  assert.match(workflow, /actions\/download-artifact@v4/);
  assert.match(workflow, /Publish GitHub Release/);
  assert.match(workflow, /gh release create/);
  assert.match(workflow, /--verify-tag/);
});

test('release version contract keeps Rust Tauri and npm package versions synchronized', () => {
  const script = src('../../scripts/check-release-version.mjs');
  assert.match(script, /package-lock\.json/);
  assert.match(script, /tauri\.conf\.json/);
  assert.match(script, /Cargo\.toml/);
  assert.match(script, /RELEASE_TAG/);
  assert.match(script, /Version contract: PASS/);
});

test('workspace persistence rejects drift and verifies cache integrity', () => {
  const workspace = src('../../crates/reposlice-workspace/src/lib.rs');
  assert.match(workspace, /WORKSPACE_MODEL_CACHE_SCHEMA:\s*u32\s*=\s*3/);
  assert.match(workspace, /model_sha256:\s*String/);
  assert.match(workspace, /ensure_workspace_snapshot_unchanged/);
  assert.match(workspace, /validate_workspace_graph\(&model\)/);
  assert.match(workspace, /quarantine_corrupt_cache/);
  assert.match(workspace, /stage:\s*"validating"/);
  assert.match(workspace, /GIT_TERMINAL_PROMPT",\s*"0"/);
});

test('workspace lifecycle smoke gate is part of CI and release preflight', () => {
  const ci = src('../../.github/workflows/ci.yml');
  const release = src('../../.github/workflows/release-desktop.yml');
  const smoke = src('../../scripts/smoke-workspace.ps1');
  const cli = src('../../crates/reposlice-cli/src/main.rs');
  assert.match(ci, /smoke-workspace\.ps1/);
  assert.match(release, /smoke-workspace\.ps1/);
  assert.match(smoke, /scan-workspace/);
  assert.match(smoke, /scan-repository/);
  assert.match(smoke, /validate-workspace/);
  assert.match(smoke, /remove-repository/);
  assert.match(cli, /Some\("validate-workspace"\)/);
  assert.match(cli, /Some\("repositories"\)/);
});

test('primary bilingual views keep user-facing copy in the translation catalog', () => {
  const files = [
    'src/views/AuditView.tsx',
    'src/views/CapsulesView.tsx',
    'src/views/ComponentsView.tsx',
    'src/views/EntrypointsView.tsx'
  ];
  const combined = files.map(src).join('\n');
  for (const phrase of [
    'requieren atención',
    'Sin incidencias registradas',
    'Volumen realmente incorporado al análisis',
    'Artefactos reproducibles y su estado de verificación',
    'Separa símbolos arquitectónicos de archivos de soporte',
    'Puntos de entrada detectados con trazabilidad hacia el código fuente'
  ]) {
    assert.ok(!combined.includes(phrase), `hardcoded localized copy remains: ${phrase}`);
  }
});


test('removed intelligence module leaves no desktop or engine surface behind', () => {
  const app = src('src/App.tsx');
  const sidebar = src('src/components/Sidebar.tsx');
  const client = src('src/lib/client.ts');
  const tauri = src('src-tauri/src/lib.rs');
  const types = src('src/types.ts');
  assert.doesNotMatch(sidebar, /id:\s*"intelligence"/);
  assert.doesNotMatch(app, /IntelligenceView|intelligenceTargets/);
  assert.doesNotMatch(client, /architecture_intelligence_command|explain_target_command|workspace_history|agent_context|architecture_rules/);
  assert.doesNotMatch(tauri, /architecture_intelligence_command|explain_target_command|workspace_history|agent_context|architecture_rules/);
  assert.doesNotMatch(types, /IntelligenceReport|ArchitectureFinding|ArchitectureExplanation|AgentContext|WorkspaceHistoryEntry|ArchitectureDiff/);
  assert.equal(fs.existsSync(path.join(root, 'src', 'views', 'IntelligenceView.tsx')), false);
  assert.equal(fs.existsSync(path.resolve(root, '..', '..', 'crates', 'reposlice-intelligence')), false);
});

test('generic workspace exports remain available without the removed intelligence engine', () => {
  const cli = src('../../crates/reposlice-cli/src/main.rs');
  const sdk = src('../../crates/reposlice-sdk/src/lib.rs');
  const graph = src('../../crates/reposlice-graph/src/lib.rs');
  assert.match(cli, /export-workspace/);
  assert.match(cli, /json\|mermaid\|graphml/);
  assert.match(sdk, /pub fn export_workspace/);
  assert.match(graph, /pub fn export_workspace_mermaid/);
  assert.match(graph, /pub fn export_workspace_graphml/);
  assert.doesNotMatch(cli, /sarif|architecture-report|agent-context|init-rules|\bhistory\b|\bdiff\b/);
});

test('capsule verification exposes static readiness and sandbox plans without executing them', () => {
  const view = src('src/views/CapsulesView.tsx');
  const types = src('src/types.ts');
  const tauri = src('src-tauri/src/lib.rs');
  const verifier = src('../../crates/reposlice-verifier/src/lib.rs');
  assert.match(types, /readinessScore:\s*number/);
  assert.match(types, /sandboxValidationPlan:\s*SandboxValidationPlan\[\]/);
  assert.match(view, /report\.readinessScore/);
  assert.match(view, /report\.sandboxValidationPlan/);
  assert.match(tauri, /sandbox_validation_plan\(Path::new\(&path\)\)/);
  assert.match(verifier, /Plan only: execute explicitly inside an isolated container/);
});

test('local MCP and VS Code integration stay bounded after intelligence removal', () => {
  const mcp = src('../../crates/reposlice-mcp/src/main.rs');
  const extension = src('../../integrations/vscode/extension.js');
  const manifest = src('../../integrations/vscode/package.json');
  const workflow = src('../../.github/workflows/quality-real.yml');
  assert.match(mcp, /MCP_PROTOCOL_VERSION:\s*&str\s*=\s*"2026-07-28"/);
  assert.match(mcp, /is_loopback\(\)/);
  assert.match(mcp, /MAX_REQUEST_BYTES/);
  assert.match(mcp, /workspace_status/);
  assert.doesNotMatch(mcp, /agent_context|explain_target|workspace_history|architecture_report/);
  assert.match(extension, /execFile/);
  assert.doesNotMatch(extension, /architecture-report|agent-context|\["explain"/);
  assert.doesNotMatch(manifest, /architectureReport|explainTarget|agentContext|intelligence/i);
  assert.match(workflow, /benchmark-real-corpus\.ps1/);
});

test('workspace smoke test covers lifecycle integrity and generic exports', () => {
  const smoke = src('../../scripts/smoke-workspace.ps1');
  for (const command of ['scan-workspace', 'cached-workspace', 'validate-workspace', 'export-workspace', 'scan-repository', 'remove-repository']) {
    assert.ok(smoke.includes(command), `smoke test missing ${command}`);
  }
  for (const removed of ['architecture-report', 'init-rules', 'agent-context', 'history', 'architecture.sarif']) {
    assert.ok(!smoke.includes(removed), `removed intelligence command remains in smoke test: ${removed}`);
  }
});

test('focused interface leaves deeper engine capabilities intact without exposing legacy views', () => {
  const app = src('src/App.tsx');
  assert.doesNotMatch(app, /CapsulesView|SliceView|ImpactView/);
  assert.ok(fs.existsSync(path.resolve(root, '..', '..', 'crates', 'reposlice-capsule')));
  assert.ok(fs.existsSync(path.resolve(root, '..', '..', 'crates', 'reposlice-runtime')));
});

test('workspace integrity field contract remains defined by the graph engine', () => {
  const graph = src('../../crates/reposlice-graph/src/lib.rs');
  for (const field of [
    'missing_source_units', 'missing_target_units', 'missing_source_components',
    'missing_target_components', 'missing_target_entrypoints',
  ]) {
    assert.match(graph, new RegExp(`pub ${field}:`), `workspace integrity report is missing ${field}`);
  }
});

test('public desktop assets exclude redundant and regenerable repository artifacts', () => {
  const assets = JSON.parse(src('src/lib/assets.json'));
  assert.ok(fs.existsSync(path.join(root, 'public', 'assets', 'branding', 'reposlice.png')));
  for (const {path: asset} of assets.technologies) {
    assert.ok(fs.existsSync(path.join(root, 'public', 'assets', asset)));
  }
  const gitignore = fs.readFileSync(path.resolve(root, '..', '..', '.gitignore'), 'utf8');
  assert.match(gitignore, /src-tauri\/target/);
  assert.match(gitignore, /\.work\//);
});
