# Supported technologies

| Technology | Discovery | Components | Entrypoints | Dependencies | Level |
| --- | --- | --- | --- | --- | --- |
| Java (generic) | `.java` | class, interface, record, enum | — | imports, references | L2 |
| Spring Boot | Maven evidence + source annotations | controller, service, repository, entity | HTTP mappings | generic Java graph | L3 |
| TypeScript / JavaScript web clients | `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`, `.vue`, `.svelte`, `.astro` | files, classes, functions/modules | conservative `fetch`, Axios and Angular HttpClient calls | imports and resolved TS/JS aliases where available | L1–L2 |
| PHP (generic) | `.php` | class, interface, trait, enum, function | — | imports, extends, implements, traits, constructor injection | L2 |
| Composer PHP library | `composer.json` without Laravel evidence | generic PHP symbols | — | generic PHP graph | L2 |
| Laravel | weighted Composer + structure evidence | controllers, middleware, commands, jobs, events, listeners, models, providers, migrations | HTTP routes, groups and resources | injection, route handlers, Eloquent relations | L3 |
| Angular | npm dependency + configuration/source evidence | components, services, modules, directives, pipes, guards | Angular Router paths | TypeScript imports | L3 |
| React | npm dependency + React source evidence | JSX/TSX components | React Router paths | JS/TypeScript imports | L3 |
| Next.js | npm dependency + configuration/source evidence | pages, views and JS/TypeScript symbols | App Router, Pages Router and API route files | JS/TypeScript imports | L3 |
| Vue | npm dependency + SFC/router evidence | Vue single-file components | Vue Router paths | SFC/JS/TypeScript imports | L3 |
| Nuxt | npm dependency + configuration/source evidence | Vue pages and components | file-system pages and Nitro server routes | SFC/JS/TypeScript imports | L3 |
| Svelte | npm dependency + configuration/source evidence | Svelte components | — | Svelte/JS/TypeScript imports | L2 |
| SvelteKit | npm dependency + configuration/source evidence | SvelteKit pages and components | file-system pages and server endpoints | Svelte/JS/TypeScript imports | L3 |
| Astro | npm dependency + configuration/source evidence | Astro pages and components | file-system pages and endpoints | Astro/JS/TypeScript imports | L3 |
| NestJS | npm dependency + decorators/bootstrap evidence | controllers, services, modules, guards | controller HTTP decorators | TypeScript imports | L3 |
| Express | npm dependency + application/router evidence | applications and routers | verb-based application/router calls | JS/TypeScript imports | L3 |
| Fastify | npm dependency + application/plugin evidence | plugins and handlers | shorthand and full route declarations | JS/TypeScript imports | L3 |
| Hono | npm dependency + application evidence | applications and handlers | verb routes with base paths | JS/TypeScript imports | L3 |
| Django | Python dependency + project/source conventions | views and Python symbols | `path`/`re_path`, including mounted URL prefixes | Python imports | L3 |
| FastAPI | Python dependency + application/router evidence | routers and handlers | HTTP decorators | Python imports | L3 |
| Flask | Python dependency + application/blueprint evidence | views, blueprints and Python symbols | route decorators and HTTP shortcuts | Python imports | L3 |
| ASP.NET | Web SDK + source APIs | controllers and C# symbols | controller attributes and minimal APIs | C# namespace imports | L3 |
| Blazor | Blazor SDK/package + Razor evidence | Razor components | `@page` routes | Razor/C# imports | L3 |
| Symfony | Composer dependency + console/config/source evidence | controllers and PHP symbols | PHP `Route` attributes | generic PHP graph | L3 |
| Rails | Rails gem + application conventions | controllers and Ruby symbols | verb routes and REST resources | Ruby relative requires | L3 |
| Go Web | `net/http`, Gin, Echo, Fiber or Chi evidence | handlers and Go symbols | registered HTTP handlers/routes | Go imports | L3 |
| Rust Web | Actix, Axum, Rocket or Warp evidence | handlers and Rust symbols | route attributes/builders | Rust modules | L3 |
| Quarkus | Maven/Gradle dependency + source evidence | JAX-RS resources and CDI services | JAX-RS path/verb annotations | Java/Kotlin imports | L3 |
| Micronaut | Maven/Gradle dependency + source evidence | controllers and singleton services | HTTP annotations | Java/Kotlin imports | L3 |
| Ktor | Gradle/Maven dependency + source evidence | routing modules and Kotlin symbols | nested Ktor route builders | Kotlin imports | L3 |
| Phoenix | Mix dependency + router evidence | controllers, LiveViews and modules | scoped routes and REST resources | Elixir aliases/imports | L3 |
| Other recognized languages | file extension | file | — | — | L0 |

## Laravel coverage

- HTTP methods: `get`, `post`, `put`, `patch`, `delete`, `options`, `any`, `match`.
- Route groups: nested `prefix`, `middleware`, `name`, `domain` and `controller`.
- Resource routes: `resource`, `apiResource`, `resources`, `apiResources`, including
  `only` and `except` and nested resource paths.
- Handlers: controller arrays, `Controller@method`, invokable controllers and closures.
- Components: conventional `app` directories plus inheritance-based controller/model
  classification and file-based migrations.
- Dependencies: `use`, inheritance, interfaces, traits, constructor type injection,
  route-to-controller edges and common Eloquent relationship methods.
- Runtime: PHP and Composer requirements, including the Composer PHP constraint when
  present.

## Detection guarantees

PHP does not imply Laravel. A Composer package without `laravel/framework` remains a
generic Composer/PHP project. Laravel is activated only at confidence 60 or higher.
Unknown or malformed projects return the best partial generic model and diagnostics;
they do not fail solely because framework evidence is absent.

HTTP links require a unique local endpoint with a compatible method and route. Exact
paths receive 100 confidence, normalized parameter matches receive 90, and statically
absolute localhost calls are capped at 85. External absolute URLs, unresolved dynamic
base URLs and ambiguous matches are not linked.

Workspace package links are resolved from explicit local project identities and manifest
dependencies in package.json, composer.json, Cargo.toml, pyproject.toml and pom.xml. A
matching source import raises confidence; unresolved package names are not linked.

## Fixture matrix

Regression fixtures live in `fixtures/analysis` and include executable analysis cases
for Spring, Laravel, Angular, React, NestJS, Express, Django, FastAPI, ASP.NET, Symfony,
Rails, Go Web and Rust Web. A generated regression matrix additionally covers Next.js,
Vue, Nuxt, Svelte, SvelteKit, Astro, Fastify, Hono, Flask, Blazor, Quarkus, Micronaut,
Ktor and Phoenix. Generic PHP, Composer library PHP, a mixed monorepo, malformed input,
excluded heavy directories and an empty project are also covered. Oversized-file
behavior is exercised by generated test data so a multi-megabyte file does not need to
be committed. `quality/framework-ground-truth.tsv` defines positive and negative
framework ground truth; CI calculates precision, recall and F1 and rejects regressions
below the configured thresholds.
