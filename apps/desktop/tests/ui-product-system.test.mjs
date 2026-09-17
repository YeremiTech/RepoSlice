import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const src = (file) => fs.readFileSync(path.join(root, file), 'utf8');

test('product shell keeps progress and notifications outside normal view flow', () => {
  const app = src('src/App.tsx');
  const css = src('src/styles/product-shell.css');
  assert.match(app, /analysis-progress-layer/);
  assert.match(app, /floating-alert/);
  assert.match(css, /\.analysis-progress-layer\s*\{[\s\S]*position:\s*fixed/);
  assert.match(css, /\.floating-alert[\s\S]*position:\s*fixed/);
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

test('desktop sidebar stays fixed and keeps all primary view ids', () => {
  const sidebar = src('src/components/Sidebar.tsx');
  const app = src('src/App.tsx');
  const css = src('src/styles/product-shell.css');
  assert.doesNotMatch(sidebar, /onToggle|collapsed\?/);
  assert.match(app, /<div className="app-shell"><Sidebar view=\{view\} engineState=\{engineState\} onChange=\{setView\}/);
  assert.match(css, /grid-template-columns:\s*var\(--shell-sidebar\) minmax\(0, 1fr\)/);
  for (const view of ['overview', 'projects', 'architecture', 'audit', 'components', 'entrypoints', 'dependencies', 'capsules']) {
    assert.match(sidebar, new RegExp(`id:\\s*"${view}"`));
  }
});

test('section header keeps module headers compact without decorative subtitles', () => {
  const header = src('src/components/SectionHeader.tsx');
  assert.doesNotMatch(header, /subtitle/);
  assert.doesNotMatch(header, /<p>/);
});

test('scope toolbar is compact and only dependencies exposes direct target selection', () => {
  const app = src('src/App.tsx');
  const toolbar = src('src/components/ProjectToolbar.tsx');
  const css = src('src/styles/product-calibration-v3.css');
  assert.match(app, /showTarget=\{view === "dependencies"\}/);
  assert.match(toolbar, /scope-toolbar--compact/);
  assert.match(toolbar, /scope-toolbar--target/);
  assert.match(css, /\.project-toolbar\.scope-toolbar--compact/);
  assert.match(css, /height:\s*34px/);
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
