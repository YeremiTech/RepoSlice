import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const scriptPath = fileURLToPath(import.meta.url);
const root = path.resolve(path.dirname(scriptPath), '..');
const violations = [];

const forbiddenDirectoryNames = new Set([
  'target', 'node_modules', 'dist', 'coverage', '.reposlice',
  '.idea', '.vscode', '.fleet', '.history',
  '.cursor', '.claude', '.codex', '.aider', '.windsurf', '.continue', '.roo', '.cody',
]);

const forbiddenRelativePrefixes = [
  'apps/desktop/src-tauri/gen/schemas/',
  'apps/desktop/src-tauri/icons/android/',
  'apps/desktop/src-tauri/icons/ios/',
];

const forbiddenFileNames = new Set([
  '.DS_Store', 'Thumbs.db', 'Desktop.ini', 'credentials.json',
  'AGENTS.md', 'CLAUDE.md', 'GEMINI.md', 'copilot-instructions.md',
]);

const forbiddenExtensions = new Set([
  '.tmp', '.temp', '.bak', '.orig', '.rej', '.swp', '.swo',
  '.pem', '.key', '.p12', '.pfx',
]);

const privateWorkingNamePatterns = [
  /pasted[ _-]*text/i,
  /scratch/i,
  /draft[ _-]*copy/i,
  /conversation[ _-]*export/i,
];

const suspiciousBannerPatterns = [
  /generated\s+by\s+(?:chatgpt|openai|claude|gemini|copilot)/i,
  /(?:ai|llm)[ -]?generated/i,
  /conversation\s+export/i,
  /assistant\s+response\s*:/i,
  /system\s+prompt\s*:/i,
];

const textExtensions = new Set([
  '.rs', '.ts', '.tsx', '.js', '.mjs', '.cjs', '.json', '.toml', '.yaml', '.yml',
  '.md', '.txt', '.css', '.scss', '.html', '.xml', '.properties', '.sh', '.ps1',
  '.bat', '.cmd', '.gitignore', '.gitattributes', '.editorconfig',
]);

function relative(file) {
  return path.relative(root, file).split(path.sep).join('/');
}

function isTextFile(file) {
  const base = path.basename(file);
  if (base.startsWith('.') && textExtensions.has(base)) return true;
  return textExtensions.has(path.extname(file).toLowerCase());
}

function visit(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === '.git') continue;
    const full = path.join(directory, entry.name);
    const rel = relative(full);

    if (entry.isDirectory()) {
      if (forbiddenDirectoryNames.has(entry.name)) {
        violations.push(`generated/local/tool directory committed: ${rel}/`);
      }
      visit(full);
      continue;
    }

    if (!entry.isFile()) continue;

    const segments = rel.split('/');
    for (const directoryName of forbiddenDirectoryNames) {
      if (segments.includes(directoryName)) {
        violations.push(`generated/local/tool directory committed: ${rel}`);
        break;
      }
    }

    if (forbiddenRelativePrefixes.some((prefix) => rel.toLowerCase().startsWith(prefix.toLowerCase()))) {
      violations.push(`regenerable Tauri artifact committed: ${rel}`);
    }

    if (forbiddenFileNames.has(entry.name)) {
      violations.push(`local/tool-specific file committed: ${rel}`);
    }

    const extension = path.extname(entry.name).toLowerCase();
    if (forbiddenExtensions.has(extension)) {
      violations.push(`temporary/sensitive extension committed: ${rel}`);
    }

    if (entry.name === '.env' || (entry.name.startsWith('.env.') && entry.name !== '.env.example')) {
      violations.push(`environment file committed: ${rel}`);
    }

    if (privateWorkingNamePatterns.some((pattern) => pattern.test(entry.name))) {
      violations.push(`private working artifact committed: ${rel}`);
    }

    const stat = fs.statSync(full);
    if (stat.size > 10 * 1024 * 1024) {
      violations.push(`unexpected file larger than 10 MiB: ${rel}`);
    }

    if (full !== scriptPath && isTextFile(full) && stat.size <= 2 * 1024 * 1024) {
      const sample = fs.readFileSync(full, 'utf8').slice(0, 256 * 1024);
      if (suspiciousBannerPatterns.some((pattern) => pattern.test(sample))) {
        violations.push(`generator/conversation residue in text file: ${rel}`);
      }
    }
  }
}

visit(root);

const unique = [...new Set(violations)].sort();
if (unique.length > 0) {
  console.error(`Repository hygiene check failed:\n - ${unique.join('\n - ')}`);
  process.exit(1);
}

const fileCount = (() => {
  let count = 0;
  function walk(dir) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === '.git') continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.isFile()) count += 1;
    }
  }
  walk(root);
  return count;
})();

console.log(`Repository hygiene: PASS (${fileCount} files checked)`);
