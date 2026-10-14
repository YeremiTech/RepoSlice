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

function imageHeader(file) {
  const fd=fs.openSync(file,'r');
  const header=Buffer.alloc(24);
  try { assert.equal(fs.readSync(fd,header,0,24,0),24,`truncated image ${file}`); }
  finally { fs.closeSync(fd); }
  if (file.endsWith('.webp')) {
    assert.equal(header.toString('ascii',0,4),'RIFF',`invalid WebP ${file}`);
    assert.equal(header.toString('ascii',8,12),'WEBP',`missing WebP header ${file}`);
  } else {
    assert.equal(header.subarray(0,8).toString('hex'),'89504e470d0a1a0a',`invalid PNG ${file}`);
    assert.equal(header.toString('ascii',12,16),'IHDR',`missing PNG header ${file}`);
    assert.ok(header.readUInt32BE(16)>0 && header.readUInt32BE(20)>0,`invalid image dimensions ${file}`);
  }
}

const materialized = asset => asset.startsWith('technologies/') && asset.endsWith('.png') ? asset.replace(/\.png$/, '.webp') : asset;

test('all bundled technology images are catalogued, valid images and assigned to a unique technology',()=>{
  const techRoot=path.join(assetRoot,'technologies');
  const disk=new Set([...categories].flatMap(cat=>fs.readdirSync(path.join(techRoot,cat)).filter(f=>/\.(png|webp)$/.test(f)).map(f=>`technologies/${cat}/${f}`)));
  const used=new Map();
  for(const entry of entries) {
    assert.ok(categories.has(entry.category),`invalid category ${entry.category}`);
    const target=path.join(assetRoot,materialized(entry.path));
    assert.ok(fs.existsSync(target),`missing image for ${entry.slug}: ${entry.path}`);
    imageHeader(target);
    assert.equal(entry.specific,!entry.path.startsWith('fallback/'),`misleading specific flag: ${entry.slug}`);
    if(entry.path.startsWith('technologies/')) {
      assert.ok(!used.has(entry.path),`image reused by ${used.get(entry.path)} and ${entry.slug}: ${entry.path}`);
      used.set(entry.path,entry.slug);
    }
  }
  assert.deepEqual([...disk].sort(),[...used.keys()].map(materialized).sort(),'unregistered technology images are silently replaced by fallbacks');
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
    const expectPath=(name,category,catalogPath,materializedPath)=>{
      assert.equal(resolveTechnologyIcon(name,category)?.path,catalogPath,name);
      assert.ok(technologyIconUrl(name,category).endsWith('assets/'+materializedPath),name);
    };
    for(const [name,category,p,asset] of [
      ['HTML','languages','technologies/languages/html.png','technologies/languages/html.webp'],
      ['CSS','languages','technologies/languages/css.png','technologies/languages/css.webp'],
      ['SCSS','languages','technologies/languages/sass.png','technologies/languages/sass.webp'],
      ['Bootstrap','frameworks','technologies/frameworks/bootstrap.png','technologies/frameworks/bootstrap.webp'],
      ['SolidJS','frameworks','technologies/frameworks/solidjs.png','technologies/frameworks/solidjs.webp'],
      ['Tailwind CSS','frameworks','technologies/frameworks/tailwind.png','technologies/frameworks/tailwind.webp'],
      ['Hono','frameworks','technologies/frameworks/hono.png','technologies/frameworks/hono.webp'],
      ['Neo4j','databases','technologies/databases/neo4j.png','technologies/databases/neo4j.webp'],
      ['Duck DB','databases','technologies/databases/duckdb.png','technologies/databases/duckdb.webp'],
      ['esbuild','tooling','technologies/tooling/esbuild.png','technologies/tooling/esbuild.webp'],
      ['Bun','runtime','technologies/runtime/bun.png','technologies/runtime/bun.webp'],
      ['Java','languages','technologies/languages/java.png','technologies/languages/java.webp'],
      ['Java','runtime','technologies/runtime/java.png','technologies/runtime/java.webp']
    ]) { expectPath(name,category,p,asset); }
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
