import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const src = (file) => fs.readFileSync(path.join(root, file), 'utf8');

test('focused analysis exposes status and recoverable errors in the active view', () => {
  const app = src('src/App.tsx');
  const css = src('src/styles/index.css');
  assert.match(app, /analysis-status/);
  assert.match(app, /role="alert"/);
  assert.match(css, /analysis-status/);
});

test('long analysis screens are split into focused tab surfaces', () => {
  for (const file of [
    'src/views/OverviewView.tsx',
    'src/views/ArchitectureView.tsx',
    'src/views/AuditView.tsx'
  ]) {
    assert.match(src(file), /<ViewTabs/);
  }
});

test('dense explorers paginate instead of mounting thousands of rows at once', () => {
  for (const file of [
    'src/views/ComponentsView.tsx',
    'src/views/EntrypointsView.tsx',
    'src/views/DependenciesView.tsx'
  ]) {
    const text = src(file);
    assert.match(text, /<Pagination/);
    assert.match(text, /pageSize/);
  }
});

test('desktop navigation keeps the three primary views in a fixed sidebar', () => {
  const app = src('src/App.tsx');
  const css = src('src/styles/index.css');
  assert.match(app, /className="app-shell"/);
  assert.match(app, /sidebar-nav/);
  assert.match(css, /grid-template-columns:\s*var\(--sidebar\)/);
  for (const view of ['Resumen', 'Tecnologías', 'Endpoints']) assert.ok(app.includes(view));
});

test('section header keeps module headers compact without decorative subtitles', () => {
  const header = src('src/components/SectionHeader.tsx');
  assert.doesNotMatch(header, /subtitle/);
  assert.doesNotMatch(header, /<p>/);
});

test('source controls stay in the application shell across all views', () => {
  const app = src('src/App.tsx');
  assert.match(app, /source-toolbar/);
  assert.match(app, /Local/);
  assert.match(app, /GitHub/);
  assert.match(app, /Analizar/);
  assert.match(app, /view-container/);
});

test('components and entrypoints share one reusable explorer command bar', () => {
  const components = src('src/views/ComponentsView.tsx');
  const entrypoints = src('src/views/EntrypointsView.tsx');
  const commandBar = src('src/components/ExplorerCommandBar.tsx');
  assert.match(components, /<ExplorerCommandBar/);
  assert.match(entrypoints, /<ExplorerCommandBar/);
  assert.match(commandBar, /explorer-command-bar__search/);
  assert.match(commandBar, /scopeOptions/);
});

test('overview and audit use the remaining viewport without stretching summary blocks', () => {
  const css = src('src/styles/product-calibration-v3.css');
  assert.match(css, /\.overview-view\s*\{[\s\S]*min-height:\s*100%/);
  assert.match(css, /\.audit-view\s*\{[\s\S]*height:\s*100%/);
  assert.match(css, /\.audit-tab-content\s*\{[\s\S]*flex:\s*1 1 auto/);
  assert.match(css, /\.overview-summary-columns[\s\S]*height:\s*auto/);
  assert.match(css, /\.audit-summary-layout[\s\S]*height:\s*auto/);
});

test('audit reproducibility fields wrap instead of being visually truncated', () => {
  const css = src('src/styles/product-calibration-v3.css');
  assert.match(css, /\.audit-kv\s*>\s*strong[\s\S]*white-space:\s*normal/);
  assert.match(css, /overflow-wrap:\s*anywhere/);
  assert.match(css, /\.audit-detail\s*>\s*summary\s*small/);
});
