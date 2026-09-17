export type TechnologyIconCategory =
  | "runtime"
  | "language"
  | "frontend"
  | "backend"
  | "database"
  | "package-manager"
  | "build-tool"
  | "devops";

export type TechnologyIconDefinition = {
  name: string;
  slug: string;
  category: TechnologyIconCategory;
  aliases?: readonly string[];
};

const LOCAL_TECH_ICON_BASE = "/technologies";

/*
 * Catálogo local de iconos vectoriales usado por Overview y Runtime.
 * Los alias también cubren los nombres exactos que genera RepoSlice.
 *
 * Los nombres canónicos del motor se mantienen como alias explícitos para que
 * cada resultado pueda resolver su icono aunque el catálogo use una variante.
 */
export const technologyIconCatalog: readonly TechnologyIconDefinition[] = [
  // Runtime local
  { name: "Node.js", slug: "nodejs", category: "runtime", aliases: ["Node"] },
  { name: "Deno", slug: "deno", category: "runtime" },
  { name: "Bun", slug: "bun", category: "runtime" },
  { name: "Python", slug: "python", category: "runtime" },
  { name: "Java", slug: "java", category: "runtime", aliases: ["JVM", "Groovy", "Micronaut"] },
  { name: ".NET", slug: "dotnet", category: "runtime", aliases: ["ASP.NET", "Blazor", "F#", "Razor"] },
  { name: "PHP", slug: "php", category: "runtime" },
  { name: "Ruby", slug: "ruby", category: "runtime", aliases: ["Rails", "Ruby on Rails"] },
  { name: "Dart", slug: "dart", category: "runtime" },
  { name: "Elixir", slug: "elixir", category: "runtime" },
  { name: "Erlang", slug: "erlang", category: "runtime" },
  { name: "Lua", slug: "lua", category: "runtime" },
  { name: "Perl", slug: "perl", category: "runtime" },
  { name: "Julia", slug: "julia", category: "runtime" },
  { name: "Go", slug: "go", category: "runtime", aliases: ["Go Modules", "Go Web"] },
  { name: "Rust", slug: "rust", category: "runtime", aliases: ["Cargo", "Rust Web"] },
  { name: "Mix", slug: "mix", category: "runtime" },

  // Lenguajes
  { name: "HTML5", slug: "html5", category: "language", aliases: ["HTML"] },
  { name: "CSS", slug: "css", category: "language" },
  { name: "JavaScript", slug: "javascript", category: "language" },
  { name: "TypeScript", slug: "typescript", category: "language" },
  { name: "Kotlin", slug: "kotlin", category: "language", aliases: ["Ktor"] },
  { name: "Scala", slug: "scala", category: "language" },
  { name: "C", slug: "c", category: "language" },
  { name: "C++", slug: "cplusplus", category: "language" },
  { name: "C#", slug: "csharp", category: "language" },
  { name: "Swift", slug: "swift", category: "language" },
  { name: "Haskell", slug: "haskell", category: "language" },
  { name: "Clojure", slug: "clojure", category: "language" },
  { name: "Zig", slug: "zig", category: "language" },
  { name: "Solidity", slug: "solidity", category: "language" },
  { name: "Shell", slug: "linux", category: "language" },
  { name: "SQL", slug: "microsoft", category: "language" },

  // Frontend / frameworks
  { name: "Angular", slug: "angular", category: "frontend" },
  { name: "React", slug: "react", category: "frontend" },
  { name: "Vue.js", slug: "vuedotjs", category: "frontend", aliases: ["Vue"] },
  { name: "Svelte", slug: "svelte", category: "frontend", aliases: ["SvelteKit"] },
  { name: "Next.js", slug: "nextdotjs", category: "frontend" },
  { name: "Nuxt", slug: "nuxt", category: "frontend" },
  { name: "Astro", slug: "astro", category: "frontend" },
  { name: "Remix", slug: "remix", category: "frontend" },
  { name: "SolidJS", slug: "solid", category: "frontend" },
  { name: "Qwik", slug: "qwik", category: "frontend" },
  { name: "Alpine.js", slug: "alpinedotjs", category: "frontend" },
  { name: "Tailwind CSS", slug: "tailwindcss", category: "frontend" },
  { name: "Bootstrap", slug: "bootstrap", category: "frontend" },
  { name: "Material UI", slug: "mui", category: "frontend" },
  { name: "Sass", slug: "sass", category: "frontend", aliases: ["SCSS"] },
  { name: "Less", slug: "less", category: "frontend" },

  // Backend
  { name: "Spring", slug: "spring", category: "backend", aliases: ["Spring Boot"] },
  { name: "Quarkus", slug: "quarkus", category: "backend" },
  { name: "Django", slug: "django", category: "backend" },
  { name: "Flask", slug: "flask", category: "backend" },
  { name: "FastAPI", slug: "fastapi", category: "backend" },
  { name: "Laravel", slug: "laravel", category: "backend" },
  { name: "Symfony", slug: "symfony", category: "backend" },
  { name: "Express", slug: "express", category: "backend" },
  { name: "NestJS", slug: "nestjs", category: "backend" },
  { name: "Fastify", slug: "fastify", category: "backend" },
  { name: "Hono", slug: "hono", category: "backend" },
  { name: "Phoenix", slug: "phoenix-framework", category: "backend" },
  { name: "Entity Framework Core", slug: "entityframeworkcore", category: "backend" },

  // Bases de datos
  { name: "PostgreSQL", slug: "postgresql", category: "database" },
  { name: "MySQL", slug: "mysql", category: "database" },
  { name: "MariaDB", slug: "mariadb", category: "database" },
  { name: "SQLite", slug: "sqlite", category: "database" },
  { name: "MongoDB", slug: "mongodb", category: "database" },
  { name: "Redis", slug: "redis", category: "database" },
  { name: "Oracle", slug: "oracle", category: "database" },
  { name: "Microsoft SQL Server", slug: "microsoft", category: "database" },
  { name: "Apache Cassandra", slug: "cassandra", category: "database" },
  { name: "Apache CouchDB", slug: "couchdb", category: "database" },
  { name: "Neo4j", slug: "neo4j", category: "database" },
  { name: "Elasticsearch", slug: "elasticsearch", category: "database" },
  { name: "Supabase", slug: "supabase", category: "database" },
  { name: "Firebase", slug: "firebase", category: "database" },
  { name: "H2 Database", slug: "h2-database", category: "database" },

  // Package managers
  { name: "npm", slug: "npm", category: "package-manager", aliases: ["npm-compatible"] },
  { name: "pnpm", slug: "pnpm", category: "package-manager" },
  { name: "Yarn", slug: "yarn", category: "package-manager" },
  { name: "Composer", slug: "composer", category: "package-manager" },
  { name: "PyPI", slug: "pypi", category: "package-manager" },
  { name: "Poetry", slug: "poetry", category: "package-manager" },
  { name: "Gradle", slug: "gradle", category: "package-manager" },
  { name: "Apache Maven", slug: "apache-maven", category: "build-tool", aliases: ["Maven"] },
  { name: "Cargo", slug: "cargo", category: "package-manager" },
  {
    name: "SQL Database",
    slug: "azure-sql-database",
    category: "language",
    aliases: ["SQL"],
  },
  { name: "XML", slug: "xml", category: "language" },
  { name: "Hibernate", slug: "hibernate", category: "backend" },

  // Build tools / bundlers
  { name: "Vite", slug: "vite", category: "build-tool" },
  { name: "Webpack", slug: "webpack", category: "build-tool" },
  { name: "Rollup", slug: "rollupdotjs", category: "build-tool" },
  { name: "Babel", slug: "babel", category: "build-tool" },
  { name: "esbuild", slug: "esbuild", category: "build-tool" },
  { name: "SWC", slug: "swc", category: "build-tool" },
  { name: "Turborepo", slug: "turborepo", category: "build-tool" },
  { name: "Nx", slug: "nx", category: "build-tool" },
  { name: "Grunt", slug: "grunt", category: "build-tool" },
  { name: "Gulp", slug: "gulp", category: "build-tool" },

  // DevOps / infraestructura
  { name: "Docker", slug: "docker", category: "devops" },
  { name: "Kubernetes", slug: "kubernetes", category: "devops" },
  { name: "Git", slug: "git", category: "devops" },
  { name: "GitHub", slug: "github", category: "devops" },
  { name: "GitLab", slug: "gitlab", category: "devops" },
  { name: "Jenkins", slug: "jenkins", category: "devops" },
  { name: "Terraform", slug: "terraform", category: "devops" },
  { name: "Ansible", slug: "ansible", category: "devops" },
  { name: "Nginx", slug: "nginx", category: "devops" },
  { name: "Apache", slug: "apache", category: "devops" },
  { name: "Linux", slug: "linux", category: "devops" },
  { name: "Ubuntu", slug: "ubuntu", category: "devops" }
];

function normalizeTechnologyName(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]/g, "");
}

const technologyIconsByName = new Map<string, TechnologyIconDefinition>();

for (const icon of technologyIconCatalog) {
  for (const name of [icon.name, ...(icon.aliases ?? [])]) {
    technologyIconsByName.set(normalizeTechnologyName(name), icon);
  }
}

export function resolveTechnologyIcon(name: string): TechnologyIconDefinition | undefined {
  return technologyIconsByName.get(normalizeTechnologyName(name));
}

export function technologyIconUrl(name: string): string | undefined {
  const icon = resolveTechnologyIcon(name);
  return icon ? `${LOCAL_TECH_ICON_BASE}/${icon.slug}.svg` : undefined;
}
