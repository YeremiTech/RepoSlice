import catalog from "./assets.json" with { type: "json" };
import type { Technology } from "../types";

export type TechnologyIconCategory = "languages" | "frameworks" | "databases" | "orm" | "tooling" | "runtime";
export const assetUrl = (path: string) => `${import.meta.env.BASE_URL}assets/${path}`;
// Keep + and # distinct: C, C++ and C# must never collapse into the same key.
export const normalizeTechnologyName = (name: string) => name.trim().toLowerCase().replace(/[ ._-]/g, "");
export const technologyIconCatalog = catalog.technologies;
const byName = new Map(technologyIconCatalog.flatMap(icon => [icon.slug, ...icon.aliases].map(name => [normalizeTechnologyName(name), icon] as const)));
export const resolveTechnologyIcon = (name: string) => byName.get(normalizeTechnologyName(name));
export function technologyCategory(technology: Pick<Technology, "name" | "category">): TechnologyIconCategory {
  const known = resolveTechnologyIcon(technology.name);
  const category = technology.category.toLowerCase();
  if (category === "language") return "languages";
  if (known?.category === "orm" || known?.category === "runtime") return known.category;
  return ({language: "languages", framework: "frameworks", library: "frameworks", data: "databases", database: "databases", orm: "orm", build: "tooling", infrastructure: "tooling", runtime: "runtime"} as Record<string, TechnologyIconCategory>)[category] ?? known?.category as TechnologyIconCategory ?? "tooling";
}
export const categoryLabels: Record<TechnologyIconCategory, string> = {languages: "Lenguajes", frameworks: "Frameworks y librerías", databases: "Bases de datos", orm: "ORM / Data", tooling: "Build & Tooling", runtime: "Desktop / Runtime"};
export const fallbackUrl = (category: string = "generic") => assetUrl(`fallback/${({languages: "language", frameworks: "framework", databases: "database", orm: "database", tooling: "language", runtime: "runtime"} as Record<string,string>)[category] ?? "language"}.png`);
export function technologyIconUrl(name: string, category = "generic"): string {
  const icon = resolveTechnologyIcon(name);
  return icon ? assetUrl(icon.path) : fallbackUrl(category);
}
