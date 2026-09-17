import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const src = (file) => fs.readFileSync(path.join(root, file), 'utf8');

test('technology icons resolve locally without remote CDN URLs', () => {
  const catalog = src('src/lib/technologyIcons.ts');
  const icons = src('src/components/Icons.tsx');
  assert.match(catalog, /LOCAL_TECH_ICON_BASE/);
  assert.doesNotMatch(catalog, /https:\/\/(?:thesvg\.org|cdn\.jsdelivr\.net)/);
  assert.doesNotMatch(icons, /https:\/\/(?:thesvg\.org|cdn\.jsdelivr\.net)/);
  const slugs = [...catalog.matchAll(/slug:\s*"([^"]+)"/g)].map((match) => match[1]);
  assert.ok(slugs.length >= 50, 'expected broad technology catalog');
  for (const slug of new Set(slugs)) {
    assert.ok(fs.existsSync(path.join(root, 'public', 'technologies', `${slug}.svg`)), `missing local technology icon ${slug}`);
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
  const sidebar = src('src/components/Sidebar.tsx');
  for (const view of ['overview', 'projects', 'architecture', 'audit', 'components', 'entrypoints', 'dependencies', 'capsules']) {
    assert.match(sidebar, new RegExp(`id:\\s*"${view}"`));
  }
});

test('styles are layered and readability overrides protect dense views', () => {
  const entry = src('src/styles/index.css');
  assert.match(entry, /@import "\.\/base\.css"/);
  assert.match(entry, /@import "\.\/projects\.css"/);
  assert.match(entry, /@import "\.\/architecture\.css"/);
  assert.match(entry, /@import "\.\/components\.css"/);
  assert.match(entry, /@import "\.\/entrypoints\.css"/);
  assert.match(entry, /@import "\.\/dependencies\.css"/);
  assert.match(entry, /@import "\.\/capsules\.css"/);
  assert.match(entry, /@import "\.\/readability\.css"/);
  assert.match(entry, /@import "\.\/i18n\.css"/);
  assert.match(entry, /@import "\.\/calibration\.css"/);
  const readability = src('src/styles/readability.css');
  for (const selector of ['.components-neon-file', '.entrypoints-file', '.audit-diagnostic', '.dependency-node strong']) {
    assert.ok(readability.includes(selector), `missing readability override for ${selector}`);
  }
  assert.match(readability, /font-size:\s*13px\s*!important/);
});

test('workspace refresh, repository lifecycle, progress and cancellation are wired end to end', () => {
  const client = src('src/lib/client.ts');
  const app = src('src/App.tsx');
  const projects = src('src/views/ProjectsView.tsx');
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

  assert.match(app, /listen<AnalysisProgress>\("analysis-progress"/);
  assert.match(app, /<AnalysisProgressBanner/);
  assert.match(projects, /onUpdate:\s*\(id:\s*string\)/);
  assert.match(projects, /onRemove:\s*\(id:\s*string\)/);
});

test('desktop interactions are race-safe and keyboard accessible', () => {
  const app = src('src/App.tsx');
  const modal = src('src/components/Modal.tsx');
  const toolbar = src('src/components/ProjectToolbar.tsx');
  const sidebar = src('src/components/Sidebar.tsx');

  assert.doesNotMatch(app, /window\.prompt/);
  assert.match(app, /workspaceLoadRequest/);
  assert.match(app, /Promise\.all/);
  assert.match(modal, /event\.key === "Escape"/);
  assert.match(modal, /FOCUSABLE_SELECTOR/);
  assert.match(modal, /aria-modal="true"/);
  assert.match(toolbar, /aria-busy=\{props\.busy\}/);
  assert.match(toolbar, /disabled=\{props\.busy\}/);
  assert.match(sidebar, /aria-current=\{active \? "page" : undefined\}/);
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

test('visual target semantics stay scoped to supported slice and capsule actions', () => {
  const app = src('src/App.tsx');
  const toolbarTargets = app.match(/const targets = useMemo\(\(\) => \{([\s\S]*?)async function refreshRepositories/)?.[1] ?? '';
  assert.ok(toolbarTargets, 'toolbar target model was not found');
  assert.doesNotMatch(toolbarTargets, /project-unit:/, 'ProjectToolbar must not offer project-unit targets to slice/capsule actions');
  assert.doesNotMatch(toolbarTargets, /model\.technologies/, 'ProjectToolbar must not offer technology targets to slice/capsule actions');
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
  const sidebar = src('src/components/Sidebar.tsx');
  assert.match(sidebar, /src="\/logo\.png"/);
  assert.ok(fs.existsSync(path.join(root, 'public', 'logo.png')), 'optimized sidebar logo must exist');
  for (const obsolete of ['Logo.png', 'Logo-4k.png', 'LogoMark-4k.png', 'favicon-256.png']) {
    assert.equal(fs.existsSync(path.join(root, 'public', obsolete)), false, `obsolete public asset should not be committed: ${obsolete}`);
  }
  assert.equal(fs.existsSync(path.join(root, 'src-tauri', 'gen', 'schemas')), false, 'Tauri schemas are regenerable and should not be committed');
  assert.equal(fs.existsSync(path.join(root, 'src-tauri', 'icons', 'android')), false, 'mobile Android icons are outside the Desktop product');
  assert.equal(fs.existsSync(path.join(root, 'src-tauri', 'icons', 'ios')), false, 'mobile iOS icons are outside the Desktop product');
});
