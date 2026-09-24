import { test, expect } from '@playwright/test';
import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolveTechnologyIcon, normalizeTechnologyName, technologyCategory } from '../../src/lib/technologyIcons';
import { matchesCall, filterRows, emptyFilters } from '../../src/lib/presentation';
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
// Stable UI fixture; native Tauri smoke tests exercise the actual scanner and IPC.
const analysis = {
  root: 'fixture', name: 'frontend-backend-endpoints',
  project_units: [
    {id:'backend',root:'fixture/backend',name:'backend',role:'Backend',files:2,technologies:[
      {name:'Java',category:'language',classification:'Programming Language',confidence:100,detection_kind:'DIRECT',evidence:[{kind:'source',source:'backend/src/UsersController.java',detail:'Java file extension',confidence:100}]},
      {name:'Spring Boot',category:'framework',classification:'Backend Framework',confidence:90,detection_kind:'DIRECT',evidence:[{kind:'manifest',source:'backend/pom.xml',detail:'Build manifest references Spring Boot',confidence:100}]},
      {name:'Maven',category:'build',classification:'Build Tool',confidence:100,detection_kind:'DIRECT',evidence:[{kind:'manifest',source:'backend/pom.xml',detail:'Maven evidence',confidence:95}]}
    ],backend_endpoints:[{id:'users-show',method:'GET',path:'/api/users/{id}',file:'backend/src/UsersController.java',creator:'UsersController.show',controller:'UsersController',action:'show',line:4}],frontend_calls:[]},
    {id:'frontend',root:'fixture/frontend',name:'frontend',role:'Frontend',files:3,technologies:[
      {name:'TypeScript',category:'language',classification:'Programming Language',confidence:100,detection_kind:'DIRECT',evidence:[{kind:'source',source:'frontend/src/api.ts',detail:'TypeScript file extension',confidence:100}]},
      {name:'React',category:'library',classification:'UI Library',confidence:100,detection_kind:'DIRECT',evidence:[{kind:'manifest',source:'frontend/package.json',detail:'package dependency react',confidence:100}]}
    ],backend_endpoints:[],frontend_calls:[{method:'GET',path:'/api/users/${id}',component_id:'api',file:'frontend/src/api.ts',symbol:'loadUser',line:2}]}
  ],
  endpoint_links:[{consumer_project_unit_id:'frontend',consumer_component_id:'api',consumer_file:'frontend/src/api.ts',consumer_symbol:'loadUser',consumer_line:2,method:'GET',path:'/api/users/{id}',provider_project_unit_id:'backend',provider_entrypoint_id:'users-show',provider_file:'backend/src/UsersController.java',confidence:100}]
};
async function openAnalysis(page, result = analysis) {
  await page.addInitScript(result => { window.__TAURI_INTERNALS__ = {invoke: async (command) => {if(command === 'plugin:dialog|open') return 'test-project';return result;}}; }, result);
  await page.goto('/');
  await page.getByRole('textbox', {name:'Ruta del proyecto local'}).fill('test-project');
  await page.getByRole('button', {name:'Analizar',exact:true}).click();
  await expect(page.getByRole('heading',{name:'Resumen del stack'})).toBeVisible();
}
test('explicit aliases keep frameworks and languages distinct', () => {
  for(const alias of ['Node.js','NodeJS','node']) expect(resolveTechnologyIcon(alias)?.slug).toBe('nodejs');
  expect(resolveTechnologyIcon('Spring Boot')?.slug).toBe('spring-boot');
  expect(resolveTechnologyIcon('postgres')?.slug).toBe('postgresql');
  expect(resolveTechnologyIcon('reactjs')?.slug).toBe('react');
  expect(new Set(['C','C++','C#'].map(normalizeTechnologyName)).size).toBe(3);
  expect(resolveTechnologyIcon('Micronaut')?.slug).toBe('micronaut');
  expect(resolveTechnologyIcon('Ktor')?.slug).toBe('ktor');
  expect(resolveTechnologyIcon('Micronaut')?.slug).toBe('micronaut');
  expect(technologyCategory({name:'Java',category:'language'})).toBe('languages');
  expect(technologyCategory({name:'Micronaut',category:'framework'})).toBe('frameworks');
  expect(resolveTechnologyIcon('Go Web')).toBeUndefined();
  expect(resolveTechnologyIcon('unknown-tool')).toBeUndefined();
});
test('newly bundled technology logos render for the actual analyzer category names',async({page})=>{
  const variants=[
    ['HTML','language','languages/html.png'],
    ['CSS','language','languages/css.png'],
    ['SCSS','language','languages/sass.png'],
    ['Bootstrap','framework','frameworks/bootstrap.png'],
    ['Hono','framework','frameworks/hono.png'],
    ['DuckDB','data','databases/duckdb.png'],
    ['Bun','runtime','runtime/bun.png'],
    ['esbuild','build','tooling/esbuild.png']
  ];
  const data=structuredClone(analysis);
  for(const [name,category] of variants){
    data.project_units[0].technologies.push({name,category,classification:'Technology',confidence:90,detection_kind:'DIRECT',evidence:[]});
  }
  await openAnalysis(page,data);
  await page.getByRole('button',{name:'Tecnologías',exact:true}).click();
  for(const [name,,asset] of variants){
    const tile=page.locator('.tech-tile').filter({hasText:name});
    await expect(tile).toHaveCount(1);
    await expect(tile.locator('img')).toHaveAttribute('src',`/assets/technologies/${asset}`);
    await expect.poll(()=>tile.locator('img').evaluate(img=>img.complete&&img.naturalWidth>0)).toBe(true);
  }
});

test('repeated calls retain independent file and line matching', () => {
  const link = analysis.endpoint_links[0];
  const unit = analysis.project_units.find(unit => unit.id === link.consumer_project_unit_id);
  const call = unit.frontend_calls[0];
  expect(matchesCall(link, unit.id, call)).toBe(true);
  expect(matchesCall(link, unit.id, {...call, file: 'another-file.ts'})).toBe(false);
  expect(matchesCall({...link, consumer_line: 20}, unit.id, {...call, line: 21})).toBe(false);
});
test('production views render evidence, endpoints and relationships with PNGs', async ({page}) => {
  const errors=[];page.on('pageerror',e=>errors.push(e.message));page.on('response',r=>{if(r.status()>=400) errors.push(`${r.status()} ${r.url()}`)});
  await openAnalysis(page);
  for(const [label,heading] of [['Resumen','Resumen del stack'],['Tecnologías','Evidencia encontrada'],['Endpoints','Relaciones frontend ↔ backend']]) {
    await page.getByRole('button',{name:label,exact:true}).click();
    await expect(page.getByRole('heading',{name:heading})).toBeVisible();
    await expect(page.locator('svg')).toHaveCount(0);
    await expect.poll(()=>page.locator('img').evaluateAll(images=>images.every(img=>img.complete&&img.naturalWidth>0&&img.src.endsWith('.png')))).toBe(true);
  }
  await expect(page.getByText('UsersController.show').first()).toBeVisible();
  await page.getByLabel('Estado de relación').selectOption('unmatched');
  await expect(page.getByText('No hay relaciones confirmadas para estos filtros.')).toBeVisible();
  await page.getByLabel('Estado de relación').selectOption('');
  await page.getByLabel('Buscar', {exact:true}).fill('no-such-route');
  await expect(page.locator('tbody tr')).toHaveCount(0);
  expect(errors).toEqual([]);
});
test('fallback, pagination, narrow window and source controls',async({page})=>{
  const data=structuredClone(analysis);
  data.project_units[0].technologies.push({name:'Unknown experimental framework',category:'framework',classification:'Framework',confidence:40,detection_kind:'INFERRED'});
  const endpoint=data.project_units[0].backend_endpoints[0];
  for(let i=0;i<15;i++)data.project_units[0].backend_endpoints.push({...endpoint,id:`extra-${i}`,path:`/test/${i}`});
  await page.route('**/technologies/frameworks/spring-boot.png',route=>route.fulfill({status:200,contentType:'image/png',body:'broken'}));
  await openAnalysis(page,data);
  await expect(page.getByText('Unknown experimental framework',{exact:true})).toBeVisible();
  await expect.poll(()=>page.locator('img').evaluateAll(images=>images.every(img=>img.complete&&img.naturalWidth>0))).toBe(true);
  await page.getByRole('button',{name:'Endpoints',exact:true}).click();
  await page.getByRole('button',{name:'Página siguiente Backend'}).click();
  await expect(page.getByText('7–12 de 16',{exact:true})).toBeVisible();
  await page.getByLabel('Buscar',{exact:true}).fill('/test/14');
  await expect(page.getByText('1–1 de 1',{exact:true})).toBeVisible();
  await page.setViewportSize({width:900,height:720});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  await page.getByRole('button',{name:'GitHub',exact:true}).click();
  await expect(page.getByRole('textbox',{name:'URL de GitHub'})).toBeVisible();
});
test('analysis errors are visible and allow retry',async({page})=>{
 await page.addInitScript(()=>{window.__TAURI_INTERNALS__={invoke:async()=>{throw new Error('Test analyzer failure')}}});
 await page.goto('/');await page.getByRole('textbox',{name:'Ruta del proyecto local'}).fill('bad');await page.getByRole('button',{name:'Analizar',exact:true}).click();
 await expect(page.getByRole('alert')).toContainText('Test analyzer failure');await expect(page.getByRole('button',{name:'Analizar',exact:true})).toBeEnabled();
});
test('every registered resource is included in production, with no UI vectors',()=>{
 const catalog=JSON.parse(fs.readFileSync(path.join(repo,'apps/desktop/src/lib/assets.json'),'utf8'));
 for(const entry of catalog.technologies)expect(fs.existsSync(path.join(repo,'apps/desktop/dist/assets',entry.path))).toBe(true);
 for(const p of Object.values(catalog.icons))expect(fs.existsSync(path.join(repo,'apps/desktop/dist/assets',`${p}.png`))).toBe(true);
 expect(fs.existsSync(path.join(repo,'apps/desktop/dist/technologies'))).toBe(false);
});
