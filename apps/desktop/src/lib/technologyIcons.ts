import catalog from "./assets.json" with { type: "json" };
import type { Technology } from "../types";

export type TechnologyIconCategory = "languages" | "frameworks" | "databases" | "orm" | "tooling" | "runtime";
type TechnologyIcon = (typeof catalog.technologies)[number];
export const assetUrl = (path: string) => `${import.meta.env?.BASE_URL ?? "/"}assets/${path}`;
// Keep + and # distinct: C, C++ and C# must never collapse into the same key.
export const normalizeTechnologyName = (name: string) => name.trim().toLowerCase().replace(/[ ._-]/g, "");
export const technologyIconCatalog = catalog.technologies;

// Some technologies have separate logos for their language and runtime roles
// (notably Java). Never let the final catalog entry silently overwrite another.
const byName = new Map<string, TechnologyIcon[]>();
for (const icon of technologyIconCatalog) {
  for (const key of new Set([icon.slug, ...icon.aliases].map(normalizeTechnologyName))) {
    const matches = byName.get(key) ?? [];
    if (!matches.some(item => item.slug === icon.slug && item.category === icon.category)) {
      matches.push(icon);
    }
    byName.set(key, matches);
  }
}
export function resolveTechnologyIcon(name: string, category?: string): TechnologyIcon | undefined {
  const matches = byName.get(normalizeTechnologyName(name));
  return (category ? matches?.find(item => item.category === category) : undefined) ?? matches?.[0];
}
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
  const icon = resolveTechnologyIcon(name, category);
  return icon ? assetUrl(icon.path) : fallbackUrl(category);
}
