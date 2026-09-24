import type { FocusedEndpointLink, FocusedFrontendCall, FocusedRepositoryAnalysis, FocusedTechnology } from "../types";
import { normalizeTechnologyName, resolveTechnologyIcon } from "./technologyIcons";
export function uniqueTechnologies(analysis: FocusedRepositoryAnalysis): FocusedTechnology[] {
  const unique = new Map<string, FocusedTechnology>();
  for (const tech of analysis.project_units.flatMap(unit => unit.technologies)) {
    const id = resolveTechnologyIcon(tech.name)?.slug ?? normalizeTechnologyName(tech.name);
    const previous = unique.get(id);
    unique.set(id, previous ? {...previous, confidence: Math.max(previous.confidence, tech.confidence), evidence: [...(previous.evidence ?? []), ...(tech.evidence ?? [])]} : tech);
  }
  return [...unique.values()];
}
export function matchesCall(link: FocusedEndpointLink, unitId: string, call: FocusedFrontendCall) {
  const routePattern = (path: string) => path.replace(/\$\{([^}]+)\}/g, "{$1}").replace(/:([A-Za-z_$][\w$]*)/g, "{$1}");
  return link.consumer_project_unit_id === unitId && link.consumer_component_id === call.component_id && link.method.toUpperCase() === call.method.toUpperCase() && routePattern(link.path) === routePattern(call.path) && link.consumer_file === call.file && (link.consumer_line == null || link.consumer_line === call.line);
}
export type EndpointRow = {key: string; type: "backend" | "frontend"; unitId: string; unitName: string; method: string; path: string; file: string; symbol: string; line?: number | null; links: FocusedEndpointLink[]};
export function endpointRows(analysis: FocusedRepositoryAnalysis): EndpointRow[] {
  return analysis.project_units.flatMap(unit => [
    ...unit.backend_endpoints.map(endpoint => ({key: `${unit.id}:${endpoint.id}`, type: "backend" as const, unitId: unit.id, unitName: unit.name, method: endpoint.method, path: endpoint.path, file: endpoint.file, symbol: endpoint.creator, line: endpoint.line, links: analysis.endpoint_links.filter(link => link.provider_project_unit_id === unit.id && link.provider_entrypoint_id === endpoint.id)})),
    ...unit.frontend_calls.map((call, index) => ({key: `${unit.id}:call:${index}`, type: "frontend" as const, unitId: unit.id, unitName: unit.name, method: call.method, path: call.path, file: call.file, symbol: call.symbol ?? "", line: call.line, links: analysis.endpoint_links.filter(link => matchesCall(link, unit.id, call))}))
  ]);
}
export type Filters = {search: string; method: string; type: string; state: string; project: string};
export const emptyFilters: Filters = {search: "", method: "", type: "", state: "", project: ""};
export function filterRows(rows: EndpointRow[], filters: Filters) {
  return rows.filter(row => (!filters.method || row.method.toUpperCase() === filters.method) && (!filters.type || row.type === filters.type) && (!filters.project || row.unitId === filters.project) && (!filters.state || (filters.state === "matched" ? row.links.length > 0 : row.links.length === 0)) && `${row.path} ${row.symbol} ${row.file} ${row.unitName}`.toLowerCase().includes(filters.search.trim().toLowerCase()));
}
export const shortFile = (file: string) => file.replaceAll("\\", "/").split("/").slice(-3).join("/");
