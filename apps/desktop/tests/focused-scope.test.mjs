import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";

const app = fs.readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const client = fs.readFileSync(new URL("../src/lib/client.ts", import.meta.url), "utf8");
const types = fs.readFileSync(new URL("../src/types.ts", import.meta.url), "utf8");

test("desktop exposes only focused analysis views", () => {
  assert.match(app, /Resumen/);
  assert.match(app, /Tecnologías/);
  assert.match(app, /Endpoints/);
  assert.doesNotMatch(app, /CapsulesView|ArchitectureView|DependenciesView|AuditView/);
});

test("focused desktop entry calls its analyzer while legacy workspace APIs remain available", () => {
  assert.match(client, /analyze_source_command/);
  assert.match(client, /load_cached_workspace_command/);
  assert.match(client, /create_workspace_capsule_command/);
});

test("focused transport DTOs stay compact alongside the retained workspace model", () => {
  assert.match(types, /backend_endpoints/);
  assert.match(types, /frontend_calls/);
  assert.match(types, /endpoint_links/);
  assert.match(types, /ProjectModel/);
  assert.match(types, /RuntimeCapabilities/);
});


test("desktop preserves focused GitHub source input", () => {
  assert.match(app, /Repositorio GitHub|GitHub/);
  assert.match(client, /analyze_source_command/);
  const tauri = fs.readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  assert.match(tauri, /clone_repository/);
  assert.match(tauri, /scan_workspace_repository/);
});
