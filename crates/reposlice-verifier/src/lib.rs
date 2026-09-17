use reposlice_core::Sha256Hasher;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationReport {
    pub kind: String,
    pub checks: Vec<VerificationCheck>,
}

impl VerificationReport {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }

    pub fn readiness_score(&self) -> u8 {
        if self.checks.is_empty() { return 0; }
        let passed = self.checks.iter().filter(|check| check.passed).count();
        ((passed * 100) / self.checks.len()).min(100) as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxValidationPlan {
    pub runtime: String,
    pub manifest: String,
    pub suggested_command: String,
    pub network_required: bool,
    pub note: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TreeInspection {
    files: usize,
    fingerprint: String,
    sha256: String,
    symlinks: Vec<String>,
}

pub fn verify_capsule(root: &Path) -> io::Result<VerificationReport> {
    let manifest = root.join("capsule.toml");
    let source = root.join("source");
    let repositories = root.join("repositories");
    let environment = root.join("environment/required.env");

    let manifest_is_file = regular_file(&manifest);
    let environment_is_file = regular_file(&environment);
    let content = if manifest_is_file {
        fs::read_to_string(&manifest).unwrap_or_default()
    } else {
        String::new()
    };
    let source_root = if regular_directory(&source) {
        Some(source.as_path())
    } else if regular_directory(&repositories) {
        Some(repositories.as_path())
    } else {
        None
    };
    let inspection = source_root
        .map(inspect_tree)
        .transpose()?
        .unwrap_or_default();
    let payload_inspection = inspect_payload(root)?;
    let declared_files = manifest_value(&content, "metrics", "files")
        .or_else(|| manifest_value(&content, "capsule", "files"))
        .and_then(|value| value.parse::<usize>().ok());
    let target = manifest_value(&content, "target", "value");
    let declared_fingerprint = manifest_value(&content, "integrity", "source_fingerprint");
    let declared_payload_fingerprint =
        manifest_value(&content, "integrity", "payload_fingerprint");
    let declared_source_sha256 = manifest_value(&content, "integrity", "source_sha256");
    let declared_payload_sha256 = manifest_value(&content, "integrity", "payload_sha256");
    let declared_source_root = manifest_value(&content, "source", "root");
    let environment_content = if environment_is_file {
        fs::read_to_string(&environment).unwrap_or_default()
    } else {
        String::new()
    };
    let root_metadata = fs::symlink_metadata(root)?;
    let root_is_symlink = root_metadata.file_type().is_symlink();
    let manifest_is_symlink = is_symlink(&manifest);
    let environment_is_symlink = is_symlink(&environment);
    let symlink_safe = !root_is_symlink
        && !manifest_is_symlink
        && !environment_is_symlink
        && inspection.symlinks.is_empty()
        && payload_inspection.symlinks.is_empty();
    let relative_source_root = declared_source_root
        .as_deref()
        .map(|value| !Path::new(value).is_absolute())
        .unwrap_or(true);

    Ok(VerificationReport {
        kind: "structural-integrity".to_string(),
        checks: vec![
            VerificationCheck {
                name: "manifest".to_string(),
                passed: manifest_is_file && content.contains("[capsule]"),
                detail: manifest.to_string_lossy().to_string(),
            },
            VerificationCheck {
                name: "source".to_string(),
                passed: source_root.is_some() && inspection.files > 0,
                detail: format!("{} files", inspection.files),
            },
            VerificationCheck {
                name: "file-inventory".to_string(),
                passed: declared_files == Some(inspection.files),
                detail: format!(
                    "declared {}, found {}",
                    declared_files
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "missing".to_string()),
                    inspection.files
                ),
            },
            VerificationCheck {
                name: "content-integrity".to_string(),
                passed: declared_fingerprint.as_deref() == Some(inspection.fingerprint.as_str()),
                detail: format!(
                    "declared {}, found {}",
                    declared_fingerprint.unwrap_or_else(|| "missing".to_string()),
                    if inspection.fingerprint.is_empty() {
                        "unavailable".to_string()
                    } else {
                        inspection.fingerprint.clone()
                    }
                ),
            },
            VerificationCheck {
                name: "payload-integrity".to_string(),
                passed: declared_payload_fingerprint.as_deref()
                    == Some(payload_inspection.fingerprint.as_str()),
                detail: format!(
                    "declared {}, found {}",
                    declared_payload_fingerprint
                        .unwrap_or_else(|| "missing".to_string()),
                    if payload_inspection.fingerprint.is_empty() {
                        "unavailable".to_string()
                    } else {
                        payload_inspection.fingerprint.clone()
                    }
                ),
            },
            VerificationCheck {
                name: "content-integrity-sha256".to_string(),
                passed: declared_source_sha256
                    .as_deref()
                    .map(|declared| declared == inspection.sha256)
                    .unwrap_or(true),
                detail: match declared_source_sha256 {
                    Some(declared) => format!("declared {declared}, found {}", inspection.sha256),
                    None => "not declared (legacy capsule)".to_string(),
                },
            },
            VerificationCheck {
                name: "payload-integrity-sha256".to_string(),
                passed: declared_payload_sha256
                    .as_deref()
                    .map(|declared| declared == payload_inspection.sha256)
                    .unwrap_or(true),
                detail: match declared_payload_sha256 {
                    Some(declared) => {
                        format!("declared {declared}, found {}", payload_inspection.sha256)
                    }
                    None => "not declared (legacy capsule)".to_string(),
                },
            },
            VerificationCheck {
                name: "symlink-safety".to_string(),
                passed: symlink_safe,
                detail: if symlink_safe {
                    "no symlinks detected in capsule inputs".to_string()
                } else {
                    format!(
                        "symlinks detected: {}",
                        inspection
                            .symlinks
                            .iter()
                            .chain(payload_inspection.symlinks.iter())
                            .take(8)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
            },
            VerificationCheck {
                name: "portable-source-root".to_string(),
                passed: relative_source_root,
                detail: declared_source_root.unwrap_or_else(|| {
                    "workspace capsule uses repository-relative roots".to_string()
                }),
            },
            VerificationCheck {
                name: "target".to_string(),
                passed: target.as_deref().is_some_and(|value| !value.is_empty()),
                detail: target.unwrap_or_else(|| "target missing".to_string()),
            },
            VerificationCheck {
                name: "environment".to_string(),
                passed: environment_is_file
                    && environment_content
                        .lines()
                        .any(|line| line.trim() == "REPOSLICE_CAPSULE=true"),
                detail: environment.to_string_lossy().to_string(),
            },
        ],
    })
}

pub fn sandbox_validation_plan(root: &Path) -> io::Result<Vec<SandboxValidationPlan>> {
    let source = if regular_directory(&root.join("source")) { root.join("source") } else { root.join("repositories") };
    if !source.is_dir() { return Ok(Vec::new()); }
    let mut manifests = Vec::new();
    collect_manifests(&source, &source, &mut manifests)?;
    manifests.sort();
    manifests.dedup();
    let mut plans = Vec::new();
    for relative in manifests {
        let file_name = Path::new(&relative).file_name().and_then(|value| value.to_str()).unwrap_or("");
        let (runtime, command, network_required) = match file_name {
            "pom.xml" => ("Java/Maven", "mvn -o -DskipTests package", false),
            "build.gradle" | "build.gradle.kts" => ("Java/Gradle", "gradle --offline build -x test", false),
            "package.json" => ("Node.js", "npm test --if-present && npm run build --if-present", true),
            "Cargo.toml" => ("Rust", "cargo check --locked --offline", false),
            "composer.json" => ("PHP/Composer", "composer validate --no-check-publish", false),
            "requirements.txt" | "pyproject.toml" => ("Python", "python -m compileall .", false),
            _ => continue,
        };
        plans.push(SandboxValidationPlan {
            runtime: runtime.to_string(),
            manifest: relative,
            suggested_command: command.to_string(),
            network_required,
            note: "Plan only: execute explicitly inside an isolated container; RepoSlice verification never runs repository code automatically".to_string(),
        });
    }
    Ok(plans)
}

fn collect_manifests(root: &Path, directory: &Path, manifests: &mut Vec<String>) -> io::Result<()> {
    let mut entries = match fs::read_dir(directory) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Ok(()),
        Err(error) => return Err(error),
    };
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() { continue; }
        if metadata.is_dir() {
            collect_manifests(root, &path, manifests)?;
        } else if metadata.is_file() {
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
            if matches!(name, "pom.xml" | "build.gradle" | "build.gradle.kts" | "package.json" | "Cargo.toml" | "composer.json" | "requirements.txt" | "pyproject.toml") {
                manifests.push(path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}

fn regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

fn regular_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_dir())
        .unwrap_or(false)
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn manifest_value(content: &str, section: &str, key: &str) -> Option<String> {
    let mut current_section = "";
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            current_section = line.trim_matches(['[', ']']);
            continue;
        }
        if current_section != section {
            continue;
        }
        let Some((candidate, value)) = line.split_once('=') else {
            continue;
        };
        if candidate.trim() == key {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

fn inspect_tree(root: &Path) -> io::Result<TreeInspection> {
    inspect_tree_excluding(root, None)
}

fn inspect_payload(root: &Path) -> io::Result<TreeInspection> {
    inspect_tree_excluding(root, Some("capsule.toml"))
}

fn inspect_tree_excluding(
    root: &Path,
    excluded_relative: Option<&str>,
) -> io::Result<TreeInspection> {
    let mut files = Vec::new();
    let mut symlinks = Vec::new();
    collect_tree_files(root, root, excluded_relative, &mut files, &mut symlinks)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    symlinks.sort();
    let mut hash = 14695981039346656037u64;
    let mut sha256 = Sha256Hasher::new();
    for (relative, path) in &files {
        let bytes = fs::read(path)?;
        hash = fnv1a(hash, relative.as_bytes());
        hash = fnv1a(hash, &[0]);
        hash = fnv1a(hash, &bytes);
        hash = fnv1a(hash, &[255]);
        sha256.update(relative.as_bytes());
        sha256.update(&[0]);
        sha256.update(&bytes);
        sha256.update(&[255]);
    }
    Ok(TreeInspection {
        files: files.len(),
        fingerprint: format!("fnv1a64:{hash:016x}"),
        sha256: format!("sha256:{}", sha256.finalize_hex()),
        symlinks,
    })
}

fn collect_tree_files(
    root: &Path,
    current: &Path,
    excluded_relative: Option<&str>,
    files: &mut Vec<(String, PathBuf)>,
    symlinks: &mut Vec<String>,
) -> io::Result<()> {
    let relative = current
        .strip_prefix(root)
        .unwrap_or(current)
        .to_string_lossy()
        .replace('\\', "/");
    if excluded_relative.is_some_and(|excluded| relative == excluded) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(current)?;
    if metadata.file_type().is_symlink() {
        symlinks.push(relative);
        return Ok(());
    }
    if metadata.file_type().is_file() {
        files.push((relative, current.to_path_buf()));
        return Ok(());
    }
    if metadata.file_type().is_dir() {
        for entry in fs::read_dir(current)? {
            collect_tree_files(root, &entry?.path(), excluded_relative, files, symlinks)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_capsule_manifest(
        root: &Path,
        files: usize,
        fingerprint: &str,
        payload_fingerprint: &str,
    ) {
        fs::write(
            root.join("capsule.toml"),
            format!(
                "[capsule]\nname = \"test\"\n\n[source]\nroot = \".\"\n\n[target]\nvalue = \"component:main\"\n\n[metrics]\nfiles = {files}\n\n[integrity]\nsource_fingerprint = \"{fingerprint}\"\npayload_fingerprint = \"{payload_fingerprint}\"\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn verifies_structure_and_detects_content_drift() {
        let root = std::env::temp_dir().join(format!("reposlice-verifier-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("source")).unwrap();
        fs::create_dir_all(root.join("environment")).unwrap();
        fs::write(root.join("source/main.rs"), "fn main() {}").unwrap();
        fs::write(
            root.join("environment/required.env"),
            "REPOSLICE_CAPSULE=true\n",
        )
        .unwrap();
        let fingerprint = inspect_tree(&root.join("source")).unwrap().fingerprint;
        let payload_fingerprint = inspect_payload(&root).unwrap().fingerprint;
        write_capsule_manifest(&root, 1, &fingerprint, &payload_fingerprint);

        let report = verify_capsule(&root).unwrap();
        assert_eq!(report.kind, "structural-integrity");
        assert!(report.passed());

        fs::write(
            root.join("source/main.rs"),
            "fn main() { println!(\"changed\"); }",
        )
        .unwrap();
        let report = verify_capsule(&root).unwrap();
        assert!(!report.passed());
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "content-integrity" && !check.passed));
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "file-inventory" && check.passed));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_payload_drift_outside_source_tree() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-verifier-payload-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("source")).unwrap();
        fs::create_dir_all(root.join("environment")).unwrap();
        fs::write(root.join("source/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("package.json"), "{\"name\":\"demo\"}").unwrap();
        fs::write(
            root.join("environment/required.env"),
            "REPOSLICE_CAPSULE=true\n",
        )
        .unwrap();
        let fingerprint = inspect_tree(&root.join("source")).unwrap().fingerprint;
        let payload_fingerprint = inspect_payload(&root).unwrap().fingerprint;
        write_capsule_manifest(&root, 1, &fingerprint, &payload_fingerprint);
        assert!(verify_capsule(&root).unwrap().passed());

        fs::write(root.join("package.json"), "{\"name\":\"changed\"}").unwrap();
        let report = verify_capsule(&root).unwrap();
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "content-integrity" && check.passed));
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "payload-integrity" && !check.passed));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_absolute_source_roots() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-verifier-root-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("source")).unwrap();
        fs::create_dir_all(root.join("environment")).unwrap();
        fs::write(root.join("source/main.rs"), "fn main() {}").unwrap();
        fs::write(
            root.join("environment/required.env"),
            "REPOSLICE_CAPSULE=true\n",
        )
        .unwrap();
        let fingerprint = inspect_tree(&root.join("source")).unwrap().fingerprint;
        fs::write(
            root.join("capsule.toml"),
            format!(
                "[capsule]\nname=\"test\"\n[source]\nroot=\"/private/work\"\n[target]\nvalue=\"x\"\n[metrics]\nfiles=1\n[integrity]\nsource_fingerprint=\"{fingerprint}\"\n"
            ),
        )
        .unwrap();
        let report = verify_capsule(&root).unwrap();
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "portable-source-root" && !check.passed));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn reports_symlinks_without_following_them() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-verifier-symlink-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("source")).unwrap();
        fs::create_dir_all(root.join("environment")).unwrap();
        fs::write(root.join("source/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("outside.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(root.join("outside.txt"), root.join("source/link.txt")).unwrap();
        fs::write(
            root.join("environment/required.env"),
            "REPOSLICE_CAPSULE=true\n",
        )
        .unwrap();
        let fingerprint = inspect_tree(&root.join("source")).unwrap().fingerprint;
        let payload_fingerprint = inspect_payload(&root).unwrap().fingerprint;
        write_capsule_manifest(&root, 1, &fingerprint, &payload_fingerprint);
        let report = verify_capsule(&root).unwrap();
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "symlink-safety" && !check.passed));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn verifies_declared_sha256_and_detects_sha_drift() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-verifier-sha256-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("source")).unwrap();
        fs::create_dir_all(root.join("environment")).unwrap();
        fs::write(root.join("source/main.rs"), "fn main() {}").unwrap();
        fs::write(
            root.join("environment/required.env"),
            "REPOSLICE_CAPSULE=true\n",
        )
        .unwrap();

        let source = inspect_tree(&root.join("source")).unwrap();
        let payload = inspect_payload(&root).unwrap();
        fs::write(
            root.join("capsule.toml"),
            format!(
                "[capsule]\nname = \"test\"\n\n[source]\nroot = \".\"\n\n[target]\nvalue = \"component:main\"\n\n[metrics]\nfiles = 1\n\n[integrity]\nsource_fingerprint = \"{}\"\npayload_fingerprint = \"{}\"\nsource_sha256 = \"{}\"\npayload_sha256 = \"{}\"\n",
                source.fingerprint, payload.fingerprint, source.sha256, payload.sha256
            ),
        )
        .unwrap();

        let report = verify_capsule(&root).unwrap();
        assert!(report.passed());
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "content-integrity-sha256" && check.passed));
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "payload-integrity-sha256" && check.passed));

        fs::write(
            root.join("source/main.rs"),
            "fn main() { println!(\"changed\"); }",
        )
        .unwrap();
        let report = verify_capsule(&root).unwrap();
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "content-integrity-sha256" && !check.passed));
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "payload-integrity-sha256" && !check.passed));
        let _ = fs::remove_dir_all(root);
    }
}
