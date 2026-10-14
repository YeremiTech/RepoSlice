import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(scriptDir, '..');

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(root, relativePath), 'utf8'));
}

function fail(message) {
  console.error(`Version contract: FAIL - ${message}`);
  process.exitCode = 1;
}

const desktopPackage = readJson('apps/desktop/package.json');
const desktopLock = readJson('apps/desktop/package-lock.json');
const tauriConfig = readJson('apps/desktop/src-tauri/tauri.conf.json');
const vscodePackage = readJson('integrations/vscode/package.json');
const expected = desktopPackage.version;

const jsonChecks = [
  ['apps/desktop/package-lock.json', desktopLock.version],
  ['apps/desktop/package-lock.json packages[""]', desktopLock.packages?.['']?.version],
  ['apps/desktop/src-tauri/tauri.conf.json', tauriConfig.version],
  ['integrations/vscode/package.json', vscodePackage.version],
];

for (const [source, version] of jsonChecks) {
  if (version !== expected) {
    fail(`${source} has version ${String(version)}, expected ${expected}`);
  }
}

const cargoRoots = ['crates', 'parsers', 'adapters', 'apps/desktop/src-tauri'];
const cargoManifests = [];

function collectCargoManifests(directory) {
  const absolute = path.join(root, directory);
  if (!fs.existsSync(absolute)) return;
  const stat = fs.statSync(absolute);
  if (stat.isFile()) {
    if (path.basename(absolute) === 'Cargo.toml') cargoManifests.push(absolute);
    return;
  }
  for (const entry of fs.readdirSync(absolute, { withFileTypes: true })) {
    const child = path.join(absolute, entry.name);
    if (entry.isDirectory()) {
      collectCargoManifests(path.relative(root, child));
    } else if (entry.isFile() && entry.name === 'Cargo.toml') {
      cargoManifests.push(child);
    }
  }
}

for (const cargoRoot of cargoRoots) collectCargoManifests(cargoRoot);

for (const manifest of cargoManifests.sort()) {
  const content = fs.readFileSync(manifest, 'utf8');
  const packageSection = content.match(/\[package\]([\s\S]*?)(?=\n\[|$)/);
  if (!packageSection) continue;
  const versionMatch = packageSection[1].match(/^version\s*=\s*["']([^"']+)["']/m);
  if (!versionMatch) {
    fail(`${path.relative(root, manifest)} does not declare a package version`);
    continue;
  }
  if (versionMatch[1] !== expected) {
    fail(`${path.relative(root, manifest)} has version ${versionMatch[1]}, expected ${expected}`);
  }
}

const releaseTag = process.env.RELEASE_TAG?.trim()
  || (process.env.GITHUB_REF_TYPE === 'tag' ? process.env.GITHUB_REF_NAME?.trim() : '');

if (releaseTag) {
  const match = releaseTag.match(/^v(\d+\.\d+\.\d+)(?:-[0-9A-Za-z.-]+)?$/);
  if (!match) {
    fail(`release tag ${releaseTag} must use vMAJOR.MINOR.PATCH or a SemVer prerelease suffix`);
  } else if (match[1] !== expected) {
    fail(`release tag ${releaseTag} targets ${match[1]}, but project version is ${expected}`);
  }
}

if (!process.exitCode) {
  console.log(`Version contract: PASS (${expected}, ${cargoManifests.length} Cargo packages checked)`);
}
