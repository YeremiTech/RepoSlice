import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const assetRoot = path.join(root,'public','assets');
const entries = JSON.parse(fs.readFileSync(path.join(root,'src/lib/assets.json'),'utf8')).technologies;
const categories = new Set(['languages','frameworks','databases','orm','tooling','runtime']);
const normalize = name => name.trim().toLowerCase().replace(/[ ._-]/g,'');

function pngHeader(file) {
  const fd=fs.openSync(file,'r');
  const header=Buffer.alloc(24);
  try { assert.equal(fs.readSync(fd,header,0,24,0),24,`truncated PNG ${file}`); }
  finally { fs.closeSync(fd); }
  assert.equal(header.subarray(0,8).toString('hex'),'89504e470d0a1a0a',`invalid PNG ${file}`);
  assert.equal(header.toString('ascii',12,16),'IHDR',`missing PNG header ${file}`);
  assert.ok(header.readUInt32BE(16)>0 && header.readUInt32BE(20)>0,`invalid image dimensions ${file}`);
}

test('all bundled technology images are catalogued, real PNGs and assigned to a unique technology',()=>{
  const techRoot=path.join(assetRoot,'technologies');
  const disk=new Set([...categories].flatMap(cat=>fs.readdirSync(path.join(techRoot,cat)).filter(f=>f.endsWith('.png')).map(f=>`technologies/${cat}/${f}`)));
  const used=new Map();
  for(const entry of entries) {
    assert.ok(categories.has(entry.category),`invalid category ${entry.category}`);
    const target=path.join(assetRoot,entry.path);
    assert.ok(fs.existsSync(target),`missing image for ${entry.slug}: ${entry.path}`);
    pngHeader(target);
    assert.equal(entry.specific,!entry.path.startsWith('fallback/'),`misleading specific flag: ${entry.slug}`);
    if(entry.path.startsWith('technologies/')) {
      assert.ok(!used.has(entry.path),`image reused by ${used.get(entry.path)} and ${entry.slug}: ${entry.path}`);
      used.set(entry.path,entry.slug);
    }
  }
  assert.deepEqual([...disk].sort(),[...used.keys()].sort(),'unregistered PNGs are silently replaced by fallbacks');
});

test('catalog aliases cannot map the same name to two different technologies',()=>{
  const names=new Map();
  for(const entry of entries) for(const alias of [entry.slug,...entry.aliases]) {
    const key=normalize(alias);
    assert.ok(!names.has(key)||names.get(key)===entry.slug,`${alias} resolves ambiguously to ${entry.slug} and ${names.get(key)}`);
    names.set(key,entry.slug);
  }
  for(const [name,slug] of Object.entries({'C':'c','C++':'cpp','C#':'csharp','HTML':'html5','SCSS':'sass','Ionic':'ionic','DuckDB':'duckdb','Bootstrap':'bootstrap','SolidJS':'solid','Node.js':'nodejs','JavaFX':'javafx','.NET MAUI':'net-maui'})){
    assert.equal(names.get(normalize(name)),slug,`missing detection alias: ${name}`);
  }
});

test('the production TypeScript resolver selects specific new logos and distinguishes Java language from runtime',()=>{
  // The project and its CI use Node 22. Run the real TS module without cloning the resolver in a JS test.
  const moduleUrl=new URL('../src/lib/technologyIcons.ts',import.meta.url).href;
  const code=`import assert from 'node:assert/strict';import { resolveTechnologyIcon,technologyCategory,technologyIconUrl } from ${JSON.stringify(moduleUrl)};
    const expectPath=(name,category,expected)=>{
      assert.equal(resolveTechnologyIcon(name,category)?.path,expected,name);
      assert.ok(technologyIconUrl(name,category).endsWith('assets/'+expected),name);
    };
    for(const [name,category,p] of [
      ['HTML','languages','technologies/languages/html.png'],
      ['CSS','languages','technologies/languages/css.png'],
      ['SCSS','languages','technologies/languages/sass.png'],
      ['Bootstrap','frameworks','technologies/frameworks/bootstrap.png'],
      ['SolidJS','frameworks','technologies/frameworks/solidjs.png'],
      ['Tailwind CSS','frameworks','technologies/frameworks/tailwind.png'],
      ['Hono','frameworks','technologies/frameworks/hono.png'],
      ['Neo4j','databases','technologies/databases/neo4j.png'],
      ['Duck DB','databases','technologies/databases/duckdb.png'],
      ['esbuild','tooling','technologies/tooling/esbuild.png'],
      ['Bun','runtime','technologies/runtime/bun.png'],
      ['Java','languages','technologies/languages/java.png'],
      ['Java','runtime','technologies/runtime/java.png']
    ]) expectPath(name,category,p);
    assert.equal(resolveTechnologyIcon('Java').category,'languages');
    assert.equal(technologyCategory({name:'Java',category:'language'}),'languages');
    assert.equal(technologyCategory({name:'.NET MAUI',category:'runtime'}),'runtime');
    assert.ok(technologyIconUrl('experimental unknown tech','frameworks').endsWith('fallback/framework.png'));
    assert.notEqual(resolveTechnologyIcon('C')?.slug,resolveTechnologyIcon('C++')?.slug);
    assert.notEqual(resolveTechnologyIcon('C++')?.slug,resolveTechnologyIcon('C#')?.slug);`;
  execFileSync(process.execPath,['--experimental-strip-types','--input-type=module','-e',code],{cwd:root,stdio:'pipe'});
});

test('pending PNG inventory lists only registered technologies still using fallback',()=>{
  const pending=JSON.parse(fs.readFileSync(path.join(root,'../../docs/png-pending.json'),'utf8'));
  assert.equal(pending.length,entries.filter(e=>e.path.startsWith('fallback/')).length);
  const slugByName=new Map(entries.flatMap(e=>[e.slug,...e.aliases].map(name=>[normalize(name),e])));
  for(const item of pending) {
    const entry=slugByName.get(normalize(item.technology));
    assert.ok(entry&&entry.path.startsWith('fallback/'),`stale pending image ${item.technology}`);
  }
});
