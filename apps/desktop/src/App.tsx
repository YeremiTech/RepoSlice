import { useRef, useState } from "react";
import type { ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { analyzeSource } from "./lib/client";
import { assetUrl, categoryLabels, fallbackUrl, technologyCategory, technologyIconUrl } from "./lib/technologyIcons";
import { endpointRows, filterRows, emptyFilters, shortFile, uniqueTechnologies } from "./lib/presentation";
import type { EndpointRow, Filters } from "./lib/presentation";
import MetricCard from "./components/MetricCard";
import PngImage from "./components/PngImage";
import { ApiIcon, CodeIcon, DashboardIcon, DatabaseIcon, FolderIcon, GitHubIcon, GlobeIcon, LinkIcon, ScanIcon, ServerIcon, StackIcon } from "./components/FocusIcons";
import type { FocusedRepositoryAnalysis, FocusedTechnology } from "./types";

type View = "resumen" | "tecnologias" | "endpoints";
type Status = "pending" | "running" | "complete" | "error";
export default function App() {
  const [mode, setMode] = useState<"local" | "github">("local");
  const [localPath, setLocalPath] = useState("");
  const [githubUrl, setGithubUrl] = useState("");
  const [analysis, setAnalysis] = useState<FocusedRepositoryAnalysis | null>(null);
  const [view, setView] = useState<View>("resumen");
  const [status, setStatus] = useState<Status>("pending");
  const [error, setError] = useState("");
  const [duration, setDuration] = useState(0);
  const running = useRef(false);
  const busy = status === "running";
  const source = mode === "local" ? localPath : githubUrl;
  async function chooseFolder() {
    try {
      const selected = await open({directory: true, multiple: false, title: "Seleccionar proyecto"});
      if (typeof selected === "string") {setLocalPath(selected);setMode("local");}
    } catch (cause) {setError(`No se pudo abrir el selector de carpetas: ${String(cause)}`);}
  }
  async function runAnalysis() {
    if (running.current || !source.trim()) return;
    if (mode === "github" && !/^https?:\/\/github\.com\/[^/]+\/[^/]+/i.test(source.trim())) {setError("Introduce una URL de repositorio de GitHub válida.");return;}
    running.current = true;setStatus("running");setError("");setAnalysis(null);setView("resumen");
    const start = performance.now();
    try {setAnalysis(await analyzeSource(source.trim()));setDuration((performance.now()-start)/1000);setStatus("complete");}
    catch (cause) {setError(cause instanceof Error ? cause.message : String(cause));setStatus("error");}
    finally {running.current = false;}
  }
  return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><PngImage src={assetUrl("branding/reposlice.png")} alt="RepoSlice" variant="brand" size={204} className="brand-logo" style={{height: 104}}/></div>
      <nav className="sidebar-nav" aria-label="Navegación principal">
        {([["resumen", "Resumen", <DashboardIcon/>], ["tecnologias", "Tecnologías", <StackIcon/>], ["endpoints", "Endpoints", <ApiIcon/>]] as const).map(([id, label, icon]) => <button type="button" key={id} className={`nav-button ${view === id ? "active" : ""}`} aria-current={view === id ? "page" : undefined} onClick={() => setView(id)}>{icon}{label}</button>)}
      </nav>
      <footer className="sidebar-footer"><span>v0.4.0</span><span className="engine-status" data-status={status}>{busy ? "Analizando" : status === "error" ? "Error" : "Listo"}</span></footer>
    </aside>
    <main className="main-content">
      <form className="source-toolbar" onSubmit={event => {event.preventDefault();void runAnalysis();}}>
        <div className="source-mode" aria-label="Origen del proyecto">
          <button type="button" disabled={busy} aria-pressed={mode === "local"} className={mode === "local" ? "active" : ""} onClick={() => setMode("local")}><FolderIcon/>Local</button>
          <button type="button" disabled={busy} aria-pressed={mode === "github"} className={mode === "github" ? "active" : ""} onClick={() => setMode("github")}><GitHubIcon/>GitHub</button>
        </div>
        <div className="source-input-shell"><FolderIcon/><input aria-label={mode === "local" ? "Ruta del proyecto local" : "URL de GitHub"} disabled={busy} value={source} onChange={event => mode === "local" ? setLocalPath(event.target.value) : setGithubUrl(event.target.value)} placeholder={mode === "local" ? "Selecciona la carpeta de tu proyecto…" : "https://github.com/owner/repository"}/>{mode === "local" && <button type="button" className="browse-button" onClick={() => void chooseFolder()} disabled={busy}>Explorar</button>}</div>
        <button className="analyze-button" disabled={busy || !source.trim()}><ScanIcon/>{busy ? "Analizando…" : "Analizar"}</button>
      </form>
      <div className="view-container" aria-busy={busy}>
        {error && <div className="error-banner" role="alert"><strong>No se pudo completar la operación</strong><span>{error}</span></div>}
        {!analysis ? <><section className="welcome-state"><ScanIcon size={52}/><h1>Conoce lo que hay en tu código.</h1><p>Selecciona una carpeta o introduce un repositorio de GitHub para descubrir tecnologías, endpoints y sus relaciones.</p><span>{busy ? "El motor está procesando el proyecto…" : "Los resultados aparecerán aquí después del análisis."}</span></section><AnalysisStatus status={status} duration={duration}/></> : <>
          {view === "resumen" && <Overview analysis={analysis} status={status} duration={duration}/>}
          {view === "tecnologias" && <Technologies analysis={analysis}/>}
          {view === "endpoints" && <Endpoints key={analysis.root} analysis={analysis}/>}
        </>}
      </div>
    </main>
  </div>;
}
function SectionTitle({icon, title, count}: {icon: ReactNode; title: string; count?: ReactNode}) {return <header className="section-header">{icon}<h2>{title}</h2>{count !== undefined && <span className="count-badge">{count}</span>}</header>;}
function TechnologyImage({technology, size=52}: {technology: FocusedTechnology; size?: number}) {const category=technologyCategory(technology);return <PngImage src={technologyIconUrl(technology.name, category)} fallback={fallbackUrl(category)} alt="" variant="logo" size={size}/>;}
function TechTile({technology}: {technology: FocusedTechnology}) {return <article className="tech-tile" title={`${technology.classification} · ${technology.detection_kind} · Confianza: ${technology.confidence}%`}><TechnologyImage technology={technology}/><span>{technology.name}</span></article>;}
function Overview({analysis, status, duration}: {analysis: FocusedRepositoryAnalysis; status: Status; duration: number}) {
 const techs=uniqueTechnologies(analysis);const rows=endpointRows(analysis);
 return <div className="view-stack">
   <div className="metrics-grid">
     <MetricCard label="Proyectos detectados" value={analysis.project_units.length} tone="violet" icon={<StackIcon size={32}/>}/>
     <MetricCard label="Tecnologías" value={techs.length} tone="cyan" icon={<CodeIcon size={32}/>}/>
     <MetricCard label="Endpoints backend" value={rows.filter(r=>r.type==="backend").length} tone="green" icon={<ServerIcon size={32}/>}/>
     <MetricCard label="Llamadas frontend" value={rows.filter(r=>r.type==="frontend").length} tone="violet" icon={<ApiIcon size={32}/>}/>
     <MetricCard label="Relaciones encontradas" value={analysis.endpoint_links.length} tone="cyan" icon={<LinkIcon size={32}/>}/>
   </div>
   <div className="overview-columns">
     <section className="neo-panel"><SectionTitle title="Resumen del stack" icon={<StackIcon size={28}/>} count={`${techs.length} tecnologías`}/><div className="stack-grid">{techs.map(t=><TechTile key={t.name} technology={t}/>)}{!techs.length && <Empty>No se detectaron tecnologías.</Empty>}</div></section>
     <section className="neo-panel"><SectionTitle title="Unidades detectadas" icon={<ServerIcon size={28}/>} count={`${analysis.project_units.length} proyectos`}/><div className="unit-list">{analysis.project_units.map(unit=><article className="unit-row" key={unit.id}><FolderIcon size={30}/><div><strong>{unit.name}</strong><span title={unit.root}>{unit.root}</span><small>{unit.files} archivos · {unit.technologies.length} tecnologías</small></div><span className={`role-badge role-${unit.role.toLowerCase()}`}>{unit.role}</span></article>)}{!analysis.project_units.length && <Empty>No se detectaron unidades.</Empty>}</div></section>
   </div>
   <AnalysisStatus status={status} duration={duration}/>
 </div>;
}
function AnalysisStatus({status, duration}: {status: Status; duration: number}) {
 const stages=["Descubrir proyecto", "Detectar tecnologías", "Detectar endpoints backend", "Detectar consumo frontend", "Relacionar frontend-backend"];
 return <section className="neo-panel analysis-status" data-status={status} aria-label="Estado del análisis"><SectionTitle title="Estado del análisis" icon={<ScanIcon size={28}/>} count={status === "complete" ? `Completado en ${duration.toFixed(1)} s` : ({pending:"Pendiente",running:"En ejecución",error:"Error"} as const)[status]}/><ol className="analysis-stages">{stages.map((stage,i)=><li key={stage}><PngImage src={assetUrl(status==="complete" ? "status/complete.png" : "actions/analyze.png")} alt="" size={28}/><strong>{i+1}. {stage}</strong><span>{status==="complete" ? "Completado" : status==="running" ? "Procesando análisis" : status==="error" ? "Sin resultado confirmado" : "Pendiente"}</span></li>)}</ol>{status === "running" && <p className="status-note" role="status">El motor no comunica avances por etapa. Esperando el resultado completo.</p>}</section>;
}
function Technologies({analysis}: {analysis: FocusedRepositoryAnalysis}) {
 const [project,setProject]=useState("");
 const selected={...analysis,project_units:analysis.project_units.filter(u=>!project || u.id===project)};
 const techs=uniqueTechnologies(selected);
 const evidence=techs.flatMap(t=>(t.evidence??[]).filter(e=>e.source.trim()).map(e=>({...e,technology:t.name})));
 const confidence=techs.length?Math.round(techs.reduce((n,t)=>n+t.confidence,0)/techs.length):null;
 return <div className="view-stack">
   <section className="project-summary neo-panel"><FolderIcon size={36}/><div><strong>{analysis.name}</strong><span>{analysis.root}</span></div><div className="summary-number"><b>{techs.length}</b><span>Tecnologías</span></div><div className="summary-number"><b>{selected.project_units.reduce((n,u)=>n+u.files,0)}</b><span>Archivos analizados</span></div><label>Proyecto<select value={project} onChange={e=>setProject(e.target.value)}><option value="">Todos los proyectos</option>{analysis.project_units.map(u=><option key={u.id} value={u.id}>{u.name}</option>)}</select></label></section>
   <div className="category-grid">{Object.entries(categoryLabels).map(([category,label])=>{const items=techs.filter(t=>technologyCategory(t)===category);return <section className="neo-panel" key={category}><SectionTitle icon={category==="databases"||category==="orm"?<DatabaseIcon/>:<CodeIcon/>} title={label} count={`${items.length} detectadas`}/><div className="category-items">{items.map(t=><TechTile key={t.name} technology={t}/>)}{!items.length&&<Empty>Sin detecciones</Empty>}</div></section>;})}</div>
   <div className="evidence-columns"><section className="neo-panel"><SectionTitle icon={<FolderIcon/>} title="Evidencia encontrada" count={`${new Set(evidence.map(e=>e.source)).size} archivos`}/><div className="evidence-list">{evidence.map((e,i)=><div className="evidence-row" key={`${e.technology}-${i}`}><FolderIcon size={18}/><span title={e.source}>{shortFile(e.source)}<small>{e.detail}</small></span><span>{e.technology}</span><span>{e.confidence}%</span></div>)}{!evidence.length&&<Empty>El análisis no proporcionó archivos de evidencia.</Empty>}</div></section>
   <section className="neo-panel"><SectionTitle title="Cobertura del análisis" icon={<ScanIcon/>}/><div className="coverage"><div className="confidence-ring" style={{"--confidence":`${confidence??0}%`} as React.CSSProperties}><strong>{confidence===null?"—":`${confidence}%`}</strong><span>Confianza media</span></div><dl><dt>Archivos analizados</dt><dd>{selected.project_units.reduce((n,u)=>n+u.files,0)}</dd><dt>Tecnologías detectadas</dt><dd>{techs.length}</dd><dt>Con evidencia</dt><dd>{techs.filter(t=>t.evidence?.some(e=>e.source.trim())).length}</dd></dl></div><p className="status-note">Promedio de confianza de las tecnologías detectadas. No representa el porcentaje del código cubierto.</p></section></div>
 </div>;
}
function Endpoints({analysis}: {analysis: FocusedRepositoryAnalysis}) {
 const [filters,setFilters]=useState<Filters>(emptyFilters);
 const all=endpointRows(analysis);const rows=filterRows(all,filters);
 const relations=analysis.endpoint_links.filter(link=>rows.some(r=>r.links.includes(link)));
 const update=(key:keyof Filters,value:string)=>setFilters(previous=>({...previous,[key]:value}));
 return <div className="view-stack">
   <div className="endpoint-filters"><label>Buscar<input value={filters.search} placeholder="Rutas, funciones, archivos…" onChange={e=>update("search",e.target.value)}/></label><label>Método HTTP<select value={filters.method} onChange={e=>update("method",e.target.value)}><option value="">Todos</option>{[...new Set(all.map(r=>r.method.toUpperCase()))].sort().map(m=><option key={m}>{m}</option>)}</select></label><label>Tipo<select value={filters.type} onChange={e=>update("type",e.target.value)}><option value="">Todos</option><option value="backend">Backend</option><option value="frontend">Frontend</option></select></label><label>Estado de relación<select value={filters.state} onChange={e=>update("state",e.target.value)}><option value="">Todos</option><option value="matched">Relacionado</option><option value="unmatched">Sin relación</option></select></label><label>Proyecto<select value={filters.project} onChange={e=>update("project",e.target.value)}><option value="">Todos</option>{analysis.project_units.map(u=><option value={u.id} key={u.id}>{u.name}</option>)}</select></label></div>
   <div className="endpoint-metrics"><MetricCard label="Endpoints backend" value={all.filter(r=>r.type==="backend").length} tone="cyan" icon={<CodeIcon size={32}/>}/><MetricCard label="Llamadas frontend" value={all.filter(r=>r.type==="frontend").length} tone="violet" icon={<ApiIcon size={32}/>}/><MetricCard label="Relaciones encontradas" value={analysis.endpoint_links.length} tone="green" icon={<LinkIcon size={32}/>}/></div>
   <div className="endpoint-columns">{filters.type!=="frontend"&&<EndpointTable key={`backend:${JSON.stringify(filters)}`} title="Backend" rows={rows.filter(r=>r.type==="backend")}/>} {filters.type!=="backend"&&<EndpointTable key={`frontend:${JSON.stringify(filters)}`} title="Frontend" rows={rows.filter(r=>r.type==="frontend")}/>}</div>
   <section className="neo-panel"><SectionTitle title="Relaciones frontend ↔ backend" icon={<LinkIcon size={28}/>} count={`${relations.length} relaciones`}/><div className="relation-grid">{relations.map((link,i)=>{const consumer=analysis.project_units.find(u=>u.id===link.consumer_project_unit_id);const provider=analysis.project_units.find(u=>u.id===link.provider_project_unit_id);const endpoint=provider?.backend_endpoints.find(e=>e.id===link.provider_entrypoint_id);return <article className="relation-card" key={i}><div><span>Frontend · {consumer?.name}</span><strong>{link.consumer_symbol||shortFile(link.consumer_file)}</strong><small title={link.consumer_file}>{shortFile(link.consumer_file)}{link.consumer_line?`:${link.consumer_line}`:""}</small></div><div className="relation-confidence"><b>{link.confidence}%</b><LinkIcon/></div><div><span>Backend · {provider?.name}</span><strong>{endpoint?.creator||shortFile(link.provider_file)}</strong><code>{link.method} {endpoint?.path??link.path}</code><small title={link.provider_file}>{shortFile(link.provider_file)}{endpoint?.line?`:${endpoint.line}`:""}</small></div></article>;})}{!relations.length&&<Empty>No hay relaciones confirmadas para estos filtros.</Empty>}</div></section>
 </div>;
}
function EndpointTable({title,rows}: {title:string;rows:EndpointRow[]}) {
 const [page,setPage]=useState(0);const size=6;const pages=Math.max(1,Math.ceil(rows.length/size));
 return <section className="neo-panel"><SectionTitle title={title} icon={title==="Backend"?<ServerIcon/>:<GlobeIcon/>} count={rows.length}/><div className="table-scroll"><table><thead><tr><th>Método</th><th>Ruta</th><th>Proyecto</th><th>{title==="Backend"?"Creado por":"Consumido por"}</th><th>Línea</th><th>Estado</th></tr></thead><tbody>{rows.slice(page*size,(page+1)*size).map(row=><tr key={row.key}><td><span className={`method-badge method-${row.method.toLowerCase()}`}>{row.method.toUpperCase()}</span></td><td><code>{row.path}</code></td><td>{row.unitName}</td><td><strong>{row.symbol||shortFile(row.file)}</strong><small title={row.file}>{shortFile(row.file)}</small></td><td>{row.line??"—"}</td><td><span className={row.links.length?"matched":"unmatched"}>{row.links.length?"Relacionado":"Sin relación"}</span></td></tr>)}</tbody></table>{!rows.length&&<Empty>No hay resultados para estos filtros.</Empty>}</div><div className="pagination"><span>{rows.length?`${page*size+1}–${Math.min((page+1)*size,rows.length)}`:"0"} de {rows.length}</span><button onClick={()=>setPage(p=>p-1)} disabled={page===0} aria-label={`Página anterior ${title}`}>Anterior</button><span>{page+1} / {pages}</span><button onClick={()=>setPage(p=>p+1)} disabled={page+1>=pages} aria-label={`Página siguiente ${title}`}>Siguiente</button></div></section>;
}
function Empty({children}: {children:ReactNode}) {return <p className="empty-message">{children}</p>;}
