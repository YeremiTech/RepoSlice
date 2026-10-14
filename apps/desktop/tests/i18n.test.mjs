import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const source = fs.readFileSync(path.join(root, 'src', 'i18n.tsx'), 'utf8');
const sourceTree = [
  'src/App.tsx',
  'src/components/Sidebar.tsx',
  'src/components/ProjectToolbar.tsx',
  'src/views/OverviewView.tsx',
  'src/views/ProjectsView.tsx',
  'src/views/ArchitectureView.tsx',
  'src/views/AuditView.tsx',
  'src/views/ComponentsView.tsx',
  'src/views/EntrypointsView.tsx',
  'src/views/DependenciesView.tsx',
  'src/views/CapsulesView.tsx'
].map((file) => fs.readFileSync(path.join(root, file), 'utf8')).join('\n');

function keysFromBlock(blockName) {
  const start = source.indexOf(`const ${blockName}: Record<string, string> = {`);
  assert.notEqual(start, -1, `${blockName} dictionary must exist`);
  const nextMarker = blockName === 'es' ? '\nconst en:' : '\nconst dictionaries:';
  const end = source.indexOf(nextMarker, start);
  assert.notEqual(end, -1, `${blockName} dictionary must terminate`);
  const block = source.slice(start, end);
  return new Set([...block.matchAll(/"([^"]+)"\s*:/g)].map((match) => match[1]));
}

test('all UI translation keys resolve in Spanish', () => {
  const es = keysFromBlock('es');
  const used = new Set([...sourceTree.matchAll(/\bt\("([^"]+)"/g)].map((match) => match[1]));
  const missing = [...used].filter((key) => !es.has(key));
  assert.deepEqual(missing, []);
});

test('English dictionary overrides every non-technical Spanish UI key in use', () => {
  const es = keysFromBlock('es');
  const en = keysFromBlock('en');
  const used = new Set([...sourceTree.matchAll(/\bt\("([^"]+)"/g)].map((match) => match[1]));
  const intentionallyNeutral = new Set([
    'common.workspace', 'common.projectUnit', 'common.projectUnits', 'common.entrypoints',
    'common.frameworks', 'common.middleware', 'common.namespace', 'common.commit'
  ]);
  const missing = [...used].filter((key) => es.has(key) && !en.has(key) && !intentionallyNeutral.has(key));
  assert.deepEqual(missing, []);
});

test('language preference is persisted and document language is updated', () => {
  assert.match(source, /localStorage\.setItem\(STORAGE_KEY, language\)/);
  assert.match(source, /document\.documentElement\.lang = language/);
  assert.match(source, /stored === "en" \? "en" : "es"/);
});

test('engine diagnostic codes have localized messages in both languages', () => {
  const es = keysFromBlock('es');
  const en = keysFromBlock('en');
  for (const code of [
    'text-index-memory-limit',
    'framework-evidence-insufficient',
    'generic-php-fallback',
    'component-index-limit',
    'source-too-large',
    'framework-no-static-entrypoints'
  ]) {
    assert.ok(es.has(`diagnostic.${code}`), `missing Spanish diagnostic ${code}`);
    assert.ok(en.has(`diagnostic.${code}`), `missing English diagnostic ${code}`);
  }
});
