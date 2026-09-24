export type Technology = {
  category: string;
  name: string;
  classification: string;
  confidence: number;
  evidence: Evidence[];
  scope: string;
  repositoryId?: string;
  projectUnitId?: string;
  detectedFrom: string[];
  detectionKind: string;
};

export type Component = {
  id: string;
  name: string;
  kind: string;
  language: string;
  file: string;
  repositoryName?: string;
  projectUnitName?: string;
};

export type Entrypoint = {
  id: string;
  name: string;
  kind: string;
  componentId: string;
  file: string;
  repositoryName?: string;
  projectUnitName?: string;
};

export type Dependency = {
  sourceId: string;
  targetId: string;
  kind: string;
  evidence?: string;
  confidence?: number;
};

export type Evidence = { kind: string; source: string; detail: string; confidence: number };
export type FrameworkDetection = { name: string; confidence: number; evidence: Evidence[]; actionable: boolean; directEvidence: boolean };
export type RuntimeRequirement = { name: string; executable: string; versionHint: string | null; requiredBy: string; confidence: number };
export type SymbolMetadata = { symbolId: string; qualifiedName: string | null; namespace: string | null; frameworkKind: string | null; attributes: [string, string][]; evidence: Evidence[]; confidence: number };
export type EntrypointMetadata = { entrypointId: string; method: string | null; path: string | null; routeName: string | null; domain: string | null; middleware: string[]; controller: string | null; action: string | null; evidence: Evidence[]; confidence: number };
export type DependencyMetadata = { sourceId: string; targetId: string; kind: string; evidence: Evidence[]; confidence: number };
export type AnalysisDiagnostic = { level: string; code: string; message: string; file: string | null };
export type AnalysisMetadata = {
  frameworkDetections: FrameworkDetection[];
  symbols: SymbolMetadata[];
  entrypoints: EntrypointMetadata[];
  dependencies: DependencyMetadata[];
  runtimeRequirements: RuntimeRequirement[];
  diagnostics: AnalysisDiagnostic[];
};

export type ProjectModel = {
  root: string;
  name: string;
  files: number;
  compatibility: string;
  technologies: Technology[];
  components: Component[];
  entrypoints: Entrypoint[];
  dependencies: Dependency[];
  analysis: AnalysisMetadata;
};

export type WorkspaceRecord = { id: string; name: string };
export type AnalysisRecord = {
  analysisId: string;
  status: "COMPLETED" | "COMPLETED_WITH_WARNINGS" | "PARTIAL" | "FAILED" | string;
  startedAtMs: number;
  completedAtMs: number;
  durationMs: number;
  engineVersion: string;
  registrySignature: string;
  branch: string | null;
  commit: string | null;
  modelFingerprint: string;
  files: number;
  projectUnits: number;
  components: number;
  entrypoints: number;
  dependencies: number;
  crossProjectDependencies: number;
  graphPassed: boolean;
  graphClean: boolean;
  missingSources: number;
  missingTargets: number;
  selfDependencies: number;
  cycles: number;
  diagnosticsInfo: number;
  diagnosticsWarning: number;
  diagnosticsError: number;
  frameworkDetections: number;
  actionableFrameworks: number;
  message: string;
};
export type RepositoryRecord = { id: string; workspaceId: string; name: string; path: string; origin: "clone" | "local" | string; analysis: AnalysisRecord | null };
export type GitMetadata = { isGitRepository: boolean; branch: string | null; commit: string | null; remote: string | null };
export type HttpCall = { method: string; path: string; origin: string | null; componentId: string; file: string; evidence: string };
export type ProjectUnit = { id: string; repositoryId: string; root: string; name: string; role: string; model: ProjectModel; httpCalls: HttpCall[] };
export type UnitGraphAudit = {
  projectUnitId: string;
  projectUnitName: string;
  passed: boolean;
  clean: boolean;
  missingSources: string[];
  missingTargets: string[];
  selfDependencies: number;
  cycles: string[][];
};
export type RepositoryAudit = { analysis: AnalysisRecord | null; units: UnitGraphAudit[] };
export type Repository = { id: string; name: string; root: string; origin: "clone" | "local" | string; git: GitMetadata | null; projectUnits: ProjectUnit[]; audit: RepositoryAudit };
export type CrossProjectDependency = {
  sourceRepositoryId: string;
  sourceProjectUnitId: string;
  sourceComponentId: string;
  targetRepositoryId: string;
  targetProjectUnitId: string;
  targetEntrypointId: string | null;
  targetComponentId: string | null;
  kind: string;
  evidence: string;
  confidence: number;
};
export type WorkspaceGraphIntegrity = {
  passed: boolean;
  errorCount: number;
  missingSourceUnits: string[];
  missingTargetUnits: string[];
  missingSourceComponents: string[];
  missingTargetComponents: string[];
  missingTargetEntrypoints: string[];
};
export type WorkspaceModel = { id: string; name: string; repositories: Repository[]; crossProjectDependencies: CrossProjectDependency[]; graphIntegrity: WorkspaceGraphIntegrity };

export type CapsuleSummary = {
  path: string;
  target: string;
};

export type VerificationCheck = {
  name: string;
  passed: boolean;
  detail: string;
};

export type SandboxValidationPlan = {
  runtime: string;
  manifest: string;
  suggestedCommand: string;
  networkRequired: boolean;
  note: string;
};

export type VerificationReport = {
  passed: boolean;
  kind: "structural" | string;
  readinessScore: number;
  checks: VerificationCheck[];
  sandboxValidationPlan: SandboxValidationPlan[];
};

export type RuntimeCapabilities = {
  docker: boolean;
  java: boolean;
  node: boolean;
  python: boolean;
  php: boolean;
  composer: boolean;
  tools: { name: string; executable: string; available: boolean; version: string | null }[];
};

// Compact analysis result consumed by the three focused desktop views.
export type FocusedTechnology = {
  category: string;
  name: string;
  classification: string;
  confidence: number;
  detection_kind: string;
  evidence?: Evidence[];
};
export type FocusedBackendEndpoint = {
  id: string;
  method: string;
  path: string;
  file: string;
  creator: string;
  controller?: string | null;
  action?: string | null;
  line?: number | null;
};
export type FocusedFrontendCall = {
  method: string;
  path: string;
  component_id: string;
  file: string;
  symbol?: string | null;
  line?: number | null;
};
export type FocusedProjectUnit = {
  id: string;
  root: string;
  name: string;
  role: string;
  files: number;
  technologies: FocusedTechnology[];
  backend_endpoints: FocusedBackendEndpoint[];
  frontend_calls: FocusedFrontendCall[];
};
export type FocusedEndpointLink = {
  consumer_project_unit_id: string;
  consumer_component_id: string;
  consumer_file: string;
  consumer_symbol?: string | null;
  consumer_line?: number | null;
  method: string;
  path: string;
  provider_project_unit_id: string;
  provider_entrypoint_id: string;
  provider_file: string;
  confidence: number;
};
export type FocusedRepositoryAnalysis = {
  root: string;
  name: string;
  project_units: FocusedProjectUnit[];
  endpoint_links: FocusedEndpointLink[];
};

export type AnalysisProgress = {
  workspaceId: string;
  repositoryId: string | null;
  repositoryName: string | null;
  completedRepositories: number;
  totalRepositories: number;
  stage: "preparing" | "cache" | "scanning" | "repository-complete" | "validating" | "linking" | "completed" | string;
  reusedCache: boolean;
};


export type EngineState = "checking" | "ready" | "unavailable";

export type View =
  | "overview"
  | "projects"
  | "architecture"
  | "audit"
  | "components"
  | "entrypoints"
  | "dependencies"
  | "capsules";
