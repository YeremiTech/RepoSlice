use reposlice_capsule::{
    capsules_root, create_capsule, create_workspace_capsule, list_capsules_in,
};
use reposlice_core::{ScopedTarget, WorkspaceModel};
use reposlice_graph::{
    dependency_slice, export_workspace_graphml, export_workspace_mermaid, impact_slice, validate_dependency_graph, validate_workspace_graph,
};
use reposlice_runtime::detect_runtime;
use reposlice_scanner::{clear_analysis_cache, scan_project};
use reposlice_verifier::{sandbox_validation_plan, verify_capsule};
use reposlice_workspace::{
    add_project, add_repository, clone_repository, create_workspace, delete_repository,
    latest_analysis_record, list_analysis_records, list_projects, list_repositories, list_workspaces,
    load_cached_workspace_model,
    scan_workspace, scan_workspace_repository, update_managed_repository, workspace_root,
    install_local_crash_reporting, AnalysisRecord,
};
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    install_local_crash_reporting();
    let arguments: Vec<String> = env::args().collect();
    let json = format_is_json(&arguments);
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json {
                eprintln!(
                    "{{\"ok\":false,\"error\":{{\"code\":{},\"message\":{}}}}}",
                    json_string(error_code(&error)),
                    json_string(&error)
                );
            } else {
                eprintln!("RepoSlice error: {error}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &[String]) -> Result<(), String> {
    match arguments.get(1).map(String::as_str) {
        Some("add-project") => command_add_project(arguments),
        Some("projects") => command_projects(arguments),
        Some("scan") => command_scan(arguments),
        Some("benchmark") => command_benchmark(arguments),
        Some("scan-workspace") => command_scan_workspace(arguments),
        Some("scan-repository") => command_scan_repository(arguments),
        Some("cached-workspace") => command_cached_workspace(arguments),
        Some("validate-workspace") => command_validate_workspace(arguments),
        Some("export-workspace") => command_export_workspace(arguments),
        Some("remove-repository") => command_remove_repository(arguments),
        Some("update-repository") => command_update_repository(arguments),
        Some("audit") => command_audit(arguments),
        Some("add-repository") => command_add_repository(arguments),
        Some("repositories") => command_repositories(arguments),
        Some("clone-repository") => command_clone_repository(arguments),
        Some("create-workspace") => command_create_workspace(arguments),
        Some("workspaces") => command_workspaces(arguments),
        Some("entrypoints") => command_entrypoints(arguments),
        Some("components") => command_components(arguments),
        Some("graph") => command_graph(arguments),
        Some("slice") => command_slice(arguments),
        Some("impact") => command_impact(arguments),
        Some("create") => command_create(arguments),
        Some("create-workspace-capsule") => command_create_workspace_capsule(arguments),
        Some("workspace-capsules") => command_workspace_capsules(arguments),
        Some("verify") => command_verify(arguments),
        Some("runtime") => command_runtime(arguments),
        Some("version") | Some("--version") | Some("-V") => {
            println!("reposlice {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("help") | Some("--help") | Some("-h") | None => {
            print_help();
            Ok(())
        }
        Some(command) => Err(format!("Unknown command: {command}")),
    }
}

fn command_create_workspace(arguments: &[String]) -> Result<(), String> {
    let name_parts = arguments
        .iter()
        .skip(2)
        .take_while(|argument| !argument.starts_with("--"))
        .cloned()
        .collect::<Vec<_>>();
    if name_parts.is_empty() {
        return Err("A workspace name is required".to_string());
    }
    let workspace = create_workspace(&name_parts.join(" ")).map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        println!(
            "{{\"id\":{},\"name\":{}}}",
            json_string(&workspace.id),
            json_string(&workspace.name)
        );
    } else {
        println!("{}\t{}", workspace.id, workspace.name);
    }
    Ok(())
}

fn command_workspaces(arguments: &[String]) -> Result<(), String> {
    let workspaces = list_workspaces().map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        let rows = workspaces
            .iter()
            .map(|workspace| {
                format!(
                    "{{\"id\":{},\"name\":{}}}",
                    json_string(&workspace.id),
                    json_string(&workspace.name)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!("[{rows}]");
    } else {
        for workspace in workspaces {
            println!("{}\t{}", workspace.id, workspace.name);
        }
    }
    Ok(())
}

fn command_clone_repository(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let url = arguments
        .get(3)
        .ok_or_else(|| "A repository URL is required".to_string())?;
    let repository = clone_repository(workspace_id, url).map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        println!(
            "{{\"id\":{},\"name\":{},\"path\":{}}}",
            json_string(&repository.id),
            json_string(&repository.name),
            json_string(&repository.path)
        );
    } else {
        println!("{}\t{}", repository.id, repository.path);
    }
    Ok(())
}

fn command_add_repository(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let path = path_argument(arguments, 3)?;
    let repository = add_repository(workspace_id, &path).map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        println!(
            "{{\"id\":{},\"name\":{},\"path\":{}}}",
            json_string(&repository.id),
            json_string(&repository.name),
            json_string(&repository.path)
        );
    } else {
        println!("Added repository: {}", repository.name);
        println!("{}", repository.path);
    }
    Ok(())
}

fn command_repositories(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let repositories = list_repositories(workspace_id).map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        let rows = repositories
            .iter()
            .map(|repository| {
                format!(
                    "{{\"id\":{},\"name\":{},\"path\":{}}}",
                    json_string(&repository.id),
                    json_string(&repository.name),
                    json_string(&repository.path)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!("[{}]", rows);
    } else {
        for repository in repositories {
            println!("{}\t{}\t{}", repository.id, repository.name, repository.path);
        }
    }
    Ok(())
}

fn command_scan_workspace(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let workspace = scan_workspace(workspace_id).map_err(|error| error.to_string())?;
    print_workspace_result(arguments, &workspace);
    Ok(())
}

fn command_scan_repository(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let repository_id = arguments
        .get(3)
        .ok_or_else(|| "A repository id is required".to_string())?;
    let workspace = scan_workspace_repository(workspace_id, repository_id)
        .map_err(|error| error.to_string())?;
    print_workspace_result(arguments, &workspace);
    Ok(())
}

fn command_cached_workspace(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let cached = load_cached_workspace_model(workspace_id).map_err(|error| error.to_string())?;
    match cached {
        Some(workspace) => print_workspace_result(arguments, &workspace),
        None if format_is_json(arguments) => println!("{{\"cached\":false}}"),
        None => println!("No current workspace model cache is available"),
    }
    Ok(())
}

fn command_validate_workspace(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let workspace = load_cached_workspace_model(workspace_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No current workspace model cache is available; run scan-workspace first".to_string())?;

    let cross_report = validate_workspace_graph(&workspace);
    let local_errors = workspace
        .repositories
        .iter()
        .flat_map(|repository| repository.project_units.iter())
        .map(|unit| {
            let report = validate_dependency_graph(&unit.model);
            report.missing_sources.len() + report.missing_targets.len()
        })
        .sum::<usize>();
    let passed = cross_report.passed() && local_errors == 0;

    if format_is_json(arguments) {
        println!(
            "{{\"workspace_id\":{},\"passed\":{},\"local_graph_errors\":{},\"cross_project_errors\":{},\"repositories\":{},\"cross_project_dependencies\":{}}}",
            json_string(&workspace.id),
            passed,
            local_errors,
            cross_report.error_count(),
            workspace.repositories.len(),
            workspace.cross_project_dependencies.len()
        );
    } else {
        println!("Workspace: {}", workspace.name);
        println!("Repositories: {}", workspace.repositories.len());
        println!("Local graph errors: {local_errors}");
        println!("Cross-project graph errors: {}", cross_report.error_count());
        println!("Integrity: {}", if passed { "PASS" } else { "FAIL" });
    }

    if passed {
        Ok(())
    } else {
        Err(format!(
            "Workspace integrity validation failed: {local_errors} local graph error(s), {} cross-project graph error(s)",
            cross_report.error_count()
        ))
    }
}

fn command_export_workspace(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments.get(2).ok_or_else(|| "A workspace id is required".to_string())?;
    let format = option_value(arguments, "--format").unwrap_or_else(|| "json".to_string());
    let model = load_cached_workspace_model(workspace_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No valid cached workspace model is available. Run scan-workspace first.".to_string())?;
    let content = match format.to_ascii_lowercase().as_str() {
        "json" => serde_json::to_string_pretty(&model).map_err(|error| error.to_string())?,
        "mermaid" | "mmd" => export_workspace_mermaid(&model),
        "graphml" => export_workspace_graphml(&model),
        other => return Err(format!("Unsupported export format: {other}. Use json, mermaid or graphml")),
    };
    if let Some(output) = option_value(arguments, "--output") {
        std::fs::write(&output, &content).map_err(|error| error.to_string())?;
        if !format_is_json(arguments) { println!("Workspace export: {output}"); }
    } else {
        println!("{content}");
    }
    Ok(())
}

fn command_remove_repository(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let repository_id = arguments
        .get(3)
        .ok_or_else(|| "A repository id is required".to_string())?;
    delete_repository(workspace_id, repository_id).map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        println!("{{\"ok\":true}}");
    } else {
        println!("Repository removed from workspace");
    }
    Ok(())
}

fn command_update_repository(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let repository_id = arguments
        .get(3)
        .ok_or_else(|| "A repository id is required".to_string())?;
    let repository = update_managed_repository(workspace_id, repository_id)
        .map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        println!(
            "{{\"id\":{},\"name\":{},\"path\":{}}}",
            json_string(&repository.id),
            json_string(&repository.name),
            json_string(&repository.path)
        );
    } else {
        println!("Updated repository: {}", repository.name);
    }
    Ok(())
}

fn print_workspace_result(arguments: &[String], workspace: &WorkspaceModel) {
    let project_units = workspace
        .repositories
        .iter()
        .map(|repository| repository.project_units.len())
        .sum::<usize>();
    if format_is_json(arguments) {
        let dependencies = workspace
            .cross_project_dependencies
            .iter()
            .map(|dependency| {
                format!(
                    "{{\"source_repository_id\":{},\"source_project_unit_id\":{},\"source_component_id\":{},\"target_repository_id\":{},\"target_project_unit_id\":{},\"target_entrypoint_id\":{},\"target_component_id\":{},\"kind\":{},\"confidence\":{}}}",
                    json_string(&dependency.source_repository_id),
                    json_string(&dependency.source_project_unit_id),
                    json_string(&dependency.source_component_id),
                    json_string(&dependency.target_repository_id),
                    json_string(&dependency.target_project_unit_id),
                    json_option_string(dependency.target_entrypoint_id.as_deref()),
                    json_option_string(dependency.target_component_id.as_deref()),
                    json_string(dependency.kind.as_str()),
                    dependency.confidence
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"id\":{},\"name\":{},\"repositories\":{},\"project_units\":{},\"cross_project_dependencies\":[{}]}}",
            json_string(&workspace.id),
            json_string(&workspace.name),
            workspace.repositories.len(),
            project_units,
            dependencies
        );
    } else {
        println!("Workspace: {}", workspace.name);
        println!("Repositories: {}", workspace.repositories.len());
        println!("Project units: {project_units}");
        println!(
            "Cross-project dependencies: {}",
            workspace.cross_project_dependencies.len()
        );
    }
}

fn command_audit(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let repository_id = option_optional(arguments, "--repository");
    let repositories = list_repositories(workspace_id).map_err(|error| error.to_string())?;
    let records = if let Some(repository_id) = repository_id.as_deref() {
        latest_analysis_record(workspace_id, repository_id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        list_analysis_records(workspace_id).map_err(|error| error.to_string())?
    };

    if format_is_json(arguments) {
        let rows = records
            .iter()
            .map(|record| analysis_record_json(record, &repositories))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"workspace_id\":{},\"analyses\":[{}]}}",
            json_string(workspace_id),
            rows
        );
    } else if records.is_empty() {
        println!("No analysis records found");
    } else {
        for record in records {
            let repository_name = repositories
                .iter()
                .find(|repository| repository.id == record.repository_id)
                .map(|repository| repository.name.as_str())
                .unwrap_or(record.repository_id.as_str());
            println!("Repository: {repository_name}");
            println!("Analysis: {}", record.analysis_id);
            println!("Status: {}", record.status.as_str());
            println!("Commit: {}", record.commit.as_deref().unwrap_or("-"));
            println!("Duration: {} ms", record.duration_ms);
            println!(
                "Coverage: {} files, {} units, {} components, {} entrypoints, {} dependencies",
                record.files,
                record.project_units,
                record.components,
                record.entrypoints,
                record.dependencies
            );
            println!(
                "Graph: {} / clean={} / cycles={} / missing={}+{}",
                if record.graph_passed { "PASS" } else { "FAIL" },
                record.graph_clean,
                record.cycles,
                record.missing_sources,
                record.missing_targets
            );
            println!(
                "Diagnostics: {} info, {} warnings, {} errors",
                record.diagnostics_info,
                record.diagnostics_warning,
                record.diagnostics_error
            );
            println!(
                "Frameworks: {} detected, {} actionable",
                record.framework_detections,
                record.actionable_frameworks
            );
            println!("Registry: {}", record.registry_signature);
            println!("Fingerprint: {}", record.model_fingerprint);
            println!();
        }
    }
    Ok(())
}

fn analysis_record_json(
    record: &AnalysisRecord,
    repositories: &[reposlice_workspace::RepositoryRecord],
) -> String {
    let repository_name = repositories
        .iter()
        .find(|repository| repository.id == record.repository_id)
        .map(|repository| repository.name.as_str())
        .unwrap_or(record.repository_id.as_str());
    format!(
        "{{\"repository_id\":{},\"repository_name\":{},\"analysis_id\":{},\"status\":{},\"started_at_ms\":{},\"completed_at_ms\":{},\"duration_ms\":{},\"engine_version\":{},\"registry_signature\":{},\"branch\":{},\"commit\":{},\"model_fingerprint\":{},\"files\":{},\"project_units\":{},\"components\":{},\"entrypoints\":{},\"dependencies\":{},\"cross_project_dependencies\":{},\"graph_passed\":{},\"graph_clean\":{},\"missing_sources\":{},\"missing_targets\":{},\"self_dependencies\":{},\"cycles\":{},\"diagnostics_info\":{},\"diagnostics_warning\":{},\"diagnostics_error\":{},\"framework_detections\":{},\"actionable_frameworks\":{},\"message\":{}}}",
        json_string(&record.repository_id),
        json_string(repository_name),
        json_string(&record.analysis_id),
        json_string(record.status.as_str()),
        record.started_at_ms,
        record.completed_at_ms,
        record.duration_ms,
        json_string(&record.engine_version),
        json_string(&record.registry_signature),
        json_option_string(record.branch.as_deref()),
        json_option_string(record.commit.as_deref()),
        json_string(&record.model_fingerprint),
        record.files,
        record.project_units,
        record.components,
        record.entrypoints,
        record.dependencies,
        record.cross_project_dependencies,
        record.graph_passed,
        record.graph_clean,
        record.missing_sources,
        record.missing_targets,
        record.self_dependencies,
        record.cycles,
        record.diagnostics_info,
        record.diagnostics_warning,
        record.diagnostics_error,
        record.framework_detections,
        record.actionable_frameworks,
        json_string(&record.message)
    )
}

fn command_add_project(arguments: &[String]) -> Result<(), String> {
    let path = path_argument(arguments, 2)?;
    let project = add_project(&path).map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        println!(
            "{{\"name\":{},\"path\":{}}}",
            json_string(&project.name),
            json_string(&project.path)
        );
    } else {
        println!("Added project: {}", project.name);
        println!("{}", project.path);
    }
    Ok(())
}

fn command_projects(arguments: &[String]) -> Result<(), String> {
    let projects = list_projects().map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        let rows = projects
            .iter()
            .map(|project| {
                format!(
                    "{{\"name\":{},\"path\":{}}}",
                    json_string(&project.name),
                    json_string(&project.path)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!("[{rows}]");
    } else {
        for project in projects {
            println!("{}\t{}", project.name, project.path);
        }
    }
    Ok(())
}

fn command_scan(arguments: &[String]) -> Result<(), String> {
    let path = path_argument(arguments, 2)?;
    let model = scan_project(&path).map_err(|error| error.to_string())?;
    let integrity = validate_dependency_graph(&model);
    if format_is_json(arguments) {
        let technologies = model
            .technologies
            .iter()
            .map(|technology| {
                format!(
                    "{{\"category\":{},\"name\":{},\"classification\":{},\"confidence\":{},\"detection_kind\":{}}}",
                    json_string(&technology.category),
                    json_string(&technology.name),
                    json_string(&technology.classification),
                    technology.confidence,
                    json_string(&technology.detection_kind)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let diagnostics = model
            .analysis
            .diagnostics
            .iter()
            .map(|diagnostic| {
                format!(
                    "{{\"level\":{},\"code\":{},\"message\":{}}}",
                    json_string(diagnostic.level.as_str()),
                    json_string(&diagnostic.code),
                    json_string(&diagnostic.message)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"project\":{},\"root\":{},\"files\":{},\"compatibility\":{},\"components\":{},\"entrypoints\":{},\"dependencies\":{},\"graph_integrity\":{{\"passed\":{},\"clean\":{},\"missing_sources\":{},\"missing_source_ids\":{},\"missing_targets\":{},\"missing_target_ids\":{},\"self_dependencies\":{},\"self_dependency_ids\":{},\"cycles\":{},\"cycle_paths\":{}}},\"technologies\":[{}],\"diagnostics\":[{}]}}",
            json_string(&model.name),
            json_string(&model.root),
            model.files,
            json_string(model.compatibility.as_str()),
            model.components.len(),
            model.entrypoints.len(),
            model.dependencies.len(),
            integrity.passed(),
            integrity.is_clean(),
            integrity.missing_sources.len(),
            json_string_list(&integrity.missing_sources),
            integrity.missing_targets.len(),
            json_string_list(&integrity.missing_targets),
            integrity.self_dependencies.len(),
            json_string_list(&integrity.self_dependencies),
            integrity.cycles.len(),
            json_cycles(&integrity.cycles),
            technologies,
            diagnostics
        );
    } else {
        println!("Project: {}", model.name);
        println!("Root: {}", model.root);
        println!("Files: {}", model.files);
        println!("Compatibility: {}", model.compatibility.as_str());
        println!("Components: {}", model.components.len());
        println!("Entrypoints: {}", model.entrypoints.len());
        println!("Dependencies: {}", model.dependencies.len());
        println!("Graph integrity: {}", if integrity.passed() { "PASS" } else { "FAIL" });
        println!("Graph clean: {}", if integrity.is_clean() { "YES" } else { "NO" });
        println!("Graph cycles: {}", integrity.cycles.len());
        println!();
        for technology in model.technologies {
            println!(
                "{:<12} {:<20} {}%",
                technology.category, technology.name, technology.confidence
            );
        }
    }
    Ok(())
}

fn command_benchmark(arguments: &[String]) -> Result<(), String> {
    let path = path_argument(arguments, 2)?;
    let runs = option_optional(arguments, "--runs")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3)
        .clamp(1, 10);

    clear_analysis_cache();
    let mut durations_ms = Vec::with_capacity(runs);
    let mut last_model = None;
    for _ in 0..runs {
        let started = Instant::now();
        let model = scan_project(&path).map_err(|error| error.to_string())?;
        durations_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        last_model = Some(model);
    }
    let model = last_model.ok_or_else(|| "Benchmark produced no scan result".to_string())?;
    let cold_ms = durations_ms[0];
    let warm_ms = if durations_ms.len() > 1 {
        durations_ms.iter().skip(1).sum::<f64>() / (durations_ms.len() - 1) as f64
    } else {
        cold_ms
    };
    let cold_files_per_second = if cold_ms > 0.0 { model.files as f64 / (cold_ms / 1000.0) } else { 0.0 };
    let warm_files_per_second = if warm_ms > 0.0 { model.files as f64 / (warm_ms / 1000.0) } else { 0.0 };
    let mut sorted_durations = durations_ms.clone();
    sorted_durations.sort_by(|left, right| left.total_cmp(right));
    let p95_index = ((sorted_durations.len() as f64 * 0.95).ceil() as usize).saturating_sub(1).min(sorted_durations.len().saturating_sub(1));
    let p95_ms = sorted_durations.get(p95_index).copied().unwrap_or(cold_ms);
    let model_json_bytes = serde_json::to_vec(&model).map(|value| value.len()).unwrap_or(0);
    let diagnostic_count = model.analysis.diagnostics.len();

    if format_is_json(arguments) {
        let runs_json = durations_ms.iter().map(|value| format!("{value:.3}")).collect::<Vec<_>>().join(",");
        println!(
            "{{\"project\":{},\"files\":{},\"components\":{},\"entrypoints\":{},\"dependencies\":{},\"diagnostics\":{},\"compatibility\":{},\"model_json_bytes\":{},\"runs\":{},\"durations_ms\":[{}],\"cold_ms\":{:.3},\"warm_average_ms\":{:.3},\"p95_ms\":{:.3},\"cold_files_per_second\":{:.1},\"warm_files_per_second\":{:.1}}}",
            json_string(&model.name), model.files, model.components.len(), model.entrypoints.len(), model.dependencies.len(), diagnostic_count,
            json_string(model.compatibility.as_str()), model_json_bytes, runs, runs_json, cold_ms, warm_ms, p95_ms, cold_files_per_second, warm_files_per_second
        );
    } else {
        println!("Project: {}", model.name);
        println!("Files: {}", model.files);
        println!("Components: {} | Entrypoints: {} | Dependencies: {}", model.components.len(), model.entrypoints.len(), model.dependencies.len());
        println!("Model JSON size: {} bytes | Diagnostics: {} | Compatibility: {}", model_json_bytes, diagnostic_count, model.compatibility.as_str());
        println!("Runs: {}", runs);
        println!("Cold scan: {:.3} ms ({:.1} files/s)", cold_ms, cold_files_per_second);
        println!("Warm average: {:.3} ms ({:.1} files/s)", warm_ms, warm_files_per_second);
        println!("P95 scan: {:.3} ms", p95_ms);
        for (index, duration) in durations_ms.iter().enumerate() {
            println!("Run {}: {:.3} ms", index + 1, duration);
        }
    }
    Ok(())
}


fn command_entrypoints(arguments: &[String]) -> Result<(), String> {
    let path = path_argument(arguments, 2)?;
    let model = scan_project(&path).map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        let rows = model
            .entrypoints
            .iter()
            .map(|entrypoint| {
                format!(
                    "{{\"id\":{},\"kind\":{},\"name\":{},\"component_id\":{},\"file\":{}}}",
                    json_string(&entrypoint.id),
                    json_string(entrypoint.kind.as_str()),
                    json_string(&entrypoint.name),
                    json_string(&entrypoint.component_id),
                    json_string(&entrypoint.file)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!("[{rows}]");
    } else {
        for entrypoint in model.entrypoints {
            println!(
                "{}\t{}\t{}",
                entrypoint.id,
                entrypoint.kind.as_str(),
                entrypoint.name
            );
        }
    }
    Ok(())
}

fn command_components(arguments: &[String]) -> Result<(), String> {
    let path = path_argument(arguments, 2)?;
    let model = scan_project(&path).map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        let rows = model
            .components
            .iter()
            .map(component_json)
            .collect::<Vec<_>>()
            .join(",");
        println!("[{rows}]");
    } else {
        for component in model.components {
            println!(
                "{}\t{}\t{}\t{}",
                component.id,
                component.kind.as_str(),
                component.language,
                component.name
            );
        }
    }
    Ok(())
}

fn command_graph(arguments: &[String]) -> Result<(), String> {
    let path = path_argument(arguments, 2)?;
    let model = scan_project(&path).map_err(|error| error.to_string())?;
    let integrity = validate_dependency_graph(&model);
    if format_is_json(arguments) {
        let rows = model
            .dependencies
            .iter()
            .map(|dependency| {
                format!(
                    "{{\"source_id\":{},\"kind\":{},\"target_id\":{}}}",
                    json_string(&dependency.source_id),
                    json_string(dependency.kind.as_str()),
                    json_string(&dependency.target_id)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"integrity\":{{\"passed\":{},\"clean\":{},\"missing_sources\":{},\"missing_source_ids\":{},\"missing_targets\":{},\"missing_target_ids\":{},\"self_dependencies\":{},\"self_dependency_ids\":{},\"cycles\":{},\"cycle_paths\":{}}},\"dependencies\":[{}]}}",
            integrity.passed(),
            integrity.is_clean(),
            integrity.missing_sources.len(),
            json_string_list(&integrity.missing_sources),
            integrity.missing_targets.len(),
            json_string_list(&integrity.missing_targets),
            integrity.self_dependencies.len(),
            json_string_list(&integrity.self_dependencies),
            integrity.cycles.len(),
            json_cycles(&integrity.cycles),
            rows
        );
    } else {
        for dependency in model.dependencies {
            println!(
                "{} --{}--> {}",
                dependency.source_id,
                dependency.kind.as_str(),
                dependency.target_id
            );
        }
        if !integrity.passed() {
            return Err("Dependency graph contains unresolved component references".into());
        }
    }
    Ok(())
}

fn command_slice(arguments: &[String]) -> Result<(), String> {
    let path = path_argument(arguments, 2)?;
    let target = option(arguments, "--target")?;
    let model = scan_project(&path).map_err(|error| error.to_string())?;
    let component_id = model
        .resolve_target_component_id(&target)
        .ok_or_else(|| format!("Target was not found: {target}"))?;
    let slice = dependency_slice(&model, component_id);
    output_dependency_selection(arguments, &model, "slice", &target, &slice.nodes, slice.dependencies.len());
    Ok(())
}

fn command_impact(arguments: &[String]) -> Result<(), String> {
    let path = path_argument(arguments, 2)?;
    let target = option(arguments, "--target")?;
    let model = scan_project(&path).map_err(|error| error.to_string())?;
    let component_id = model
        .resolve_target_component_id(&target)
        .ok_or_else(|| format!("Target was not found: {target}"))?;
    let impact = impact_slice(&model, component_id);
    output_dependency_selection(
        arguments,
        &model,
        "impact",
        &target,
        &impact.nodes,
        impact.dependencies.len(),
    );
    Ok(())
}

fn output_dependency_selection(
    arguments: &[String],
    model: &reposlice_core::ProjectModel,
    kind: &str,
    target: &str,
    nodes: &std::collections::BTreeSet<String>,
    dependency_count: usize,
) {
    let selected = model
        .components
        .iter()
        .filter(|component| nodes.contains(&component.id))
        .collect::<Vec<_>>();
    if format_is_json(arguments) {
        let rows = selected
            .iter()
            .map(|component| component_json(component))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"kind\":{},\"target\":{},\"components\":[{}],\"dependency_count\":{}}}",
            json_string(kind),
            json_string(target),
            rows,
            dependency_count
        );
    } else {
        for component in selected {
            println!(
                "{}\t{}\t{}\t{}",
                component.id,
                component.kind.as_str(),
                component.language,
                component.file
            );
        }
    }
}

fn component_json(component: &reposlice_core::Component) -> String {
    format!(
        "{{\"id\":{},\"name\":{},\"kind\":{},\"language\":{},\"file\":{}}}",
        json_string(&component.id),
        json_string(&component.name),
        json_string(component.kind.as_str()),
        json_string(&component.language),
        json_string(&component.file)
    )
}

fn command_create(arguments: &[String]) -> Result<(), String> {
    let path = path_argument(arguments, 2)?;
    let target = option(arguments, "--target")?;
    let output = option_optional(arguments, "--output")
        .map(PathBuf::from)
        .unwrap_or_else(|| capsules_root(&path));
    let model = scan_project(&path).map_err(|error| error.to_string())?;
    let capsule = create_capsule(&model, &target, &output).map_err(|error| error.to_string())?;
    output_path(arguments, &capsule);
    Ok(())
}

fn command_create_workspace_capsule(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let repository_id = arguments
        .get(3)
        .ok_or_else(|| "A repository id is required".to_string())?;
    let project_unit_id = arguments
        .get(4)
        .ok_or_else(|| "A project unit id is required".to_string())?;
    let target_id = option(arguments, "--target")?;
    let output = option_optional(arguments, "--output")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("capsules").join(workspace_id));
    let workspace = scan_workspace(workspace_id).map_err(|error| error.to_string())?;
    let target = ScopedTarget {
        repository_id: repository_id.clone(),
        project_unit_id: project_unit_id.clone(),
        target_id,
    };
    let capsule = create_workspace_capsule(&workspace, &target, &output)
        .map_err(|error| error.to_string())?;
    output_path(arguments, &capsule);
    Ok(())
}

fn output_path(arguments: &[String], path: &Path) {
    if format_is_json(arguments) {
        println!("{{\"path\":{}}}", json_string(&path.to_string_lossy()));
    } else {
        println!("{}", path.to_string_lossy());
    }
}

fn command_workspace_capsules(arguments: &[String]) -> Result<(), String> {
    let workspace_id = arguments
        .get(2)
        .ok_or_else(|| "A workspace id is required".to_string())?;
    let root = workspace_root().join("capsules").join(workspace_id);
    let capsules = list_capsules_in(&root).map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        let rows = capsules
            .iter()
            .map(|capsule| {
                format!(
                    "{{\"target\":{},\"path\":{}}}",
                    json_string(&capsule.target),
                    json_string(&capsule.path)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!("[{rows}]");
    } else {
        for capsule in capsules {
            println!("{}\t{}", capsule.target, capsule.path);
        }
    }
    Ok(())
}

fn command_verify(arguments: &[String]) -> Result<(), String> {
    let path = path_argument(arguments, 2)?;
    let report = verify_capsule(&path).map_err(|error| error.to_string())?;
    let readiness_score = report.readiness_score();
    let sandbox_plans = sandbox_validation_plan(&path).map_err(|error| error.to_string())?;
    if format_is_json(arguments) {
        let checks = report
            .checks
            .iter()
            .map(|check| {
                format!(
                    "{{\"name\":{},\"passed\":{},\"detail\":{}}}",
                    json_string(&check.name),
                    check.passed,
                    json_string(&check.detail)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let plans = sandbox_plans
            .iter()
            .map(|plan| {
                format!(
                    "{{\"runtime\":{},\"manifest\":{},\"suggested_command\":{},\"network_required\":{},\"note\":{}}}",
                    json_string(&plan.runtime),
                    json_string(&plan.manifest),
                    json_string(&plan.suggested_command),
                    plan.network_required,
                    json_string(&plan.note)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"kind\":{},\"passed\":{},\"readiness_score\":{},\"checks\":[{}],\"sandbox_validation_plan\":[{}]}}",
            json_string(&report.kind),
            report.passed(),
            readiness_score,
            checks,
            plans
        );
    } else {
        println!("Verification: {}", report.kind);
        println!("Static readiness: {}%", readiness_score);
        for check in &report.checks {
            println!(
                "{}\t{}\t{}",
                check.name,
                if check.passed { "PASS" } else { "FAIL" },
                check.detail
            );
        }
        if !sandbox_plans.is_empty() {
            println!("Sandbox validation plans (not executed automatically):");
            for plan in &sandbox_plans {
                println!("{}	{}	{}", plan.runtime, plan.manifest, plan.suggested_command);
            }
        }
    }
    if report.passed() {
        Ok(())
    } else {
        Err("Verification failed".to_string())
    }
}

fn command_runtime(arguments: &[String]) -> Result<(), String> {
    let runtime = detect_runtime();
    if format_is_json(arguments) {
        let tools = runtime
            .tools
            .iter()
            .map(|tool| {
                format!(
                    "{{\"name\":{},\"executable\":{},\"available\":{},\"version\":{}}}",
                    json_string(&tool.name),
                    json_string(&tool.executable),
                    tool.available,
                    tool.version
                        .as_deref()
                        .map(json_string)
                        .unwrap_or_else(|| "null".to_string())
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!("{{\"tools\":[{tools}]}}");
    } else {
        println!("Docker: {}", runtime.docker);
        println!("Java: {}", runtime.java);
        println!("Node: {}", runtime.node);
        println!("Python: {}", runtime.python);
        println!("PHP: {}", runtime.php);
        println!("Composer: {}", runtime.composer);
        for tool in runtime.tools {
            println!(
                "{}\t{}\t{}",
                tool.name,
                if tool.available { "available" } else { "missing" },
                tool.version.unwrap_or_default()
            );
        }
    }
    Ok(())
}

fn path_argument(arguments: &[String], index: usize) -> Result<PathBuf, String> {
    arguments
        .get(index)
        .map(PathBuf::from)
        .filter(|path| Path::new(path).exists())
        .ok_or_else(|| "A valid path is required".to_string())
}

fn option(arguments: &[String], name: &str) -> Result<String, String> {
    option_optional(arguments, name).ok_or_else(|| format!("{name} is required"))
}

fn option_optional(arguments: &[String], name: &str) -> Option<String> {
    arguments
        .iter()
        .position(|argument| argument == name)
        .and_then(|index| arguments.get(index + 1))
        .cloned()
}

fn format_is_json(arguments: &[String]) -> bool {
    arguments.iter().any(|argument| argument == "--json")
        || arguments.windows(2).any(|pair| {
            pair.first().is_some_and(|value| value == "--format")
                && pair.get(1).is_some_and(|value| value.eq_ignore_ascii_case("json"))
        })
}

fn error_code(error: &str) -> &'static str {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("workspace") && normalized.contains("not found") {
        "WORKSPACE_NOT_FOUND"
    } else if normalized.contains("repository") && normalized.contains("not found") {
        "REPOSITORY_NOT_FOUND"
    } else if normalized.contains("target") && normalized.contains("not found") {
        "TARGET_NOT_FOUND"
    } else if normalized.contains("required") || normalized.contains("unknown command") {
        "INVALID_ARGUMENT"
    } else if normalized.contains("valid path") || normalized.contains("no such file") {
        "PATH_NOT_FOUND"
    } else {
        "COMMAND_FAILED"
    }
}

fn json_string_list(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| json_string(value))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn json_cycles(cycles: &[Vec<String>]) -> String {
    format!(
        "[{}]",
        cycles
            .iter()
            .map(|cycle| json_string_list(cycle))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn json_option_string(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

fn json_string(value: &str) -> String {
    format!("\"{}\"", json_escape(value))
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn print_help() {
    println!("RepoSlice {}", env!("CARGO_PKG_VERSION"));
    println!("Read commands accept --format json (or --json) for machine-readable output.");
    println!("reposlice add-project <path> [--format json]");
    println!("reposlice projects [--format json]");
    println!("reposlice scan <path> [--format json]");
    println!("reposlice benchmark <path> [--runs 3] [--format json]");
    println!("reposlice add-repository <workspace-id> <path> [--format json]");
    println!("reposlice repositories <workspace-id> [--format json]");
    println!("reposlice clone-repository <workspace-id> <url> [--format json]");
    println!("reposlice create-workspace <name> [--format json]");
    println!("reposlice workspaces [--format json]");
    println!("reposlice scan-workspace <workspace-id> [--format json]");
    println!("reposlice scan-repository <workspace-id> <repository-id> [--format json]");
    println!("reposlice cached-workspace <workspace-id> [--format json]");
    println!("reposlice validate-workspace <workspace-id> [--format json]");
    println!("reposlice export-workspace <workspace-id> --format json|mermaid|graphml [--output path]");
    println!("reposlice update-repository <workspace-id> <repository-id> [--format json]");
    println!("reposlice remove-repository <workspace-id> <repository-id> [--format json]");
    println!("reposlice audit <workspace-id> [--repository <repository-id>] [--format json]");
    println!("reposlice entrypoints <path> [--format json]");
    println!("reposlice components <path> [--format json]");
    println!("reposlice graph <path> [--format json]");
    println!("reposlice slice <path> --target <id> [--format json]");
    println!("reposlice impact <path> --target <id> [--format json]");
    println!("reposlice create <path> --target <id> [--output path] [--format json]");
    println!("reposlice create-workspace-capsule <workspace-id> <repository-id> <project-unit-id> --target <id> [--output path] [--format json]");
    println!("reposlice workspace-capsules <workspace-id> [--format json]");
    println!("reposlice verify <capsule> [--format json]");
    println!("reposlice runtime [--format json]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escaping_is_machine_safe() {
        assert_eq!(json_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn classifies_structured_cli_errors() {
        assert_eq!(error_code("Target was not found"), "TARGET_NOT_FOUND");
        assert_eq!(error_code("A workspace id is required"), "INVALID_ARGUMENT");
        assert_eq!(error_code("unexpected failure"), "COMMAND_FAILED");
    }

    #[test]
    fn recognizes_json_format_switches() {
        assert!(format_is_json(&["reposlice".into(), "scan".into(), "--json".into()]));
        assert!(format_is_json(&[
            "reposlice".into(),
            "scan".into(),
            "--format".into(),
            "json".into()
        ]));
    }
}
