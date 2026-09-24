use reposlice_core::{CrossProjectDependencyKind, EntrypointKind, WorkspaceModel};
use serde::Serialize;

#[derive(Serialize)]
pub struct FocusedRepositoryAnalysis {
    pub root: String,
    pub name: String,
    pub project_units: Vec<FocusedProjectUnit>,
    pub endpoint_links: Vec<FocusedEndpointLink>,
}

#[derive(Serialize)]
pub struct FocusedProjectUnit {
    pub id: String,
    pub root: String,
    pub name: String,
    pub role: String,
    pub files: usize,
    pub technologies: Vec<FocusedTechnology>,
    pub backend_endpoints: Vec<FocusedBackendEndpoint>,
    pub frontend_calls: Vec<FocusedFrontendCall>,
}

#[derive(Serialize)]
pub struct FocusedTechnology {
    pub category: String,
    pub name: String,
    pub classification: String,
    pub confidence: u8,
    pub detection_kind: String,
    pub evidence: Vec<FocusedEvidence>,
}

#[derive(Serialize)]
pub struct FocusedEvidence {
    pub kind: String,
    pub source: String,
    pub detail: String,
    pub confidence: u8,
}

#[derive(Serialize)]
pub struct FocusedBackendEndpoint {
    pub id: String,
    pub method: String,
    pub path: String,
    pub file: String,
    pub creator: String,
    pub controller: Option<String>,
    pub action: Option<String>,
    pub line: Option<usize>,
}

#[derive(Serialize)]
pub struct FocusedFrontendCall {
    pub method: String,
    pub path: String,
    pub component_id: String,
    pub file: String,
    pub symbol: Option<String>,
    pub line: Option<usize>,
}

#[derive(Serialize)]
pub struct FocusedEndpointLink {
    pub consumer_project_unit_id: String,
    pub consumer_component_id: String,
    pub consumer_file: String,
    pub consumer_symbol: Option<String>,
    pub consumer_line: Option<usize>,
    pub method: String,
    pub path: String,
    pub provider_project_unit_id: String,
    pub provider_entrypoint_id: String,
    pub provider_file: String,
    pub confidence: u8,
}

pub fn from_workspace(
    model: WorkspaceModel,
    repository_id: &str,
) -> Result<FocusedRepositoryAnalysis, String> {
    let repository = model
        .repositories
        .iter()
        .find(|repository| repository.id == repository_id)
        .ok_or_else(|| {
            "The analyzed repository was not returned by the analysis engine".to_string()
        })?;

    let project_units = repository
        .project_units
        .iter()
        .map(|unit| {
            let backend_endpoints = unit
                .model
                .entrypoints
                .iter()
                .filter(|entrypoint| entrypoint.kind == EntrypointKind::HttpEndpoint)
                .filter_map(|entrypoint| {
                    let (method, path) = entrypoint.name.split_once(' ')?;
                    let metadata = unit
                        .model
                        .analysis
                        .entrypoints
                        .iter()
                        .find(|metadata| metadata.entrypoint_id == entrypoint.id);
                    let controller = metadata.and_then(|item| item.controller.clone());
                    let action = metadata.and_then(|item| item.action.clone());
                    let creator = match (controller.as_deref(), action.as_deref()) {
                        (Some(controller), Some(action)) => format!("{controller}.{action}"),
                        (Some(controller), None) => controller.to_string(),
                        (None, Some(action)) => action.to_string(),
                        (None, None) => entrypoint.name.clone(),
                    };
                    Some(FocusedBackendEndpoint {
                        id: entrypoint.id.clone(),
                        method: method.to_ascii_uppercase(),
                        path: path.to_string(),
                        file: entrypoint.file.clone(),
                        creator,
                        controller,
                        action,
                        // The current endpoint model has file paths but no source line.
                        line: None,
                    })
                })
                .collect();

            FocusedProjectUnit {
                id: unit.id.clone(),
                root: unit.root.clone(),
                name: unit.name.clone(),
                role: unit.role.as_str().to_string(),
                files: unit.model.files,
                technologies: unit
                    .model
                    .technologies
                    .iter()
                    .map(|technology| FocusedTechnology {
                        category: technology.category.clone(),
                        name: technology.name.clone(),
                        classification: technology.classification.clone(),
                        confidence: technology.confidence,
                        detection_kind: technology.detection_kind.clone(),
                        evidence: technology
                            .evidence
                            .iter()
                            .map(|evidence| FocusedEvidence {
                                kind: evidence.kind.as_str().to_string(),
                                source: evidence.source.clone(),
                                detail: evidence.detail.clone(),
                                confidence: evidence.confidence,
                            })
                            .collect(),
                    })
                    .collect(),
                backend_endpoints,
                frontend_calls: unit
                    .http_calls
                    .iter()
                    .map(|call| FocusedFrontendCall {
                        method: call.method.to_ascii_uppercase(),
                        path: call.path.clone(),
                        component_id: call.component_id.clone(),
                        file: call.file.clone(),
                        symbol: unit
                            .model
                            .components
                            .iter()
                            .find(|component| component.id == call.component_id)
                            .map(|component| component.name.clone()),
                        // The current call model has source paths but no source line.
                        line: None,
                    })
                    .collect(),
            }
        })
        .collect();

    let endpoint_links = model
        .cross_project_dependencies
        .iter()
        .filter(|dependency| {
            dependency.source_repository_id == repository_id
                && dependency.target_repository_id == repository_id
                && dependency.kind == CrossProjectDependencyKind::Http
        })
        .filter_map(|dependency| {
            let source = repository
                .project_units
                .iter()
                .find(|unit| unit.id == dependency.source_project_unit_id)?;
            let target = repository
                .project_units
                .iter()
                .find(|unit| unit.id == dependency.target_project_unit_id)?;
            let call = source.http_calls.iter().find(|call| {
                call.component_id == dependency.source_component_id
                    && call.evidence == dependency.evidence
            })?;
            let endpoint_id = dependency.target_entrypoint_id.as_deref()?;
            let endpoint = target
                .model
                .entrypoints
                .iter()
                .find(|entrypoint| entrypoint.id == endpoint_id)?;
            Some(FocusedEndpointLink {
                consumer_project_unit_id: source.id.clone(),
                consumer_component_id: call.component_id.clone(),
                consumer_file: call.file.clone(),
                consumer_symbol: source
                    .model
                    .components
                    .iter()
                    .find(|component| component.id == call.component_id)
                    .map(|component| component.name.clone()),
                consumer_line: None,
                method: call.method.to_ascii_uppercase(),
                path: endpoint
                    .name
                    .split_once(' ')
                    .map_or_else(|| endpoint.name.clone(), |(_, path)| path.to_string()),
                provider_project_unit_id: target.id.clone(),
                provider_entrypoint_id: endpoint.id.clone(),
                provider_file: endpoint.file.clone(),
                confidence: dependency.confidence,
            })
        })
        .collect();

    Ok(FocusedRepositoryAnalysis {
        root: repository.root.clone(),
        name: repository.name.clone(),
        project_units,
        endpoint_links,
    })
}
