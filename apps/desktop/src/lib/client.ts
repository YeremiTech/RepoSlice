import { invoke } from "@tauri-apps/api/core";
import type {
  CapsuleSummary,
  Dependency,
  RuntimeCapabilities,
  VerificationReport,
  WorkspaceRecord,
  RepositoryRecord,
  WorkspaceModel,
  FocusedRepositoryAnalysis,
} from "../types";

export function analyzeSource(source: string): Promise<FocusedRepositoryAnalysis> {
  return invoke("analyze_source_command", { source });
}

export function listWorkspaces(): Promise<WorkspaceRecord[]> { return invoke("list_workspaces_command"); }
export function createWorkspace(name: string): Promise<WorkspaceRecord> { return invoke("create_workspace_command", { name }); }
export function deleteWorkspace(workspaceId: string): Promise<void> { return invoke("delete_workspace_command", { workspaceId }); }
export function listRepositories(workspaceId: string): Promise<RepositoryRecord[]> { return invoke("list_repositories_command", { workspaceId }); }
export function addRepository(workspaceId: string, path: string): Promise<RepositoryRecord> { return invoke("add_repository_command", { workspaceId, path }); }
export function cloneRepository(workspaceId: string, url: string): Promise<RepositoryRecord> { return invoke("clone_repository_command", { workspaceId, url }); }
export function deleteRepository(workspaceId: string, repositoryId: string): Promise<void> { return invoke("delete_repository_command", { workspaceId, repositoryId }); }
export function updateRepository(workspaceId: string, repositoryId: string): Promise<RepositoryRecord> { return invoke("update_repository_command", { workspaceId, repositoryId }); }
export function loadCachedWorkspace(workspaceId: string): Promise<WorkspaceModel | null> { return invoke("load_cached_workspace_command", { workspaceId }); }
export function cancelAnalysis(workspaceId: string): Promise<boolean> { return invoke("cancel_analysis_command", { workspaceId }); }
export function scanWorkspace(workspaceId: string): Promise<WorkspaceModel> { return invoke("scan_workspace_command", { workspaceId }); }
export function scanRepository(workspaceId: string, repositoryId: string): Promise<WorkspaceModel> { return invoke("scan_repository_command", { workspaceId, repositoryId }); }
export function sliceWorkspaceDependencies(workspaceId: string, projectUnitId: string, target: string): Promise<Dependency[]> { return invoke("slice_workspace_dependencies_command", { workspaceId, projectUnitId, target }); }
export function impactWorkspaceDependencies(workspaceId: string, projectUnitId: string, target: string): Promise<Dependency[]> { return invoke("impact_workspace_dependencies_command", { workspaceId, projectUnitId, target }); }
export function createWorkspaceCapsule(workspaceId: string, repositoryId: string, projectUnitId: string, target: string): Promise<CapsuleSummary> { return invoke("create_workspace_capsule_command", { workspaceId, repositoryId, projectUnitId, target }); }
export function listWorkspaceCapsules(workspaceId: string): Promise<CapsuleSummary[]> { return invoke("list_workspace_capsules_command", { workspaceId }); }
export function resetAll(): Promise<void> { return invoke("reset_all_command"); }

export function verifyCapsule(path: string): Promise<VerificationReport> {
  return invoke<VerificationReport>("verify_capsule_command", { path });
}

export function detectRuntime(): Promise<RuntimeCapabilities> {
  return invoke<RuntimeCapabilities>("detect_runtime_command");
}

