use reposlice_core::{
    slugify, stable_hash, CapsuleManifest, ProjectModel, ScopedTarget, Sha256Hasher, WorkspaceModel,
};
use reposlice_graph::{dependency_closure, scoped_node, workspace_dependency_slice};
use reposlice_model::find_component;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT_MANIFESTS: &[&str] = &[
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "gradle.properties",
    "package.json",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "bun.lockb",
    "tsconfig.json",
    "jsconfig.json",
    "angular.json",
    "nx.json",
    "next.config.js",
    "next.config.mjs",
    "next.config.ts",
    "nuxt.config.js",
    "nuxt.config.ts",
    "svelte.config.js",
    "svelte.config.ts",
    "astro.config.mjs",
    "astro.config.ts",
    "vite.config.js",
    "vite.config.ts",
    "Cargo.toml",
    "Cargo.lock",
    "pyproject.toml",
    "requirements.txt",
    "Pipfile",
    "Pipfile.lock",
    "poetry.lock",
    "manage.py",
    "go.mod",
    "go.sum",
    "composer.json",
    "composer.lock",
    "artisan",
    "Gemfile",
    "Gemfile.lock",
    "mix.exs",
    "mix.lock",
    "pubspec.yaml",
    "pubspec.lock",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapsuleSummary {
    pub path: String,
    pub target: String,
}

pub fn capsules_root(project_root: &Path) -> PathBuf {
    project_root.join(".reposlice").join("capsules")
}

pub fn create_capsule(
    model: &ProjectModel,
    target: &str,
    output_root: &Path,
) -> io::Result<PathBuf> {
    let target_component = resolve_target_component(model, target)?;
    let closure = dependency_closure(model, &target_component);
    let capsule_name = scoped_capsule_name(target, &format!("{}\0{target}", model.root));
    let final_root = output_root.join(&capsule_name);
    let capsule_root = output_root.join(format!(".{capsule_name}.{}.building", std::process::id()));

    if capsule_root.exists() {
        fs::remove_dir_all(&capsule_root)?;
    }

    fs::create_dir_all(capsule_root.join("source"))?;
    fs::create_dir_all(capsule_root.join("environment"))?;

    let selected = if closure.is_empty() {
        BTreeSet::from([target_component.clone()])
    } else {
        closure
    };

    let mut copied = BTreeSet::new();

    for component_id in selected {
        let component = find_component(model, &component_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Dependency closure references missing component: {component_id}"),
            )
        })?;
        let source = PathBuf::from(&component.file);
        let metadata = fs::symlink_metadata(&source).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "Component source file cannot be inspected: {}",
                    source.display()
                ),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Component source symlinks are not copied: {}",
                    source.display()
                ),
            ));
        }
        if !metadata.file_type().is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Component source file was not found: {}", source.display()),
            ));
        }
        if is_sensitive_source(&source, Path::new(&model.root)) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Sensitive source file cannot be copied into a capsule: {}",
                    source.display()
                ),
            ));
        }
        let source = fs::canonicalize(source)?;
        if copied.insert(source.clone()) {
            copy_preserving_root(
                Path::new(&model.root),
                &source,
                &capsule_root.join("source"),
            )?;
        }
    }

    for file_name in ROOT_MANIFESTS {
        copy_root_file_if_exists(Path::new(&model.root), &capsule_root, file_name)?;
    }
    copy_matching_root_manifests(Path::new(&model.root), &capsule_root)?;

    fs::write(
        capsule_root.join("environment/required.env"),
        "REPOSLICE_CAPSULE=true\n",
    )?;
    write_runtime_requirements(&capsule_root, model)?;

    let manifest = CapsuleManifest {
        name: capsule_name,
        version: env!("CARGO_PKG_VERSION").to_string(),
        source_root: ".".to_string(),
        git_commit: git_commit(Path::new(&model.root)),
        target: target.to_string(),
        files: copied.len(),
    };

    let source_fingerprint = tree_fingerprint(&capsule_root.join("source"))?;
    let payload_fingerprint = tree_fingerprint(&capsule_root)?;
    let source_sha256 = tree_sha256(&capsule_root.join("source"))?;
    let payload_sha256 = tree_sha256(&capsule_root)?;
    let mut manifest_content = manifest.to_toml();
    manifest_content.push_str(&format!(
        "\n[integrity]\nsource_fingerprint = \"{}\"\npayload_fingerprint = \"{}\"\nsource_sha256 = \"{}\"\npayload_sha256 = \"{}\"\n",
        source_fingerprint, payload_fingerprint, source_sha256, payload_sha256
    ));
    fs::write(capsule_root.join("capsule.toml"), manifest_content)?;

    publish_capsule(&capsule_root, &final_root)?;
    Ok(final_root)
}

pub fn create_workspace_capsule(
    workspace: &WorkspaceModel,
    target: &ScopedTarget,
    output_root: &Path,
) -> io::Result<PathBuf> {
    let repository = workspace
        .repositories
        .iter()
        .find(|repository| repository.id == target.repository_id)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Target repository was not found")
        })?;
    let unit = repository
        .project_units
        .iter()
        .find(|unit| unit.id == target.project_unit_id)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Target project unit was not found")
        })?;
    let target_components = if target.target_id == format!("project-unit:{}", unit.id) {
        unit.model
            .components
            .iter()
            .filter(|component| {
                !is_sensitive_source(Path::new(&component.file), Path::new(&unit.model.root))
            })
            .map(|component| component.id.clone())
            .collect::<Vec<_>>()
    } else {
        vec![resolve_target_component(&unit.model, &target.target_id)?]
    };
    if target_components.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Target project unit has no source components",
        ));
    }
    let starts = target_components
        .into_iter()
        .map(|component| (unit.id.clone(), component))
        .collect::<Vec<_>>();
    let closure = workspace_dependency_slice(workspace, &starts).nodes;
    let capsule_name = scoped_capsule_name(
        &target.target_id,
        &format!(
            "{}\0{}\0{}",
            target.repository_id, target.project_unit_id, target.target_id
        ),
    );
    let final_root = output_root.join(&capsule_name);
    let capsule_root = output_root.join(format!(".{capsule_name}.{}.building", std::process::id()));
    if capsule_root.exists() {
        fs::remove_dir_all(&capsule_root)?;
    }
    fs::create_dir_all(capsule_root.join("repositories"))?;
    fs::create_dir_all(capsule_root.join("environment"))?;
    let mut copied = BTreeSet::new();
    let mut involved = BTreeSet::new();
    let mut materialized = BTreeSet::new();
    for repository in &workspace.repositories {
        for unit in &repository.project_units {
            for component in &unit.model.components {
                if !closure.contains(&scoped_node(&unit.id, &component.id)) {
                    continue;
                }
                materialized.insert(scoped_node(&unit.id, &component.id));
                let source = PathBuf::from(&component.file);
                let metadata = fs::symlink_metadata(&source).map_err(|error| {
                    io::Error::new(
                        error.kind(),
                        format!(
                            "Component source file cannot be inspected: {}",
                            source.display()
                        ),
                    )
                })?;
                if metadata.file_type().is_symlink() {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!(
                            "Component source symlinks are not copied: {}",
                            source.display()
                        ),
                    ));
                }
                if !metadata.file_type().is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("Component source file was not found: {}", source.display()),
                    ));
                }
                if is_sensitive_source(&source, Path::new(&unit.model.root)) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!(
                            "Sensitive source file cannot be copied into a capsule: {}",
                            source.display()
                        ),
                    ));
                }
                let canonical_source = fs::canonicalize(&source)?;
                let canonical_root = fs::canonicalize(&repository.root)?;
                if !canonical_source.starts_with(&canonical_root) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "Component path escaped repository root",
                    ));
                }
                involved.insert(repository.id.clone());
                involved.insert(format!("{}\0{}", repository.id, unit.id));
                if copied.insert(canonical_source.clone()) {
                    copy_preserving_root(
                        &canonical_root,
                        &canonical_source,
                        &capsule_root
                            .join("repositories")
                            .join(repository_folder(&repository.name, &repository.id))
                            .join("source"),
                    )?;
                }
            }
        }
    }
    if let Some(missing) = closure.difference(&materialized).next() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Dependency closure references missing scoped component: {missing}"),
        ));
    }
    for repository in &workspace.repositories {
        let repository_destination = capsule_root
            .join("repositories")
            .join(repository_folder(&repository.name, &repository.id))
            .join("source");
        for unit in &repository.project_units {
            if !involved.contains(&format!("{}\0{}", repository.id, unit.id)) {
                continue;
            }
            copy_unit_manifests(
                Path::new(&repository.root),
                Path::new(&unit.model.root),
                &repository_destination,
                &mut copied,
            )?;
        }
    }
    fs::write(
        capsule_root.join("environment/required.env"),
        "REPOSLICE_CAPSULE=true\n",
    )?;
    write_workspace_runtime_requirements(&capsule_root, workspace, &involved)?;
    let mut manifest = format!("[capsule]\nname = \"{}\"\nversion = \"{}\"\nworkspace = \"{}\"\n\n[target]\nrepository = \"{}\"\nproject_unit = \"{}\"\nvalue = \"{}\"\n\n[metrics]\nfiles = {}\n", manifest_escape(&capsule_name), env!("CARGO_PKG_VERSION"), manifest_escape(&workspace.name), manifest_escape(&target.repository_id), manifest_escape(&target.project_unit_id), manifest_escape(&target.target_id), copied.len());
    for repository in workspace
        .repositories
        .iter()
        .filter(|repository| involved.contains(&repository.id))
    {
        let folder = repository_folder(&repository.name, &repository.id);
        manifest.push_str(&format!(
            "\n[[repositories]]\nid = \"{}\"\nname = \"{}\"\nfolder = \"{}\"\n",
            manifest_escape(&repository.id),
            manifest_escape(&repository.name),
            manifest_escape(&folder),
        ));
        if let Some(git) = &repository.git {
            if let Some(commit) = &git.commit {
                manifest.push_str(&format!("commit = \"{}\"\n", manifest_escape(commit)));
            }
            if let Some(branch) = &git.branch {
                manifest.push_str(&format!("branch = \"{}\"\n", manifest_escape(branch)));
            }
            if let Some(remote) = &git.remote {
                manifest.push_str(&format!(
                    "remote = \"{}\"\n",
                    manifest_escape(&safe_remote_for_manifest(remote))
                ));
            }
        }
        for project_unit in repository
            .project_units
            .iter()
            .filter(|unit| involved.contains(&format!("{}\0{}", repository.id, unit.id)))
        {
            let relative_root = Path::new(&project_unit.root)
                .strip_prefix(&repository.root)
                .unwrap_or(Path::new("."))
                .to_string_lossy()
                .replace('\\', "/");
            manifest.push_str(&format!(
                "\n[[project_units]]\nrepository_id = \"{}\"\nid = \"{}\"\nname = \"{}\"\nroot = \"{}\"\n",
                manifest_escape(&repository.id),
                manifest_escape(&project_unit.id),
                manifest_escape(&project_unit.name),
                manifest_escape(if relative_root.is_empty() { "." } else { &relative_root }),
            ));
        }
    }
    let source_fingerprint = tree_fingerprint(&capsule_root.join("repositories"))?;
    let payload_fingerprint = tree_fingerprint(&capsule_root)?;
    let source_sha256 = tree_sha256(&capsule_root.join("repositories"))?;
    let payload_sha256 = tree_sha256(&capsule_root)?;
    manifest.push_str(&format!(
        "\n[integrity]\nsource_fingerprint = \"{}\"\npayload_fingerprint = \"{}\"\nsource_sha256 = \"{}\"\npayload_sha256 = \"{}\"\n",
        source_fingerprint, payload_fingerprint, source_sha256, payload_sha256
    ));
    fs::write(capsule_root.join("capsule.toml"), manifest)?;
    publish_capsule(&capsule_root, &final_root)?;
    Ok(final_root)
}

pub fn list_capsules(project_root: &Path) -> io::Result<Vec<CapsuleSummary>> {
    list_capsules_in(&capsules_root(project_root))
}

pub fn list_capsules_in(root: &Path) -> io::Result<Vec<CapsuleSummary>> {
    let root = root.to_path_buf();

    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut capsules = Vec::new();

    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if !path.is_dir()
            || path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with('.'))
        {
            continue;
        }

        let manifest = path.join("capsule.toml");
        if !manifest.is_file() {
            continue;
        }

        let content = fs::read_to_string(manifest)?;
        let Some(target) = manifest_target(&content) else {
            continue;
        };

        capsules.push(CapsuleSummary {
            path: path.to_string_lossy().to_string(),
            target,
        });
    }

    capsules.sort_by(|left, right| {
        left.target
            .cmp(&right.target)
            .then(left.path.cmp(&right.path))
    });
    Ok(capsules)
}

fn resolve_target_component(model: &ProjectModel, target: &str) -> io::Result<String> {
    model
        .resolve_target_component_id(target)
        .map(str::to_string)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Target was not found in the project model",
            )
        })
}

fn manifest_target(content: &str) -> Option<String> {
    let mut in_target = false;

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with('[') && line.ends_with(']') {
            in_target = line == "[target]";
            continue;
        }

        if in_target {
            if let Some(value) = line.strip_prefix("value = ") {
                return parse_quoted_value(value.trim());
            }
        }
    }

    None
}

fn parse_quoted_value(value: &str) -> Option<String> {
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut result = String::new();
    let mut escaped = false;

    for character in value.chars() {
        if escaped {
            result.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            result.push(character);
        }
    }

    if escaped {
        result.push('\\');
    }

    Some(result)
}

fn copy_preserving_root(root: &Path, source: &Path, destination: &Path) -> io::Result<()> {
    let relative = source.strip_prefix(root).map_err(|_| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Source path escaped project root",
        )
    })?;
    let target = destination.join(relative);

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::copy(source, target)?;
    Ok(())
}

fn publish_capsule(staging: &Path, destination: &Path) -> io::Result<()> {
    if !destination.exists() {
        return fs::rename(staging, destination);
    }
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("capsule");
    let backup = destination.with_file_name(format!(".{name}.backup"));
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    fs::rename(destination, &backup)?;
    if let Err(error) = fs::rename(staging, destination) {
        let _ = fs::rename(&backup, destination);
        return Err(error);
    }
    let _ = fs::remove_dir_all(backup);
    Ok(())
}

fn copy_root_file_if_exists(root: &Path, destination: &Path, name: &str) -> io::Result<()> {
    let source = root.join(name);

    if source.is_file() {
        if fs::symlink_metadata(&source)?.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Manifest symlinks are not copied into capsules: {}",
                    source.display()
                ),
            ));
        }
        let canonical_root = fs::canonicalize(root)?;
        let canonical_source = fs::canonicalize(&source)?;
        if !canonical_source.starts_with(&canonical_root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Manifest path escaped project root",
            ));
        }
        if is_sensitive_source(&canonical_source, &canonical_root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Manifest contains sensitive material and cannot be copied into a capsule: {}",
                    canonical_source.display()
                ),
            ));
        }
        fs::copy(canonical_source, destination.join(name))?;
    }

    Ok(())
}

fn copy_matching_root_manifests(root: &Path, destination: &Path) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let extension = path.extension().and_then(|value| value.to_str());
        if path.is_file() && matches!(extension, Some("csproj" | "fsproj" | "sln")) {
            if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "Manifest symlinks are not copied into capsules: {}",
                        path.display()
                    ),
                ));
            }
            if let Some(name) = path.file_name() {
                let canonical_root = fs::canonicalize(root)?;
                let canonical_source = fs::canonicalize(&path)?;
                if !canonical_source.starts_with(&canonical_root) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "Manifest path escaped project root",
                    ));
                }
                if is_sensitive_source(&canonical_source, &canonical_root) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!(
                            "Manifest contains sensitive material and cannot be copied into a capsule: {}",
                            canonical_source.display()
                        ),
                    ));
                }
                fs::copy(canonical_source, destination.join(name))?;
            }
        }
    }
    Ok(())
}

fn write_runtime_requirements(capsule_root: &Path, model: &ProjectModel) -> io::Result<()> {
    let content = model
        .analysis
        .runtime_requirements
        .iter()
        .map(|item| {
            format!(
                "{}\t{}\t{}\t{}%",
                item.name,
                item.executable,
                item.version_hint.as_deref().unwrap_or("any"),
                item.confidence
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(capsule_root.join("environment/runtime.tsv"), content)
}

fn write_workspace_runtime_requirements(
    capsule_root: &Path,
    workspace: &WorkspaceModel,
    involved: &BTreeSet<String>,
) -> io::Result<()> {
    let mut rows = BTreeSet::new();
    for repository in &workspace.repositories {
        for unit in &repository.project_units {
            if !involved.contains(&format!("{}\0{}", repository.id, unit.id)) {
                continue;
            }
            for item in &unit.model.analysis.runtime_requirements {
                rows.insert(format!(
                    "{}\t{}\t{}\t{}\t{}\t{}%",
                    repository.id,
                    unit.id,
                    item.name,
                    item.executable,
                    item.version_hint.as_deref().unwrap_or("any"),
                    item.confidence
                ));
            }
        }
    }
    fs::write(
        capsule_root.join("environment/runtime.tsv"),
        rows.into_iter().collect::<Vec<_>>().join("\n"),
    )
}

fn copy_unit_manifests(
    repository_root: &Path,
    unit_root: &Path,
    destination: &Path,
    copied: &mut BTreeSet<PathBuf>,
) -> io::Result<()> {
    for name in ROOT_MANIFESTS {
        let source = unit_root.join(name);
        if source.is_file() {
            if fs::symlink_metadata(&source)?.file_type().is_symlink() {
                continue;
            }
            let source = fs::canonicalize(source)?;
            if !source.starts_with(repository_root) || is_sensitive_source(&source, repository_root)
            {
                continue;
            }
            if copied.insert(source.clone()) {
                copy_preserving_root(repository_root, &source, destination)?;
            }
        }
    }
    for entry in fs::read_dir(unit_root)? {
        let source = entry?.path();
        let extension = source.extension().and_then(|value| value.to_str());
        if source.is_file() && matches!(extension, Some("csproj" | "fsproj" | "sln")) {
            if fs::symlink_metadata(&source)?.file_type().is_symlink() {
                continue;
            }
            let source = fs::canonicalize(source)?;
            if !source.starts_with(repository_root) || is_sensitive_source(&source, repository_root)
            {
                continue;
            }
            if copied.insert(source.clone()) {
                copy_preserving_root(repository_root, &source, destination)?;
            }
        }
    }
    Ok(())
}

fn scoped_capsule_name(label: &str, scope: &str) -> String {
    let base = slugify(label);
    let shortened = base.chars().take(72).collect::<String>();
    let base = shortened.trim_end_matches('-');
    let base = if base.is_empty() { "capsule" } else { base };
    format!("{}-{}", base, stable_hash(scope))
}

fn git_commit(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn safe_remote_for_manifest(value: &str) -> String {
    let value = value.trim();
    if let Some((scheme, rest)) = value.split_once("://") {
        let authority_end = rest.find('/').unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let safe_authority = authority.rsplit('@').next().unwrap_or(authority);
        let suffix = rest[authority_end..].split(['?', '#']).next().unwrap_or("");
        return format!("{scheme}://{safe_authority}{suffix}");
    }
    let without_suffix = value.split(['?', '#']).next().unwrap_or(value);
    if let Some((credentials, destination)) = without_suffix.split_once('@') {
        if credentials.contains(':') && credentials != "git" {
            return destination.to_string();
        }
    }
    without_suffix.to_string()
}

fn is_sensitive_source(path: &Path, project_root: &Path) -> bool {
    let relative = path.strip_prefix(project_root).unwrap_or(path);
    let normalized = relative
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let example = name.contains("example") || name.contains("sample") || name.contains("template");
    if !example
        && (name == ".env"
            || name.starts_with(".env.")
            || matches!(
                name.as_str(),
                ".npmrc"
                    | ".netrc"
                    | "credentials"
                    | "credentials.json"
                    | "secrets.json"
                    | "secrets.yml"
                    | "secrets.yaml"
                    | "id_rsa"
                    | "id_ed25519"
                    | "service-account.json"
                    | "keystore.jks"
            )
            || matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("pem" | "key" | "p12" | "pfx")
            )
            || normalized.starts_with(".ssh/")
            || normalized.starts_with(".aws/"))
    {
        return true;
    }
    fs::File::open(path)
        .ok()
        .and_then(|file| {
            let mut bytes = Vec::new();
            file.take(8192).read_to_end(&mut bytes).ok().map(|_| bytes)
        })
        .map(|bytes| {
            let sample = String::from_utf8_lossy(&bytes);
            sample.contains("-----BEGIN PRIVATE KEY-----")
                || sample.contains("-----BEGIN RSA PRIVATE KEY-----")
                || sample.contains("-----BEGIN OPENSSH PRIVATE KEY-----")
                || (is_configuration_like(path) && contains_probable_secret(&sample))
        })
        .unwrap_or(false)
}

fn is_configuration_like(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "properties" | "toml" | "yaml" | "yml" | "json" | "ini" | "conf" | "config"
    ) || name.contains("config")
        || name.contains("settings")
        || name == "gradle.properties"
}

fn contains_probable_secret(content: &str) -> bool {
    const KEYS: &[&str] = &[
        "password",
        "passwd",
        "api_key",
        "apikey",
        "client_secret",
        "access_token",
        "auth_token",
        "private_key",
        "signing_key",
        "secret",
        "token",
    ];
    content.lines().any(|raw| {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            return false;
        }
        let Some(separator) = line.find(['=', ':']) else {
            return false;
        };
        let key = line[..separator]
            .trim()
            .trim_matches(|character: char| {
                character.is_whitespace()
                    || matches!(character, '\'' | '"' | '{' | '}' | '[' | ']' | ',')
            })
            .to_ascii_lowercase()
            .replace(['-', '.'], "_");
        let secret_key = KEYS
            .iter()
            .any(|candidate| key == *candidate || key.ends_with(&format!("_{candidate}")));
        if !secret_key {
            return false;
        }
        let value = line[separator + 1..]
            .trim()
            .trim_matches(|character: char| {
                character.is_whitespace() || matches!(character, '\'' | '"' | ',' | ';' | '}' | ']')
            })
            .trim();
        if value.len() < 8 {
            return false;
        }
        let normalized = value.to_ascii_lowercase();
        !normalized.contains("${")
            && !normalized.contains("{{")
            && !normalized.contains("example")
            && !normalized.contains("sample")
            && !normalized.contains("placeholder")
            && !normalized.contains("changeme")
            && !normalized.contains("your_")
            && !normalized.contains("your-")
            && !matches!(normalized.as_str(), "null" | "undefined" | "none")
    })
}

fn tree_fingerprint(root: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    collect_fingerprint_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hash = 14695981039346656037u64;
    for (relative, path) in files {
        hash = fnv1a(hash, relative.as_bytes());
        hash = fnv1a(hash, &[0]);
        let bytes = fs::read(path)?;
        hash = fnv1a(hash, &bytes);
        hash = fnv1a(hash, &[255]);
    }
    Ok(format!("fnv1a64:{hash:016x}"))
}

fn tree_sha256(root: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    collect_fingerprint_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256Hasher::new();
    for (relative, path) in files {
        hasher.update(relative.as_bytes());
        hasher.update(&[0]);
        hasher.update(&fs::read(path)?);
        hasher.update(&[255]);
    }
    Ok(format!("sha256:{}", hasher.finalize_hex()))
}

fn collect_fingerprint_files(
    root: &Path,
    current: &Path,
    output: &mut Vec<(String, PathBuf)>,
) -> io::Result<()> {
    if fs::symlink_metadata(current)?.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "Capsule integrity input contains a symlink: {}",
                current.display()
            ),
        ));
    }
    if current.is_file() {
        let relative = current
            .strip_prefix(root)
            .unwrap_or(current)
            .to_string_lossy()
            .replace('\\', "/");
        output.push((relative, current.to_path_buf()));
        return Ok(());
    }
    if current.is_dir() {
        for entry in fs::read_dir(current)? {
            collect_fingerprint_files(root, &entry?.path(), output)?;
        }
    }
    Ok(())
}

fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

fn repository_folder(name: &str, id: &str) -> String {
    let suffix = stable_hash(id);
    let base = slugify(name).chars().take(48).collect::<String>();
    let base = base.trim_end_matches('-');
    format!(
        "{}-{}",
        if base.is_empty() { "repository" } else { base },
        &suffix[..12]
    )
}

fn manifest_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposlice_core::{
        AnalysisMetadata, CompatibilityLevel, Component, ComponentKind, CrossProjectDependency,
        CrossProjectDependencyKind, Dependency, DependencyKind, Entrypoint, EntrypointKind,
        GitMetadata, ProjectRole, ProjectUnit, Repository, RuntimeRequirement,
    };

    #[test]
    fn reads_target_from_manifest() {
        let content = "[capsule]\nname = \"x\"\n\n[target]\nvalue = \"component:java:a.User\"\n";
        assert_eq!(
            manifest_target(content),
            Some("component:java:a.User".to_string())
        );
    }

    #[test]
    fn creates_safe_capsule_path_names() {
        assert_eq!(slugify("../../GET /api/users/{id}"), "get-api-users-id");
        assert_ne!(
            scoped_capsule_name("GET /users", "repo-a\0unit\0target"),
            scoped_capsule_name("GET /users", "repo-b\0unit\0target")
        );
        assert!(scoped_capsule_name(&"a".repeat(500), "scope").len() <= 89);
    }

    #[test]
    fn removes_credentials_from_manifest_remotes() {
        assert_eq!(
            safe_remote_for_manifest("https://user:token@example.com/org/repo.git?x=1"),
            "https://example.com/org/repo.git"
        );
    }

    #[test]
    fn fingerprint_changes_when_capsule_source_changes() {
        let root =
            std::env::temp_dir().join(format!("reposlice-fingerprint-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "one").unwrap();
        let first = tree_fingerprint(&root).unwrap();
        fs::write(root.join("a.txt"), "two").unwrap();
        let second = tree_fingerprint(&root).unwrap();
        assert_ne!(first, second);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn identifies_probable_credentials_in_configuration() {
        let root =
            std::env::temp_dir().join(format!("reposlice-secret-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let config = root.join("application.properties");
        fs::write(&config, "db.password=super-secret-value").unwrap();
        assert!(is_sensitive_source(&config, &root));
        fs::write(&config, "db.password=${DB_PASSWORD}").unwrap();
        assert!(!is_sensitive_source(&config, &root));
        let package = root.join("package.json");
        fs::write(
            &package,
            "{\n  \"jsonwebtoken\": \"9.0.2\",\n  \"name\": \"demo-app\"\n}",
        )
        .unwrap();
        assert!(!is_sensitive_source(&package, &root));
        fs::write(
            &package,
            "{\n  \"auth_token\": \"actual-secret-token-value\"\n}",
        )
        .unwrap();
        assert!(is_sensitive_source(&package, &root));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn identifies_secrets_but_allows_templates() {
        assert!(is_sensitive_source(Path::new(".env"), Path::new(".")));
        assert!(!is_sensitive_source(
            Path::new(".env.example"),
            Path::new(".")
        ));
        assert!(is_sensitive_source(
            Path::new("certificates/private.pem"),
            Path::new(".")
        ));
    }

    #[test]
    fn multi_repository_manifests_do_not_follow_symlinks() {
        let root =
            std::env::temp_dir().join(format!("reposlice-manifest-symlink-{}", std::process::id()));
        let unit = root.join("unit");
        let destination = root.join("destination");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&unit).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let secret = root.join("credentials.json");
        fs::write(&secret, "secret-token").unwrap();
        let link = unit.join("package.json");
        if create_file_symlink(&secret, &link).is_err() {
            let _ = fs::remove_dir_all(&root);
            return;
        }
        copy_unit_manifests(&root, &unit, &destination, &mut BTreeSet::new()).unwrap();
        assert!(!destination.join("unit/package.json").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn component_source_symlinks_are_rejected() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-component-symlink-{}",
            std::process::id()
        ));
        let output = root.join("capsules");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        let real_source = root.join("src/real.rs");
        let linked_source = root.join("src/linked.rs");
        fs::write(&real_source, "fn main() {}").unwrap();
        if create_file_symlink(&real_source, &linked_source).is_err() {
            let _ = fs::remove_dir_all(&root);
            return;
        }
        let model = ProjectModel {
            root: root.to_string_lossy().into_owned(),
            name: "symlink".into(),
            files: 1,
            compatibility: CompatibilityLevel::Syntax,
            technologies: vec![],
            components: vec![Component {
                id: "linked".into(),
                name: "linked".into(),
                kind: ComponentKind::Module,
                language: "Rust".into(),
                file: linked_source.to_string_lossy().into_owned(),
            }],
            entrypoints: vec![],
            dependencies: vec![],
            analysis: AnalysisMetadata::default(),
        };
        let error = create_capsule(&model, "component:linked", &output).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(!output
            .join(scoped_capsule_name(
                "component:linked",
                &format!("{}\0component:linked", model.root)
            ))
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    fn create_file_symlink(source: &Path, destination: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(source, destination)
    }

    #[cfg(windows)]
    fn create_file_symlink(source: &Path, destination: &Path) -> io::Result<()> {
        std::os::windows::fs::symlink_file(source, destination)
    }

    #[test]
    fn creates_a_traced_multi_repository_capsule() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-workspace-capsule-{}",
            std::process::id()
        ));
        let frontend_root = root.join("frontend");
        let backend_root = root.join("backend");
        fs::create_dir_all(&frontend_root).unwrap();
        fs::create_dir_all(&backend_root).unwrap();
        fs::write(frontend_root.join("client.ts"), "fetch('/health')").unwrap();
        fs::write(frontend_root.join("package.json"), "{}").unwrap();
        fs::write(backend_root.join("server.rs"), "fn health() {}").unwrap();
        fs::write(backend_root.join("Cargo.toml"), "[package]\nname='api'").unwrap();
        let frontend_root = fs::canonicalize(frontend_root).unwrap();
        let backend_root = fs::canonicalize(backend_root).unwrap();
        let frontend_file = fs::canonicalize(frontend_root.join("client.ts")).unwrap();
        let backend_file = fs::canonicalize(backend_root.join("server.rs")).unwrap();
        let frontend = ProjectUnit {
            id: "unit-front".into(),
            repository_id: "repo-front".into(),
            root: frontend_root.to_string_lossy().into_owned(),
            name: "frontend".into(),
            role: ProjectRole::Frontend,
            model: ProjectModel {
                root: frontend_root.to_string_lossy().into_owned(),
                name: "frontend".into(),
                files: 2,
                compatibility: CompatibilityLevel::Semantic,
                technologies: vec![],
                components: vec![Component {
                    id: "client".into(),
                    name: "client".into(),
                    kind: ComponentKind::Module,
                    language: "TypeScript".into(),
                    file: frontend_file.to_string_lossy().into_owned(),
                }],
                entrypoints: vec![],
                dependencies: vec![],
                analysis: AnalysisMetadata {
                    runtime_requirements: vec![RuntimeRequirement {
                        name: "Node.js".into(),
                        executable: "node".into(),
                        version_hint: None,
                        required_by: "package.json".into(),
                        confidence: 90,
                    }],
                    ..Default::default()
                },
            },
            http_calls: vec![],
        };
        let backend = ProjectUnit {
            id: "unit-back".into(),
            repository_id: "repo-back".into(),
            root: backend_root.to_string_lossy().into_owned(),
            name: "backend".into(),
            role: ProjectRole::Backend,
            model: ProjectModel {
                root: backend_root.to_string_lossy().into_owned(),
                name: "backend".into(),
                files: 2,
                compatibility: CompatibilityLevel::Framework,
                technologies: vec![],
                components: vec![
                    Component {
                        id: "controller".into(),
                        name: "controller".into(),
                        kind: ComponentKind::Controller,
                        language: "Rust".into(),
                        file: backend_file.to_string_lossy().into_owned(),
                    },
                    Component {
                        id: "service".into(),
                        name: "service".into(),
                        kind: ComponentKind::Service,
                        language: "Rust".into(),
                        file: backend_file.to_string_lossy().into_owned(),
                    },
                ],
                entrypoints: vec![Entrypoint {
                    id: "health".into(),
                    name: "GET /health".into(),
                    kind: EntrypointKind::HttpEndpoint,
                    component_id: "controller".into(),
                    file: backend_file.to_string_lossy().into_owned(),
                }],
                dependencies: vec![Dependency {
                    source_id: "controller".into(),
                    target_id: "service".into(),
                    kind: DependencyKind::Calls,
                }],
                analysis: AnalysisMetadata::default(),
            },
            http_calls: vec![],
        };
        let workspace = WorkspaceModel {
            id: "workspace".into(),
            name: "workspace".into(),
            repositories: vec![
                Repository {
                    id: "repo-front".into(),
                    name: "frontend".into(),
                    root: frontend_root.to_string_lossy().into_owned(),
                    git: Some(GitMetadata {
                        is_git_repository: true,
                        branch: Some("main".into()),
                        commit: Some("abc123".into()),
                        remote: Some("https://user:secret@example.com/org/front.git".into()),
                    }),
                    project_units: vec![frontend],
                },
                Repository {
                    id: "repo-back".into(),
                    name: "backend".into(),
                    root: backend_root.to_string_lossy().into_owned(),
                    git: None,
                    project_units: vec![backend],
                },
            ],
            cross_project_dependencies: vec![CrossProjectDependency {
                source_repository_id: "repo-front".into(),
                source_project_unit_id: "unit-front".into(),
                source_component_id: "client".into(),
                target_repository_id: "repo-back".into(),
                target_project_unit_id: "unit-back".into(),
                target_entrypoint_id: Some("health".into()),
                target_component_id: Some("controller".into()),
                kind: CrossProjectDependencyKind::Http,
                evidence: "fetch('/health')".into(),
                confidence: 100,
            }],
        };
        let capsule = create_workspace_capsule(
            &workspace,
            &ScopedTarget {
                repository_id: "repo-front".into(),
                project_unit_id: "unit-front".into(),
                target_id: "component:client".into(),
            },
            &root.join("output"),
        )
        .unwrap();
        let manifest = fs::read_to_string(capsule.join("capsule.toml")).unwrap();
        assert!(manifest.contains("commit = \"abc123\""));
        assert!(!manifest.contains("secret"));
        assert!(capsule
            .join(format!(
                "repositories/{}/source/client.ts",
                repository_folder("frontend", "repo-front")
            ))
            .is_file());
        assert!(capsule
            .join(format!(
                "repositories/{}/source/server.rs",
                repository_folder("backend", "repo-back")
            ))
            .is_file());
        assert!(capsule
            .join(format!(
                "repositories/{}/source/Cargo.toml",
                repository_folder("backend", "repo-back")
            ))
            .is_file());
        let runtime = fs::read_to_string(capsule.join("environment/runtime.tsv")).unwrap();
        assert!(runtime.contains("unit-front\tNode.js\tnode"));
        fs::remove_dir_all(root).unwrap();
    }
}
